use crate::state::AppState;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Verifies a Ballchasing.com API token by making a GET request to the validation endpoint.
pub async fn verify_token(api_key: &str) -> Result<(), String> {
    let client = wreq::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let response = client
        .get("https://ballchasing.com/api/")
        .header("Authorization", api_key)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    let status = response.status();
    if status.is_success() {
        Ok(())
    } else {
        Err(format!("Token invalid (HTTP {})", status))
    }
}

/// Helper function to spawn the asynchronous replay uploader trigger.
pub fn trigger_replay_upload(state: Arc<AppState>, scan_all_as_uploaded: bool) {
    tokio::spawn(async move {
        if let Err(e) = run_replay_upload(state, scan_all_as_uploaded).await {
            log::error!("Replay upload execution error: {}", e);
        }
    });
}

/// Scans the replays folder and uploads new files to ballchasing.com.
/// If `scan_all_as_uploaded` is true, scans all existing files and caches their names to skip uploading them.
async fn run_replay_upload(state: Arc<AppState>, scan_all_as_uploaded: bool) -> Result<(), String> {
    let config = state.config.load();
    let folder_str = config.replays_folder.trim();
    if folder_str.is_empty() {
        return Ok(());
    }

    let replays_dir = PathBuf::from(folder_str);
    if !replays_dir.exists() || !replays_dir.is_dir() {
        if !scan_all_as_uploaded && config.ballchasing_enabled {
            set_status(&state, "Error: Replays folder does not exist");
        }
        return Ok(());
    }

    // Read the directory for .replay files
    let Ok(entries) = fs::read_dir(&replays_dir) else {
        return Ok(());
    };

    let mut found_files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file()
            && path.extension().and_then(|s| s.to_str()) == Some("replay")
            && let Some(filename) = path.file_name().and_then(|s| s.to_str())
        {
            found_files.push((filename.to_string(), path));
        }
    }

    if scan_all_as_uploaded {
        // Startup mode: only upload files that were modified in the last 15 minutes and are not in cache.
        // We do NOT assume older files are on ballchasing, allowing the user to upload or sync them later.
        let now = std::time::SystemTime::now();
        let api_key = config.ballchasing_api_key.trim().to_string();
        let visibility = config.ballchasing_visibility.clone();
        let uploaded_set = &config.uploaded_replays;

        if config.ballchasing_enabled && !api_key.is_empty() {
            for (filename, path) in found_files {
                if uploaded_set.contains(&filename) {
                    continue;
                }
                if let Ok(metadata) = fs::metadata(&path)
                    && let Ok(modified) = metadata.modified()
                    && let Ok(elapsed) = now.duration_since(modified)
                    && elapsed.as_secs() < 15 * 60
                {
                    set_status(&state, &format!("Checking recent file: {}", filename));
                    if !wait_for_file_stability(&path).await {
                        continue;
                    }
                    set_status(&state, &format!("Uploading recent {}...", filename));
                    let Ok(file_bytes) = fs::read(&path) else {
                        continue;
                    };
                    if let Ok(status_code) =
                        upload_file_to_ballchasing(&api_key, &visibility, &filename, file_bytes)
                            .await
                        && (status_code == 201 || status_code == 409)
                    {
                        mark_replays_uploaded(&state, std::slice::from_ref(&filename));
                        set_status(&state, &format!("Success: Uploaded recent {}", filename));
                    }
                }
            }
        }
        return Ok(());
    }

    // Real trigger mode: upload any files not in cache
    if !config.ballchasing_enabled {
        return Ok(());
    }

    let api_key = config.ballchasing_api_key.trim().to_string();
    if api_key.is_empty() {
        set_status(&state, "Error: API key is empty");
        return Ok(());
    }

    let visibility = config.ballchasing_visibility.clone();
    let uploaded_set = &config.uploaded_replays;

    for (filename, path) in found_files {
        if uploaded_set.contains(&filename) {
            continue;
        }

        set_status(&state, &format!("Checking file stability: {}", filename));

        if !wait_for_file_stability(&path).await {
            set_status(
                &state,
                &format!("Upload skipped (unstable file): {}", filename),
            );
            continue;
        }

        set_status(&state, &format!("Uploading {}...", filename));

        let Ok(file_bytes) = fs::read(&path) else {
            set_status(&state, &format!("Error: Could not read {}", filename));
            continue;
        };

        match upload_file_to_ballchasing(&api_key, &visibility, &filename, file_bytes).await {
            Ok(status_code) => {
                if status_code == 201 || status_code == 409 {
                    // Success or duplicate
                    let success_msg = if status_code == 201 {
                        format!("Success: Uploaded {}", filename)
                    } else {
                        format!("Success: Replay already on ballchasing ({})", filename)
                    };
                    set_status(&state, &success_msg);

                    // Add to local config cache and save
                    mark_replays_uploaded(&state, &[filename]);
                } else if status_code == 401 || status_code == 403 {
                    set_status(&state, "Error: Invalid API key (401/403)");
                    break; // Stop processing further files
                } else if status_code == 429 {
                    set_status(&state, "Error: Rate limit hit (429)");
                    break;
                } else {
                    set_status(
                        &state,
                        &format!("Error: Upload failed with status {}", status_code),
                    );
                }
            }
            Err(err) => {
                set_status(&state, &format!("Error: {}", err));
            }
        }
    }

    Ok(())
}

