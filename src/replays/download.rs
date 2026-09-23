use super::set_status;
use crate::state::AppState;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::AsyncWriteExt;

pub fn start_download_replay_task(state: Arc<AppState>, replay_id: String) {
    let Ok(operation_guard) = state.replays.maintenance_gate.clone().try_read_owned() else {
        return;
    };
    if state.replays.download_active.swap(true, Ordering::SeqCst) {
        set_status(&state, "Download already in progress");
        return;
    }

    // Kept inline: must clear download_active regardless of success or failure.
    tokio::spawn(async move {
        let _operation_guard = operation_guard;
        if let Err(e) = run_download_replay(state.clone(), replay_id).await {
            log::error!("Replay download execution error: {}", e);
        }
        state.replays.download_active.store(false, Ordering::SeqCst);
    });
}

pub fn format_uuid_with_dashes(s: &str) -> Option<String> {
    let clean: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if clean.len() == 32 {
        let clean = clean.to_lowercase();
        Some(format!(
            "{}-{}-{}-{}-{}",
            &clean[0..8],
            &clean[8..12],
            &clean[12..16],
            &clean[16..20],
            &clean[20..32]
        ))
    } else if clean.len() == 36 && s.contains('-') {
        Some(s.to_lowercase())
    } else {
        None
    }
}

async fn run_download_replay(state: Arc<AppState>, replay_id: String) -> Result<(), String> {
    let raw_id = replay_id.trim();
    if raw_id.is_empty() {
        set_status(&state, "Error: Replay ID is empty");
        return Ok(());
    }

    let config = state.system.config.load();
    let folder_str = config.replays_folder.trim().to_string();
    let api_key = config.ballchasing_api_key.trim().to_string();
    drop(config);

    if folder_str.is_empty() {
        set_status(&state, "Error: Replays folder unconfigured");
        return Ok(());
    }

    let replays_dir = PathBuf::from(&folder_str);
    if !replays_dir.exists() || !replays_dir.is_dir() {
        set_status(&state, "Error: Replays folder does not exist");
        return Ok(());
    }

    if api_key.is_empty() {
        set_status(&state, "Error: API key is empty");
        return Ok(());
    }

    let id_formatted = match format_uuid_with_dashes(raw_id) {
        Some(formatted) => formatted,
        None => {
            set_status(
                &state,
                "Error: Invalid Replay ID format (expected 32 hex chars or UUID with dashes)",
            );
            return Ok(());
        }
    };

    // 1. Check if the file already exists locally
    let target_filename = format!("{}.replay", id_formatted);
    let target_path = replays_dir.join(&target_filename);
    let mut invalid_existing_path = None;

    if tokio::fs::metadata(&target_path).await.is_ok() {
        if crate::replay_metadata::validate_replay_file_strict(&target_path).is_ok() {
            set_status(
                &state,
                &format!("Success: Replay {id_formatted} already exists locally"),
            );
            return Ok(());
        }
        invalid_existing_path = Some(target_path.clone());
    }

    // Also scan directory case-insensitively just to be nice
    match tokio::fs::read_dir(&replays_dir).await {
        Ok(mut entries) => {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if entry.file_name().to_str().map(|s| s.to_lowercase())
                    == Some(target_filename.to_lowercase())
                {
                    let path = entry.path();
                    if crate::replay_metadata::validate_replay_file_strict(&path).is_ok() {
                        set_status(
                            &state,
                            &format!("Success: Replay {id_formatted} already exists locally"),
                        );
                        return Ok(());
                    }
                    invalid_existing_path.get_or_insert(path);
                }
            }
        }
        Err(e) => {
            log::warn!(
                "Could not read replays directory for duplicate check ({}): {}",
                replays_dir.display(),
                e
            );
        }
    }

    set_status(&state, &format!("Downloading replay {}...", id_formatted));

    let client = &state.system.ballchasing_client;
    let url = format!("https://ballchasing.com/api/replays/{}/file", id_formatted);

    let response = client
        .get(&url)
        .header("Authorization", &api_key)
        .send()
        .await
        .map_err(|e| {
            let err_msg = format!("Network request failed: {e}");
            set_status(&state, &format!("Error: {}", err_msg));
            err_msg
        })?;

    let status = response.status();
    if status.as_u16() == 429 {
        set_status(&state, "Error: Download rate limit hit (429)");
        return Ok(());
    } else if status.as_u16() == 401 || status.as_u16() == 403 {
        set_status(&state, "Error: Invalid API key (401/403)");
        return Ok(());
    } else if status.as_u16() == 404 {
        set_status(&state, "Error: Replay not found on Ballchasing (404)");
        return Ok(());
    } else if !status.is_success() {
        let err_msg = format!("Error: Download failed (HTTP {})", status);
        set_status(&state, &err_msg);
        return Ok(());
    }

    let bytes = response.bytes().await.map_err(|e| {
        let err_msg = format!("Failed to read response bytes: {e}");
        set_status(&state, &format!("Error: {}", err_msg));
        err_msg
    })?;

    if bytes.is_empty() {
        set_status(&state, "Error: Downloaded file is empty");
        return Ok(());
    }
    crate::replay_metadata::validate_replay_bytes_strict(&bytes).map_err(|error| {
        let message = format!("Downloaded replay failed validation: {error}");
        set_status(&state, &format!("Error: {message}"));
        message
    })?;

    if let Some(invalid_path) = invalid_existing_path {
        let quarantined = quarantine_invalid_replay(&invalid_path)
            .await
            .map_err(|error| {
                let message = format!("Could not quarantine invalid replay: {error}");
                set_status(&state, &format!("Error: {message}"));
                message
            })?;
        log::warn!(
            "Moved invalid replay {} to {} before downloading replacement.",
            invalid_path.display(),
            quarantined.display()
        );
    }

    if let Err(e) = write_downloaded_replay(&replays_dir, &target_filename, &bytes).await {
        let err_msg = format!("Failed to write file to disk: {e}");
        set_status(&state, &format!("Error: {}", err_msg));
        return Err(err_msg);
    }

    set_status(&state, &format!("Success: Downloaded {}", target_filename));

    // Force refresh metadata scan so it registers immediately
    crate::replay_metadata::start_metadata_scan(state.clone(), folder_str);

    Ok(())
}

