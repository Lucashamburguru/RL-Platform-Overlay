use crate::json_utils::number_field_i32;
use crate::state::{AppState, ReplayUploadProgress};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use url::Url;

const BULK_UPLOAD_DELAY_SECS: u64 = 30;
const BALLCHASING_API_BASE: &str = "https://ballchasing.com/api/";

#[derive(Clone)]
struct ReplayFile {
    filename: String,
    path: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UploadStatus {
    Uploaded,
    Duplicate,
    InvalidApiKey,
    RateLimited,
    Failed(u16),
}

enum BulkReplayOutcome {
    Continue,
    Stop(String),
}

impl UploadStatus {
    fn from_status_code(status_code: u16) -> Self {
        match status_code {
            201 => Self::Uploaded,
            409 => Self::Duplicate,
            401 | 403 => Self::InvalidApiKey,
            429 => Self::RateLimited,
            other => Self::Failed(other),
        }
    }

    fn is_cached_success(self) -> bool {
        matches!(self, Self::Uploaded | Self::Duplicate)
    }
}

/// Verifies a Ballchasing.com API token by making a GET request to the validation endpoint.
pub async fn verify_token(client: &wreq::Client, api_key: &str) -> Result<(), String> {
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
    // Kept inline: must clear auto_upload_running and surface status on error.
    tokio::spawn(async move {
        if state
            .replays
            .auto_upload_running
            .swap(true, Ordering::SeqCst)
        {
            return;
        }
        let state_clone = state.clone();
        let res = run_replay_upload(state, scan_all_as_uploaded).await;
        state_clone
            .replays
            .auto_upload_running
            .store(false, Ordering::SeqCst);
        if let Err(e) = res {
            log::error!("Replay uploader execution error: {}", e);
            set_status(&state_clone, &format!("Error: Upload trigger failed ({e})"));
        }
    });
}

/// Scans the replays folder and uploads new files to ballchasing.com.
/// If `scan_all_as_uploaded` is true, scans all existing files and caches their names to skip uploading them.
async fn run_replay_upload(state: Arc<AppState>, scan_all_as_uploaded: bool) -> Result<(), String> {
    let config = state.system.config.load();
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

    let found_files = match replay_files_in_dir(&replays_dir).await {
        Ok(files) => files,
        Err(error) => {
            log::error!("Failed to scan replays directory: {error}");
            return Ok(());
        }
    };

    if scan_all_as_uploaded {
        // Startup mode: only upload files that were modified in the last 15 minutes and are not in cache.
        // We do NOT assume older files are on ballchasing, allowing the user to upload or sync them later.
        let now = std::time::SystemTime::now();
        let api_key = config.ballchasing_api_key.trim().to_string();
        let visibility = config.ballchasing_visibility.clone();
        let uploaded_set = &config.uploaded_replays;

        if config.ballchasing_enabled && !api_key.is_empty() {
            for replay in found_files {
                let has_uploaded = uploaded_set
                    .iter()
                    .any(|x| x.eq_ignore_ascii_case(&replay.filename));
                if has_uploaded {
                    continue;
                }
                if let Ok(metadata) = tokio::fs::metadata(&replay.path).await
                    && let Ok(modified) = metadata.modified()
                    && let Ok(elapsed) = now.duration_since(modified)
                    && elapsed.as_secs() < 15 * 60
                {
                    set_status(
                        &state,
                        &format!("Checking recent file: {}", replay.filename),
                    );
                    if !wait_for_file_stability(&replay.path).await {
                        continue;
                    }
                    set_status(&state, &format!("Uploading recent {}...", replay.filename));
                    let Ok(file_bytes) = tokio::fs::read(&replay.path).await else {
                        continue;
                    };
                    if let Ok(status_code) = upload_file_to_ballchasing(
                        &state.system.ballchasing_client,
                        &api_key,
                        &visibility,
                        &replay.filename,
                        file_bytes,
                    )
                    .await
                        && UploadStatus::from_status_code(status_code).is_cached_success()
                    {
                        mark_replays_uploaded(&state, std::slice::from_ref(&replay.filename));
                        set_status(
                            &state,
                            &format!("Success: Uploaded recent {}", replay.filename),
                        );
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

    for replay in found_files {
        let has_uploaded = uploaded_set
            .iter()
            .any(|x| x.eq_ignore_ascii_case(&replay.filename));
        if has_uploaded {
            continue;
        }

        set_status(
            &state,
            &format!("Checking file stability: {}", replay.filename),
        );

        if !wait_for_file_stability(&replay.path).await {
            set_status(
                &state,
                &format!("Upload skipped (unstable file): {}", replay.filename),
            );
            continue;
        }

        set_status(&state, &format!("Uploading {}...", replay.filename));

        let Ok(file_bytes) = tokio::fs::read(&replay.path).await else {
            set_status(
                &state,
                &format!("Error: Could not read {}", replay.filename),
            );
            continue;
        };

        match upload_file_to_ballchasing(
            &state.system.ballchasing_client,
            &api_key,
            &visibility,
            &replay.filename,
            file_bytes,
        )
        .await
        {
            Ok(status_code) => match UploadStatus::from_status_code(status_code) {
                UploadStatus::Uploaded | UploadStatus::Duplicate => {
                    let success_msg = if status_code == 201 {
                        format!("Success: Uploaded {}", replay.filename)
                    } else {
                        format!(
                            "Success: Replay already on ballchasing ({})",
                            replay.filename
                        )
                    };
                    set_status(&state, &success_msg);
                    mark_replays_uploaded(&state, &[replay.filename]);
                }
                UploadStatus::InvalidApiKey => {
                    set_status(&state, "Error: Invalid API key (401/403)");
                    break;
                }
                UploadStatus::RateLimited => {
                    set_status(&state, "Error: Rate limit hit (429)");
                    break;
                }
                UploadStatus::Failed(status_code) => {
                    set_status(
                        &state,
                        &format!("Error: Upload failed with status {}", status_code),
                    );
                }
            },
            Err(err) => {
                set_status(&state, &format!("Error: {}", err));
            }
        }
    }

    Ok(())
}

async fn replay_files_in_dir(replays_dir: &Path) -> Result<Vec<ReplayFile>, std::io::Error> {
    let mut files = Vec::new();
    let mut entries = tokio::fs::read_dir(replays_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if let Ok(file_type) = entry.file_type().await
            && file_type.is_file()
            && path.extension().and_then(|s| s.to_str()) == Some("replay")
            && let Some(filename) = path.file_name().and_then(|s| s.to_str())
        {
            files.push(ReplayFile {
                filename: filename.to_string(),
                path,
            });
        }
    }
    Ok(files)
}

async fn pending_replay_files(
    replays_dir: &Path,
    uploaded_replays: &[String],
) -> Result<Vec<ReplayFile>, std::io::Error> {
    Ok(replay_files_in_dir(replays_dir)
        .await?
        .into_iter()
        .filter(|replay| {
            !uploaded_replays
                .iter()
                .any(|uploaded| uploaded.eq_ignore_ascii_case(&replay.filename))
        })
        .collect())
}

/// Uploads a single file to ballchasing.com using multipart/form-data.
async fn upload_file_to_ballchasing(
    client: &wreq::Client,
    api_key: &str,
    visibility: &str,
    filename: &str,
    file_bytes: Vec<u8>,
) -> Result<u16, String> {
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
    if tokio::fs::metadata(path).await.is_err() {
        return false;
    }
    // Sleep initially to let the game start writing the file
    tokio::time::sleep(Duration::from_secs(5)).await;

    if tokio::fs::metadata(path).await.is_err() {
        return false;
    }

    // Check if the file size remains the same (confirm writing is complete)
    let mut last_size = match tokio::fs::metadata(path).await {
        Ok(m) => m.len(),
        Err(_) => return false,
    };

    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if tokio::fs::metadata(path).await.is_err() {
            return false;
        }
        let current_size = match tokio::fs::metadata(path).await {
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
    if state.replays.upload_running.swap(true, Ordering::SeqCst) {
        set_status(&state, "Bulk upload already running");
        return;
    }

    state.replays.upload_paused.store(false, Ordering::SeqCst);
    state
        .replays
        .upload_stop_requested
        .store(false, Ordering::SeqCst);
    state
        .replays
        .upload_progress
        .store(Arc::new(ReplayUploadProgress {
            running: true,
            ..Default::default()
        }));

    // Bulk upload has its own finish/status calls inside the error path, so we use a
    // slightly customised inline spawn rather than spawn_task.
    tokio::spawn(async move {
        let state_clone = state.clone();
        if let Err(e) = run_bulk_upload(state).await {
            log::error!("Bulk upload execution error: {}", e);
            set_status(&state_clone, &format!("Error: Bulk upload failed ({e})"));
            finish_bulk_upload(&state_clone, &format!("Execution error: {e}"));
        }
    });
}

async fn run_bulk_upload(state: Arc<AppState>) -> Result<(), String> {
    let config = state.system.config.load();
    let folder_str = config.replays_folder.trim();
    if folder_str.is_empty() {
        set_status(&state, "Error: Replays folder unconfigured");
        finish_bulk_upload(&state, "Replays folder unconfigured");
        return Ok(());
    }

    let replays_dir = PathBuf::from(folder_str);
    if !replays_dir.exists() || !replays_dir.is_dir() {
        set_status(&state, "Error: Replays folder does not exist");
        finish_bulk_upload(&state, "Replays folder does not exist");
        return Ok(());
    }

    let api_key = config.ballchasing_api_key.trim().to_string();
    if api_key.is_empty() {
        set_status(&state, "Error: API key is empty");
        finish_bulk_upload(&state, "API key is empty");
        return Ok(());
    }

    let visibility = config.ballchasing_visibility.clone();
    let uploaded_replays = config.uploaded_replays.clone();
    drop(config);

    let Ok(to_upload) = pending_replay_files(&replays_dir, &uploaded_replays).await else {
        set_status(&state, "Error: Could not read directory");
        finish_bulk_upload(&state, "Could not read directory");
        return Ok(());
    };

    if to_upload.is_empty() {
        set_status(&state, "Success: All local replays already uploaded!");
        state.replays.upload_running.store(false, Ordering::SeqCst);
        state
            .replays
            .upload_progress
            .store(Arc::new(ReplayUploadProgress {
                running: false,
                recent_events: vec!["All local replays already uploaded".to_string()],
                ..Default::default()
            }));
        return Ok(());
    }

    let total = to_upload.len();
    update_upload_progress(&state, |progress| {
        progress.running = true;
        progress.total = total;
        progress.recent_events.clear();
        progress
            .recent_events
            .push(format!("Bulk upload queued {total} files"));
    });
    set_status(
        &state,
        &format!("Bulk upload started: 0/{} uploaded", total),
    );

    let mut aborted_reason: Option<String> = None;
    for (index, replay) in to_upload.into_iter().enumerate() {
        if state.replays.upload_stop_requested.load(Ordering::SeqCst) {
            set_status(&state, "Bulk upload stopped by user");
            push_upload_event(&state, "Stopped by user".to_string());
            break;
        }

        wait_while_paused(&state).await;
        if state.replays.upload_stop_requested.load(Ordering::SeqCst) {
            set_status(&state, "Bulk upload stopped by user");
            push_upload_event(&state, "Stopped by user".to_string());
            break;
        }

        // Double check configuration key is not cleared mid-run
        let current_config = state.system.config.load();
        if current_config.ballchasing_api_key.trim().is_empty() {
            set_status(&state, "Bulk upload stopped: API key cleared");
            push_upload_event(&state, "Stopped: API key cleared".to_string());
            aborted_reason = Some("API key cleared".to_string());
            break;
        }

        if let BulkReplayOutcome::Stop(reason) =
            upload_bulk_replay(&state, replay, &api_key, &visibility, index, total).await
        {
            aborted_reason = Some(reason);
            break;
        }

        // Delay to respect rate limits.
        if index + 1 < total
            && let Some(reason) = wait_between_bulk_uploads(&state, index, total).await
        {
            aborted_reason = Some(reason);
            break;
        }
    }

    let final_config = state.system.config.load();
    let current_uploaded_count = final_config.uploaded_replays.len();
    let stopped = state.replays.upload_stop_requested.load(Ordering::SeqCst);
    if stopped {
        set_status(&state, "Bulk upload stopped by user");
    } else if let Some(reason) = &aborted_reason {
        set_status(&state, &format!("Error: Bulk upload stopped ({reason})"));
    } else {
        set_status(
            &state,
            &format!(
                "Success: Bulk upload finished. Cache holds {} uploads.",
                current_uploaded_count
            ),
        );
    }
    state.replays.upload_running.store(false, Ordering::SeqCst);
    update_upload_progress(&state, |progress| {
        progress.running = false;
        progress.paused = false;
        progress.stop_requested = false;
        progress.current_file.clear();
        push_event(
            progress,
            if stopped {
                format!(
                    "Stopped: {} processed, {} uploaded, {} skipped, {} failed",
                    progress.processed, progress.uploaded, progress.skipped, progress.failed
                )
            } else if let Some(reason) = &aborted_reason {
                format!(
                    "Stopped: {reason}. {} processed, {} uploaded, {} skipped, {} failed",
                    progress.processed, progress.uploaded, progress.skipped, progress.failed
                )
            } else {
                format!(
                    "Finished: {} uploaded, {} skipped, {} failed",
                    progress.uploaded, progress.skipped, progress.failed
                )
            },
        );
    });
    Ok(())
}

async fn upload_bulk_replay(
    state: &Arc<AppState>,
    replay: ReplayFile,
    api_key: &str,
    visibility: &str,
    index: usize,
    total: usize,
) -> BulkReplayOutcome {
    update_upload_progress(state, |progress| {
        progress.current_file = replay.filename.clone();
        progress.paused = false;
        progress.stop_requested = false;
    });
    set_status(
        state,
        &format!(
            "Bulk uploading {} ({}/{})",
            replay.filename,
            index + 1,
            total
        ),
    );

    if !wait_for_file_stability(&replay.path).await {
        set_status(
            state,
            &format!("Skipped {} (unstable file)", replay.filename),
        );
        update_upload_progress(state, |progress| {
            progress.processed += 1;
            progress.skipped += 1;
            push_event(
                progress,
                format!("Skipped {}: file was still changing", replay.filename),
            );
        });
        return BulkReplayOutcome::Continue;
    }

    let Ok(file_bytes) = tokio::fs::read(&replay.path).await else {
        set_status(
            state,
            &format!(
                "Error: Could not read {} ({}/{})",
                replay.filename,
                index + 1,
                total
            ),
        );
        update_upload_progress(state, |progress| {
            progress.processed += 1;
            progress.failed += 1;
            progress.last_error = format!("Could not read {}", replay.filename);
            push_event(
                progress,
                format!("Failed {}: could not read file", replay.filename),
            );
        });
        return BulkReplayOutcome::Continue;
    };

    match upload_file_to_ballchasing(
        &state.system.ballchasing_client,
        api_key,
        visibility,
        &replay.filename,
        file_bytes,
    )
    .await
    {
        Ok(status_code) => {
            handle_bulk_upload_status(state, &replay.filename, status_code, index, total)
        }
        Err(error) => {
            set_status(
                state,
                &format!("Error uploading {}: {}", replay.filename, error),
            );
            update_upload_progress(state, |progress| {
                progress.processed += 1;
                progress.failed += 1;
                progress.last_error = format!("Error uploading {}: {error}", replay.filename);
                push_event(progress, format!("Failed {}: {error}", replay.filename));
            });
            BulkReplayOutcome::Continue
        }
    }
}

fn handle_bulk_upload_status(
    state: &Arc<AppState>,
    filename: &str,
    status_code: u16,
    index: usize,
    total: usize,
) -> BulkReplayOutcome {
    match UploadStatus::from_status_code(status_code) {
        UploadStatus::Uploaded | UploadStatus::Duplicate => {
            let upload_status = UploadStatus::from_status_code(status_code);
            mark_replays_uploaded(state, &[filename.to_string()]);
            let status_message = if upload_status == UploadStatus::Uploaded {
                format!("Uploaded {} ({}/{})", filename, index + 1, total)
            } else {
                format!(
                    "Already on ballchasing {} ({}/{})",
                    filename,
                    index + 1,
                    total
                )
            };
            set_status(state, &status_message);
            update_upload_progress(state, |progress| {
                progress.processed += 1;
                if upload_status == UploadStatus::Uploaded {
                    progress.uploaded += 1;
                    push_event(progress, format!("Uploaded {filename}"));
                } else {
                    progress.skipped += 1;
                    push_event(
                        progress,
                        format!("Skipped {filename}: already on ballchasing"),
                    );
                }
            });
            BulkReplayOutcome::Continue
        }
        UploadStatus::InvalidApiKey => {
            set_status(state, "Error: Invalid API key during bulk upload");
            update_upload_progress(state, |progress| {
                progress.processed += 1;
                progress.failed += 1;
                progress.last_error = "Invalid API key during bulk upload".to_string();
                push_event(progress, "Stopped: invalid API key".to_string());
            });
            BulkReplayOutcome::Stop("Invalid API key".to_string())
        }
        UploadStatus::RateLimited => {
            set_status(state, "Error: Rate limit hit during bulk upload");
            update_upload_progress(state, |progress| {
                progress.processed += 1;
                progress.failed += 1;
                progress.last_error = "Rate limit hit during bulk upload".to_string();
                push_event(progress, "Stopped: Ballchasing rate limit hit".to_string());
            });
            BulkReplayOutcome::Stop("Rate limit hit".to_string())
        }
        UploadStatus::Failed(status_code) => {
            set_status(
                state,
                &format!("Error: Failed status {} on {}", status_code, filename),
            );
            update_upload_progress(state, |progress| {
                progress.processed += 1;
                progress.failed += 1;
                progress.last_error =
                    format!("Ballchasing returned HTTP {status_code} for {filename}");
                push_event(progress, format!("Failed {filename}: HTTP {status_code}"));
            });
            BulkReplayOutcome::Continue
        }
    }
}

async fn wait_between_bulk_uploads(state: &AppState, index: usize, total: usize) -> Option<String> {
    for seconds in (1..=BULK_UPLOAD_DELAY_SECS).rev() {
        wait_while_paused(state).await;
        if state.replays.upload_stop_requested.load(Ordering::SeqCst) {
            set_status(state, "Bulk upload stopped by user");
            push_upload_event(state, "Stopped by user".to_string());
            return None;
        }
        if state
            .system
            .config
            .load()
            .ballchasing_api_key
            .trim()
            .is_empty()
        {
            return Some("API key cleared".to_string());
        }
        set_status(
            state,
            &format!(
                "Waiting {}s before next upload... ({}/{})",
                seconds,
                index + 1,
                total
            ),
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    None
}

pub fn set_bulk_upload_paused(state: &AppState, paused: bool) {
    state.replays.upload_paused.store(paused, Ordering::SeqCst);
    update_upload_progress(state, |progress| {
        progress.paused = paused;
        push_event(
            progress,
            if paused {
                "Paused by user".to_string()
            } else {
                "Resumed by user".to_string()
            },
        );
    });
    set_status(
        state,
        if paused {
            "Bulk upload paused"
        } else {
            "Bulk upload resumed"
        },
    );
}

pub fn stop_bulk_upload(state: &AppState) {
    state
        .replays
        .upload_stop_requested
        .store(true, Ordering::SeqCst);
    state.replays.upload_paused.store(false, Ordering::SeqCst);
    update_upload_progress(state, |progress| {
        progress.stop_requested = true;
        progress.paused = false;
        push_event(progress, "Stop requested".to_string());
    });
    set_status(state, "Stopping bulk upload...");
}

async fn wait_while_paused(state: &AppState) {
    while state.replays.upload_paused.load(Ordering::SeqCst)
        && !state.replays.upload_stop_requested.load(Ordering::SeqCst)
    {
        update_upload_progress(state, |progress| {
            progress.paused = true;
        });
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    update_upload_progress(state, |progress| {
        progress.paused = false;
    });
}

fn finish_bulk_upload(state: &AppState, reason: &str) {
    state.replays.upload_running.store(false, Ordering::SeqCst);
    update_upload_progress(state, |progress| {
        progress.running = false;
        progress.paused = false;
        progress.current_file.clear();
        progress.last_error = reason.to_string();
        push_event(progress, format!("Stopped: {reason}"));
    });
}

fn push_upload_event(state: &AppState, event: String) {
    update_upload_progress(state, |progress| push_event(progress, event));
}

fn update_upload_progress(state: &AppState, update: impl FnOnce(&mut ReplayUploadProgress)) {
    let mut progress = (**state.replays.upload_progress.load()).clone();
    progress.paused = state.replays.upload_paused.load(Ordering::SeqCst);
    progress.stop_requested = state.replays.upload_stop_requested.load(Ordering::SeqCst);
    update(&mut progress);
    state.replays.upload_progress.store(Arc::new(progress));
}

fn push_event(progress: &mut ReplayUploadProgress, event: String) {
    progress.recent_events.push(event);
    while progress.recent_events.len() > 12 {
        progress.recent_events.remove(0);
    }
}

pub fn maybe_start_initial_replay_cache_sync(state: &Arc<AppState>) -> bool {
    if state
        .system
        .config
        .load()
        .ballchasing_api_key
        .trim()
        .is_empty()
    {
        return false;
    }

    if state
        .replays
        .initial_cache_sync_started
        .swap(true, Ordering::SeqCst)
    {
        return false;
    }

    start_sync_replays_task(state.clone())
}

pub fn start_sync_replays_task(state: Arc<AppState>) -> bool {
    if state.replays.sync_running.swap(true, Ordering::SeqCst) {
        return false;
    }

    tokio::spawn(async move {
        let state_clone = state.clone();
        if let Err(e) = run_sync_replays(state).await {
            set_status(&state_clone, &format!("Error: Sync failed ({e})"));
            log::error!("Sync replays execution error: {e}");
        }
        state_clone
            .replays
            .sync_running
            .store(false, Ordering::SeqCst);
    });
    true
}

fn parse_cloud_metadata(
    item: &serde_json::Value,
) -> Option<crate::replay_metadata::ReplayMetadataEntry> {
    let id = item["id"].as_str()?;
    let filename = format!("{}.replay", id.to_lowercase());

    let display_name = item["replay_title"]
        .as_str()
        .or_else(|| item["title"].as_str())
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| id.to_string());

    let date = item["date"]
        .as_str()
        .or_else(|| item["match_date"].as_str())
        .or_else(|| item["created"].as_str())
        .unwrap_or("")
        .to_string();

    let map_name = item["map_name"]
        .as_str()
        .or_else(|| item["map_code"].as_str())
        .unwrap_or("")
        .to_string();

    let team0_score = number_field_i32(&item["blue"], &["score"]);
    let team1_score = number_field_i32(&item["orange"], &["score"]);

    let mut player_names = Vec::new();
    if let Some(players) = item["blue"]["players"].as_array() {
        for p in players {
            if let Some(name) = p["name"].as_str() {
                player_names.push(name.to_string());
            }
        }
    }
    if let Some(players) = item["orange"]["players"].as_array() {
        for p in players {
            if let Some(name) = p["name"].as_str() {
                player_names.push(name.to_string());
            }
        }
    }

    let match_type = item["playlist_name"]
        .as_str()
        .or_else(|| item["playlist_id"].as_str())
        .unwrap_or("")
        .to_string();

    let player_name = item["uploader"]["name"].as_str().unwrap_or("").to_string();

    Some(crate::replay_metadata::ReplayMetadataEntry {
        filename,
        display_name,
        date,
        map_name,
        team0_score,
        team1_score,
        player_names,
        replay_id: id.to_string(),
        file_size: 0, // Mark as cloud entry
        modified_unix_secs: None,
        error: String::new(),
        player_name,
        match_type,
    })
}

fn validated_ballchasing_api_url(raw: &str) -> Result<Url, String> {
    let base = Url::parse(BALLCHASING_API_BASE)
        .map_err(|error| format!("Ballchasing API base URL is invalid: {error}"))?;
    let url = base
        .join(raw)
        .map_err(|error| format!("Invalid Ballchasing pagination URL: {error}"))?;
    let valid_path = url.path() == "/api" || url.path().starts_with("/api/");
    if url.scheme() != "https"
        || url.host_str() != Some("ballchasing.com")
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || !valid_path
    {
        return Err(format!(
            "Rejected untrusted Ballchasing pagination URL: {url}"
        ));
    }
    Ok(url)
}

async fn run_sync_replays(state: Arc<AppState>) -> Result<(), String> {
    let config = state.system.config.load();
    let api_key = config.ballchasing_api_key.trim().to_string();
    if api_key.is_empty() {
        set_status(&state, "Error: API key is empty");
        return Ok(());
    }

    set_status(&state, "Syncing from ballchasing.com...");

    let client = &state.system.ballchasing_client;

    let mut next_url = Some(validated_ballchasing_api_url(
        "replays?uploader=me&count=200",
    )?);
    let mut fetched_ids = Vec::new();
    let mut cloud_entries = Vec::new();
    let mut pages_fetched = 0;

    // Fetch up to 500 replays (capping at 3 pages max to prevent infinite loops)
    while let Some(url) = next_url.take() {
        if pages_fetched >= 3 {
            break;
        }

        let response = client
            .get(url.as_str())
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
                if let Some(entry) = parse_cloud_metadata(item) {
                    cloud_entries.push(entry);
                }
            }
        }

        next_url = json["next"]
            .as_str()
            .map(validated_ballchasing_api_url)
            .transpose()?;
        pages_fetched += 1;

        if next_url.is_some() {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    let count = fetched_ids.len();
    state
        .replays
        .ballchasing_cloud_count
        .store(usize_to_u32_saturating(count), Ordering::SeqCst);

    let cloud_entries = cloud_entries
        .into_iter()
        .map(|entry| (entry.filename.clone(), entry))
        .collect();
    state
        .replays
        .cloud_metadata_cache
        .store(Arc::new(cloud_entries));

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

pub fn start_download_replay_task(state: Arc<AppState>, replay_id: String) {
    if state.replays.download_active.swap(true, Ordering::SeqCst) {
        set_status(&state, "Download already in progress");
        return;
    }

    // Kept inline: must clear download_active regardless of success or failure.
    tokio::spawn(async move {
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
        if crate::replay_metadata::validate_replay_file(&target_path).is_ok() {
            set_status(
                &state,
                &format!("Success: Replay {id_formatted} already exists locally"),
            );
            return Ok(());
        }
        invalid_existing_path = Some(target_path.clone());
    }

    // Also scan directory case-insensitively just to be nice
    // Bolt: Use eq_ignore_ascii_case to avoid O(N) string allocations during directory scan
    match tokio::fs::read_dir(&replays_dir).await {
        Ok(mut entries) => {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if entry
                    .file_name()
                    .to_str()
                    .is_some_and(|s| s.eq_ignore_ascii_case(&target_filename))
                {
                    let path = entry.path();
                    if crate::replay_metadata::validate_replay_file(&path).is_ok() {
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
    crate::replay_metadata::validate_replay_bytes(&bytes).map_err(|error| {
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

pub fn mark_replays_uploaded(state: &Arc<AppState>, filenames: &[String]) -> usize {
    if filenames.is_empty() {
        return 0;
    }

    state.update_config(|config| {
        let mut added = 0;
        for filename in filenames {
            if let Some(pos) = config
                .uploaded_replays
                .iter()
                .position(|x| x.eq_ignore_ascii_case(filename))
            {
                config.uploaded_replays.remove(pos);
                config.uploaded_replays.push(filename.clone());
            } else {
                config.uploaded_replays.push(filename.clone());
                added += 1;
            }
        }
        const MAX_UPLOADED_CACHE: usize = 10_000;
        if config.uploaded_replays.len() > MAX_UPLOADED_CACHE {
            let overflow = config.uploaded_replays.len() - MAX_UPLOADED_CACHE;
            config.uploaded_replays.drain(0..overflow);
        }
        added
    })
}

fn usize_to_u32_saturating(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "rl_overlay_replays_test_{name}_{}",
            crate::stats_api::now_ms()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn valid_replay_bytes() -> Vec<u8> {
        fn push_i32(bytes: &mut Vec<u8>, value: i32) {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        fn push_str(bytes: &mut Vec<u8>, value: &str) {
            push_i32(bytes, value.len() as i32 + 1);
            bytes.extend_from_slice(value.as_bytes());
            bytes.push(0);
        }

        let mut header = Vec::new();
        push_i32(&mut header, 868);
        push_i32(&mut header, 22);
        push_i32(&mut header, 10);
        push_str(&mut header, "TAGame.Replay");
        push_str(&mut header, "None");

        let mut replay = Vec::new();
        push_i32(&mut replay, header.len() as i32);
        replay.extend_from_slice(&0_u32.to_le_bytes());
        replay.extend_from_slice(&header);
        replay
    }

    #[test]
    fn test_mark_replays_uploaded() {
        let state = AppState::new();

        // Initially config should be empty
        {
            let config = state.system.config.load();
            assert!(config.uploaded_replays.is_empty());
        }

        // Add two replays
        let added =
            mark_replays_uploaded(&state, &["a.replay".to_string(), "b.replay".to_string()]);
        assert_eq!(added, 2);

        {
            let config = state.system.config.load();
            assert_eq!(config.uploaded_replays.len(), 2);
            assert!(config.uploaded_replays.contains(&"a.replay".to_string()));
            assert!(config.uploaded_replays.contains(&"b.replay".to_string()));
        }

        // Adding duplicate replays
        let added_dup = mark_replays_uploaded(&state, &["a.replay".to_string()]);
        assert_eq!(added_dup, 0);

        {
            let config = state.system.config.load();
            assert_eq!(config.uploaded_replays.len(), 2);
        }

        // Max limit of 10000 replays
        let mut massive_list = Vec::new();
        for i in 0..10100 {
            massive_list.push(format!("r{i}.replay"));
        }
        mark_replays_uploaded(&state, &massive_list);

        {
            let config = state.system.config.load();
            assert_eq!(config.uploaded_replays.len(), 10000);
            // Verify newer ones exist
            assert!(
                config
                    .uploaded_replays
                    .contains(&"r10099.replay".to_string())
            );
            // Verify older ones were pruned
            assert!(!config.uploaded_replays.contains(&"r0.replay".to_string()));
        }
    }

    #[test]
    fn parse_cloud_metadata_ignores_scores_that_do_not_fit_i32() {
        let normal = parse_cloud_metadata(&serde_json::json!({
            "id": "ABC123",
            "blue": {
                "score": 3,
                "players": [{"name": "Blue"}]
            },
            "orange": {
                "score": 2,
                "players": [{"name": "Orange"}]
            }
        }))
        .expect("cloud metadata should parse");

        assert_eq!(normal.team0_score, Some(3));
        assert_eq!(normal.team1_score, Some(2));
        assert_eq!(normal.player_names, vec!["Blue", "Orange"]);

        let oversized = parse_cloud_metadata(&serde_json::json!({
            "id": "DEF456",
            "blue": {
                "score": i32::MAX as i64 + 1
            },
            "orange": {
                "score": i32::MIN as i64 - 1
            }
        }))
        .expect("cloud metadata should parse without score fields");

        assert_eq!(oversized.team0_score, None);
        assert_eq!(oversized.team1_score, None);
    }

    #[test]
    fn usize_to_u32_saturating_caps_large_values() {
        assert_eq!(usize_to_u32_saturating(42), 42);
        assert_eq!(usize_to_u32_saturating(u32::MAX as usize), u32::MAX);
        assert_eq!(usize_to_u32_saturating(u32::MAX as usize + 1), u32::MAX);
    }

    #[tokio::test]
    async fn test_pending_replay_files() {
        let root = temp_dir("pending");

        // Create a fake replay file and a regular file
        let replay_path = root.join("match1.replay");
        fs::write(&replay_path, b"fake-replay-data").unwrap();
        fs::write(root.join("not_a_replay.txt"), b"some-text").unwrap();

        // Verify replay_files_in_dir finds it
        let files = replay_files_in_dir(&root).await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, replay_path);

        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn bulk_upload_empty_queue_clears_running_guard() {
        let root = temp_dir("bulk-empty");
        fs::write(root.join("already_uploaded.replay"), b"fake-replay").unwrap();

        let state = AppState::new();
        let current = state.system.config.load();
        let mut config = (**current).clone();
        config.replays_folder = root.to_string_lossy().to_string();
        config.ballchasing_api_key = "test-key".to_string();
        config.uploaded_replays = vec!["already_uploaded.replay".to_string()];
        state.system.config.store(Arc::new(config));
        state.replays.upload_running.store(true, Ordering::SeqCst);

        run_bulk_upload(state.clone()).await.unwrap();

        assert!(!state.replays.upload_running.load(Ordering::SeqCst));

        let _ = fs::remove_dir_all(root);
    }

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

    #[test]
    fn pagination_url_validation_stays_on_ballchasing_api() {
        assert_eq!(
            validated_ballchasing_api_url("replays?after=cursor")
                .unwrap()
                .as_str(),
            "https://ballchasing.com/api/replays?after=cursor"
        );
        assert!(
            validated_ballchasing_api_url("https://ballchasing.com/api/replays?after=cursor")
                .is_ok()
        );

        for untrusted in [
            "http://ballchasing.com/api/replays",
            "https://evil.example/api/replays",
            "https://api.ballchasing.com/api/replays",
            "https://user:password@ballchasing.com/api/replays",
            "https://ballchasing.com:444/api/replays",
            "//evil.example/api/replays",
            "https://ballchasing.com/upload",
            "https://ballchasing.com/api/replays#fragment",
        ] {
            assert!(
                validated_ballchasing_api_url(untrusted).is_err(),
                "accepted {untrusted}"
            );
        }
    }

    #[tokio::test]
    async fn invalid_existing_download_is_quarantined_before_atomic_replace() {
        let root = temp_dir("atomic-download");
        let target_name = "match.replay";
        let target = root.join(target_name);
        fs::write(&target, b"truncated").unwrap();
        assert!(crate::replay_metadata::validate_replay_file(&target).is_err());

        let quarantined = quarantine_invalid_replay(&target).await.unwrap();
        let valid = valid_replay_bytes();
        assert!(crate::replay_metadata::validate_replay_bytes(&valid).is_ok());
        write_downloaded_replay(&root, target_name, &valid)
            .await
            .unwrap();

        assert_eq!(fs::read(&quarantined).unwrap(), b"truncated");
        assert!(crate::replay_metadata::validate_replay_file(&target).is_ok());
        assert!(
            fs::read_dir(&root)
                .unwrap()
                .flatten()
                .all(|entry| !entry.file_name().to_string_lossy().contains(".download-"))
        );
        let _ = fs::remove_dir_all(root);
    }
}