/// Uploads a single file to ballchasing.com using multipart/form-data.
async fn upload_file_to_ballchasing(
    api_key: &str,
    visibility: &str,
    filename: &str,
    file_bytes: Vec<u8>,
) -> Result<u16, String> {
    let client = wreq::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let part = wreq::multipart::Part::bytes(file_bytes)
        .file_name(filename.to_string())
        .mime_str("application/octet-stream")
        .map_err(|e| format!("Failed to create multipart part: {e}"))?;

    let form = wreq::multipart::Form::new().part("file", part);
    let url = format!(
        "https://ballchasing.com/api/v2/upload?visibility={}",
        visibility
    );

    let response = client
        .post(&url)
        .header("Authorization", api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Network request failed: {e}"))?;

    Ok(response.status().as_u16())
}

/// Helper to set AppState's ballchasing status in a thread-safe manner.
fn set_status(state: &AppState, status: &str) {
    if let Ok(mut status_lock) = state.replays.ballchasing_status.lock() {
        *status_lock = status.to_string();
    }
}

/// Waits for a file to stabilize by sleeping initially and checking if its size stops changing.
async fn wait_for_file_stability(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    // Sleep initially to let the game start writing the file
    tokio::time::sleep(Duration::from_secs(5)).await;

    if !path.exists() {
        return false;
    }

    // Check if the file size remains the same (confirm writing is complete)
    let mut last_size = match fs::metadata(path) {
        Ok(m) => m.len(),
        Err(_) => return false,
    };

    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if !path.exists() {
            return false;
        }
        let current_size = match fs::metadata(path) {
            Ok(m) => m.len(),
            Err(_) => return false,
        };
        if current_size == last_size && current_size > 0 {
            return true;
        }
        last_size = current_size;
    }

    false
}

pub fn start_bulk_upload_task(state: Arc<AppState>) {
    tokio::spawn(async move {
        if let Err(e) = run_bulk_upload(state).await {
            log::error!("Bulk upload execution error: {}", e);
        }
    });
}