async fn quarantine_invalid_replay(path: &Path) -> Result<PathBuf, String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("invalid.replay");
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let timestamp = crate::stats_api::now_ms();
    for attempt in 0..100_u8 {
        let suffix = if attempt == 0 {
            format!("{file_name}.invalid-{timestamp}")
        } else {
            format!("{file_name}.invalid-{timestamp}-{attempt}")
        };
        let quarantine_path = parent.join(suffix);
        if tokio::fs::metadata(&quarantine_path).await.is_ok() {
            continue;
        }
        tokio::fs::rename(path, &quarantine_path)
            .await
            .map_err(|error| error.to_string())?;
        return Ok(quarantine_path);
    }
    Err("Could not allocate a unique quarantine filename.".to_string())
}

async fn write_downloaded_replay(
    replays_dir: &Path,
    target_filename: &str,
    bytes: &[u8],
) -> Result<PathBuf, String> {
    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    let target_path = replays_dir.join(target_filename);
    let temp_path = replays_dir.join(format!(
        ".{target_filename}.download-{}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = async {
        let mut file = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .await
            .map_err(|error| error.to_string())?;
        file.write_all(bytes)
            .await
            .map_err(|error| error.to_string())?;
        file.flush().await.map_err(|error| error.to_string())?;
        file.sync_all().await.map_err(|error| error.to_string())?;
        drop(file);
        tokio::fs::rename(&temp_path, &target_path)
            .await
            .map_err(|error| error.to_string())?;
        Ok(target_path.clone())
    }
    .await;

    if result.is_err() {
        let _ = tokio::fs::remove_file(&temp_path).await;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_format_uuid_with_dashes() {
        assert_eq!(
            format_uuid_with_dashes("38D82A9C4F817B27C17409AC772861F4"),
            Some("38d82a9c-4f81-7b27-c174-09ac772861f4".to_string())
        );
        assert_eq!(
            format_uuid_with_dashes("38d82a9c-4f81-7b27-c174-09ac772861f4"),
            Some("38d82a9c-4f81-7b27-c174-09ac772861f4".to_string())
        );
        assert_eq!(
            format_uuid_with_dashes("38D82A9C-4F81-7B27-C174-09AC772861F4"),
            Some("38d82a9c-4f81-7b27-c174-09ac772861f4".to_string())
        );
        assert_eq!(format_uuid_with_dashes("invalid-uuid"), None);
        assert_eq!(
            format_uuid_with_dashes("38D82A9C4F817B27C17409AC772861F"),
            None
        );
    }

    #[tokio::test]
    async fn invalid_existing_download_is_quarantined_before_atomic_replace() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        let target_name = "match.replay";
        let target = root.join(target_name);
        fs::write(&target, b"truncated").unwrap();
        assert!(crate::replay_metadata::validate_replay_file_strict(&target).is_err());

        let quarantined = quarantine_invalid_replay(&target).await.unwrap();
        let valid = super::super::valid_replay_bytes();
        assert!(crate::replay_metadata::validate_replay_bytes_strict(&valid).is_ok());
        write_downloaded_replay(root, target_name, &valid)
            .await
            .unwrap();

        assert_eq!(fs::read(&quarantined).unwrap(), b"truncated");
        assert!(crate::replay_metadata::validate_replay_file_strict(&target).is_ok());
        assert!(
            fs::read_dir(root)
                .unwrap()
                .flatten()
                .all(|entry| !entry.file_name().to_string_lossy().contains(".download-"))
        );
    }
}