async fn run_bulk_upload(state: Arc<AppState>) -> Result<(), String> {
    let config = state.config.load();
    let folder_str = config.replays_folder.trim();
    if folder_str.is_empty() {
        set_status(&state, "Error: Replays folder unconfigured");
        return Ok(());
    }

    let replays_dir = PathBuf::from(folder_str);
    if !replays_dir.exists() || !replays_dir.is_dir() {
        set_status(&state, "Error: Replays folder does not exist");
        return Ok(());
    }

    let api_key = config.ballchasing_api_key.trim().to_string();
    if api_key.is_empty() {
        set_status(&state, "Error: API key is empty");
        return Ok(());
    }

    let visibility = config.ballchasing_visibility.clone();
    let uploaded_replays = config.uploaded_replays.clone();

    // Read directory
    let Ok(entries) = fs::read_dir(&replays_dir) else {
        set_status(&state, "Error: Could not read directory");
        return Ok(());
    };

    let mut to_upload = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file()
            && path.extension().and_then(|s| s.to_str()) == Some("replay")
            && let Some(filename) = path.file_name().and_then(|s| s.to_str())
            && !uploaded_replays.contains(filename)
        {
            to_upload.push((filename.to_string(), path));
        }
    }

    if to_upload.is_empty() {
        set_status(&state, "Success: All local replays already uploaded!");
        return Ok(());
    }

    let total = to_upload.len();
    set_status(
        &state,
        &format!("Bulk upload started: 0/{} uploaded", total),
    );

    for (index, (filename, path)) in to_upload.into_iter().enumerate() {
        // Double check configuration key is not cleared mid-run
        let current_config = state.config.load();
        if current_config.ballchasing_api_key.trim().is_empty() {
            set_status(&state, "Bulk upload stopped: API key cleared");
            break;
        }

        set_status(
            &state,
            &format!("Bulk uploading {} ({}/{})", filename, index + 1, total),
        );

        // Stability check
        if !wait_for_file_stability(&path).await {
            set_status(&state, &format!("Skipped {} (unstable file)", filename));
            continue;
        }

        let Ok(file_bytes) = fs::read(&path) else {
            set_status(
                &state,
                &format!(
                    "Error: Could not read {} ({}/{})",
                    filename,
                    index + 1,
                    total
                ),
            );
            continue;
        };

        match upload_file_to_ballchasing(&api_key, &visibility, &filename, file_bytes).await {
            Ok(status_code) => {
                if status_code == 201 || status_code == 409 {
                    // Success or Duplicate
                    mark_replays_uploaded(&state, std::slice::from_ref(&filename));
                    set_status(
                        &state,
                        &format!("Uploaded {} ({}/{})", filename, index + 1, total),
                    );
                } else if status_code == 401 || status_code == 403 {
                    set_status(&state, "Error: Invalid API key during bulk upload");
                    break;
                } else if status_code == 429 {
                    set_status(&state, "Error: Rate limit hit during bulk upload");
                    break;
                } else {
                    set_status(
                        &state,
                        &format!("Error: Failed status {} on {}", status_code, filename),
                    );
                }
            }
            Err(e) => {
                set_status(&state, &format!("Error uploading {}: {}", filename, e));
            }
        }

        // Delay to respect rate limits (30 seconds per file, since rate limit is 2 uploads per minute)
        if index + 1 < total {
            for s in (1..=30).rev() {
                // Check if key is cleared
                if state.config.load().ballchasing_api_key.trim().is_empty() {
                    break;
                }
                set_status(
                    &state,
                    &format!(
                        "Waiting {}s before next upload... ({}/{})",
                        s,
                        index + 1,
                        total
                    ),
                );
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }

    let final_config = state.config.load();
    let current_uploaded_count = final_config.uploaded_replays.len();
    set_status(
        &state,
        &format!(
            "Success: Bulk upload finished. Cache holds {} uploads.",
            current_uploaded_count
        ),
    );
    Ok(())
}

pub fn start_sync_replays_task(state: Arc<AppState>) {
    tokio::spawn(async move {
        if let Err(e) = run_sync_replays(state).await {
            log::error!("Sync replays execution error: {}", e);
        }
    });
}

async fn run_sync_replays(state: Arc<AppState>) -> Result<(), String> {
    let config = state.config.load();
    let api_key = config.ballchasing_api_key.trim().to_string();
    if api_key.is_empty() {
        set_status(&state, "Error: API key is empty");
        return Ok(());
    }

    set_status(&state, "Syncing from ballchasing.com...");

    let client = wreq::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let mut next_url =
        Some("https://ballchasing.com/api/replays?uploader=me&count=200".to_string());
    let mut fetched_ids = Vec::new();
    let mut pages_fetched = 0;

    // Fetch up to 500 replays (capping at 3 pages max to prevent infinite loops)
    while let Some(url) = next_url.take() {
        if pages_fetched >= 3 {
            break;
        }

        let response = client
            .get(&url)
            .header("Authorization", &api_key)
            .send()
            .await
            .map_err(|e| format!("Network request failed: {e}"))?;

        let status = response.status();
        if !status.is_success() {
            set_status(&state, &format!("Error: Sync failed (HTTP {})", status));
            return Ok(());
        }

        let body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read body: {e}"))?;
        let json: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| format!("Failed to parse response JSON: {e}"))?;

        if let Some(list) = json["list"].as_array() {
            for item in list {
                if let Some(id) = item["id"].as_str() {
                    fetched_ids.push(id.to_string());
                }
            }
        }

        next_url = json["next"].as_str().map(|s| s.to_string());
        pages_fetched += 1;

        if next_url.is_some() {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    let count = fetched_ids.len();
    state
        .replays
        .ballchasing_cloud_count
        .store(count as u32, std::sync::atomic::Ordering::SeqCst);

    // Update config cache with these formatted filenames
    let filenames: Vec<String> = fetched_ids
        .into_iter()
        .map(|id| format!("{}.replay", id.to_lowercase()))
        .collect();
    let added = mark_replays_uploaded(&state, &filenames);

    set_status(
        &state,
        &format!(
            "Success: Synced {} replays (added {} new to local cache)",
            count, added
        ),
    );
    Ok(())
}

pub fn mark_replays_uploaded(state: &Arc<AppState>, filenames: &[String]) -> usize {
    let config_current = state.config.load();
    let mut config_edit = (**config_current).clone();
    let mut added = 0;
    for filename in filenames {
        if config_edit.uploaded_replays.insert(filename.clone()) {
            added += 1;
        }
    }
    if added > 0 {
        while config_edit.uploaded_replays.len() > 500 {
            if let Some(to_remove) = config_edit.uploaded_replays.iter().next().cloned() {
                config_edit.uploaded_replays.remove(&to_remove);
            } else {
                break;
            }
        }
        state.save_config(config_edit);
    }
    added
}
