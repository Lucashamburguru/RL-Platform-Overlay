use crate::state::AppState;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

#[derive(Clone, Debug)]
pub struct PatchDetail {
    pub desc: String,
    pub count: usize,
}

type FixedReplay = (Vec<u8>, Vec<PatchDetail>);

struct Candidate {
    old: Vec<u8>,
    new: Vec<u8>,
    desc: String,
}

/// Helper to format strings into Unreal Engine 3 serialized token formats:
/// 4-byte little-endian length (including null terminator) followed by UTF-8 bytes and null terminator.
fn make_token(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let len = (bytes.len() + 1) as i32;
    let mut token = Vec::with_capacity(4 + bytes.len() + 1);
    token.extend_from_slice(&len.to_le_bytes());
    token.extend_from_slice(bytes);
    token.push(0);
    token
}

/// Returns the static list of hoops token replacements matching the original JS script.
fn get_candidates() -> &'static [Candidate] {
    static CANDIDATES: OnceLock<Vec<Candidate>> = OnceLock::new();
    CANDIDATES.get_or_init(|| {
        vec![
            Candidate {
                old: make_token("Archetypes.Ball.Ball_BasketBall_Mutator"),
                new: make_token("Archetypes.Ball.Ball_BasketBall"),
                desc: "Ball_BasketBall_Mutator -> Ball_BasketBall".to_string(),
            },
            Candidate {
                old: make_token("Archetypes.Ball.Ball_Basketball"),
                new: make_token("Archetypes.Ball.Ball_BasketBall"),
                desc: "Ball_Basketball (legacy casing) -> Ball_BasketBall".to_string(),
            },
            Candidate {
                old: make_token("Archetypes.Ball.ball_luminousairplane"),
                new: make_token("Archetypes.Ball.Ball_BasketBall"),
                desc: "LuminousAirplane -> Ball_BasketBall".to_string(),
            },
            Candidate {
                old: make_token("Archetypes.GameEvent.GameEvent_Basketball"),
                new: make_token("GameInfo_Basketball.GameInfo.GameInfo_Basketball:Archetype"),
                desc: "GameEvent_Basketball -> GameInfo_Basketball".to_string(),
            },
            Candidate {
                old: make_token(
                    "HoopsStadium_P.TheWorld:PersistentLevel.GoalVolume_TA_2.Goal_TA_0",
                ),
                new: make_token(
                    "HoopsStadium_P.TheWorld:PersistentLevel.GoalVolume_Hoops_TA_0.Goal_Hoops_TA_0",
                ),
                desc: "GoalVolume_TA_2 -> GoalVolume_Hoops_TA_0".to_string(),
            },
            Candidate {
                old: make_token(
                    "HoopsStadium_P.TheWorld:PersistentLevel.GoalVolume_TA_3.Goal_TA_0",
                ),
                new: make_token(
                    "HoopsStadium_P.TheWorld:PersistentLevel.GoalVolume_Hoops_TA_1.Goal_Hoops_TA_0",
                ),
                desc: "GoalVolume_TA_3 -> GoalVolume_Hoops_TA_1".to_string(),
            },
            Candidate {
                old: make_token("HoopsStadium_P.upk"),
                new: make_token("HoopsStadium_P"),
                desc: "Stripped .upk from HoopsStadium_P".to_string(),
            },
            Candidate {
                old: make_token("HoopsStadium_SFX.upk"),
                new: make_token("HoopsStadium_SFX"),
                desc: "Stripped .upk from HoopsStadium_SFX".to_string(),
            },
            Candidate {
                old: make_token("GameInfo_Basketball_SF.upk"),
                new: make_token("GameInfo_Basketball"),
                desc: "Stripped .upk from GameInfo_Basketball".to_string(),
            },
        ]
    })
}

/// Generates the Unreal Engine 3 CRC table dynamically.
fn get_unreal_table() -> &'static [u32; 256] {
    static UNREAL_TABLE: OnceLock<[u32; 256]> = OnceLock::new();
    UNREAL_TABLE.get_or_init(|| {
        let mut table = [0u32; 256];
        let poly = 0x04C11DB7u32;
        for (i, item) in table.iter_mut().enumerate() {
            let mut crc = (i as u32) << 24;
            for _ in 0..8 {
                if (crc & 0x80000000) != 0 {
                    crc = (crc << 1) ^ poly;
                } else {
                    crc <<= 1;
                }
            }
            *item = crc.swap_bytes();
        }
        table
    })
}

/// Calculates the Unreal Engine 3 CRC-32 checksum for a given byte buffer.
pub fn calculate_ue3_crc(data: &[u8]) -> u32 {
    let table = get_unreal_table();
    let mut crc = 0xFE0D3410u32;
    for &byte in data {
        let table_index = ((byte as usize) ^ (crc as usize & 0xFF)) & 0xFF;
        crc = (crc >> 8) ^ table[table_index];
    }
    (!crc).swap_bytes()
}

/// Scans the body byte-by-byte and replaces any occurrences of old_token with new_token.
fn replace_token(body: &[u8], old_token: &[u8], new_token: &[u8]) -> (Option<Vec<u8>>, usize) {
    if old_token.len() > body.len() {
        return (None, 0);
    }

    // First, check if there's any match in the body
    let mut first_match = None;
    for i in 0..=body.len().saturating_sub(old_token.len()) {
        if &body[i..i + old_token.len()] == old_token {
            first_match = Some(i);
            break;
        }
    }

    let Some(start_idx) = first_match else {
        return (None, 0);
    };

    // We found a match, so we allocate a new Vec
    let mut result = Vec::with_capacity(body.len());
    result.extend_from_slice(&body[..start_idx]);
    result.extend_from_slice(new_token);

    let mut count = 1;
    let mut i = start_idx + old_token.len();
    while i < body.len() {
        if i + old_token.len() <= body.len() && &body[i..i + old_token.len()] == old_token {
            result.extend_from_slice(new_token);
            i += old_token.len();
            count += 1;
        } else {
            result.push(body[i]);
            i += 1;
        }
    }
    (Some(result), count)
}

/// Fixes a validated Rocket League replay's recognized legacy Hoops tokens and
/// updates the affected body CRC. Invalid inputs are rejected rather than
/// having their stored CRCs silently replaced.
fn fix_single_replay(data: &[u8]) -> Result<Option<FixedReplay>, String> {
    crate::replay_metadata::validate_replay_bytes_strict(data)?;

    if data.len() < 8 {
        return Err("Replay is shorter than its header prefix.".to_string());
    }

    // Read header size (first 4 bytes)
    let mut h_sz_bytes = [0u8; 4];
    h_sz_bytes.copy_from_slice(&data[0..4]);
    let h_sz = u32::from_le_bytes(h_sz_bytes) as usize;
    if h_sz > data.len() || h_sz + 8 > data.len() {
        return Err("Replay header length exceeds file size.".to_string());
    }

    // Header data payload
    let h_data = &data[8..8 + h_sz];
    let h_crc = calculate_ue3_crc(h_data);

    // Body prefix position
    let body_prefix_pos = 8 + h_sz;
    if body_prefix_pos + 8 > data.len() {
        return Err("Replay is missing its body prefix.".to_string());
    }

    let mut body_size_bytes = [0u8; 4];
    body_size_bytes.copy_from_slice(&data[body_prefix_pos..body_prefix_pos + 4]);
    let body_size = u32::from_le_bytes(body_size_bytes) as usize;

    if body_prefix_pos + 8 + body_size > data.len() {
        return Err("Replay body length exceeds file size.".to_string());
    }

    let body_data = &data[body_prefix_pos + 8..body_prefix_pos + 8 + body_size];
    let mut final_body_data = body_data.to_vec();
    let mut patch_details = Vec::new();

    for candidate in get_candidates() {
        let (new_body, count) = replace_token(&final_body_data, &candidate.old, &candidate.new);
        if count > 0 {
            patch_details.push(PatchDetail {
                desc: candidate.desc.clone(),
                count,
            });
            if let Some(nb) = new_body {
                final_body_data = nb;
            }
        }
    }

    if patch_details.is_empty() {
        return Ok(None);
    }

    let new_body_crc = calculate_ue3_crc(&final_body_data);

    // Reconstruct the final patched .replay buffer
    let new_body_size = final_body_data.len();
    let remaining_offset = body_prefix_pos + 8 + body_size;
    let remaining_data = &data[remaining_offset..];

    let mut final_replay = Vec::with_capacity(8 + h_sz + 8 + new_body_size + remaining_data.len());
    // Header size
    final_replay.extend_from_slice(&data[0..4]);
    // Header CRC (updated)
    final_replay.extend_from_slice(&h_crc.to_le_bytes());
    // Header data payload
    final_replay.extend_from_slice(h_data);
    // Body size (updated)
    final_replay.extend_from_slice(&(new_body_size as u32).to_le_bytes());
    // Body CRC (updated)
    final_replay.extend_from_slice(&new_body_crc.to_le_bytes());
    // Body data payload
    final_replay.extend_from_slice(&final_body_data);
    // Rest of stream (e.g. keyframes, net cache, etc.)
    final_replay.extend_from_slice(remaining_data);

    // netversion diagnostic check
    if final_replay.len() >= 20 {
        let mut net_ver_bytes = [0u8; 4];
        net_ver_bytes.copy_from_slice(&final_replay[16..20]);
        let net_ver = u32::from_le_bytes(net_ver_bytes);
        if net_ver == 0 {
            patch_details.push(PatchDetail {
                desc: "ANCIENT REPLAY DETECTED - NetVersion 0".to_string(),
                count: 1,
            });
        }
    }

    crate::replay_metadata::validate_replay_bytes_strict(&final_replay)
        .map_err(|error| format!("Patched replay failed validation: {error}"))?;

    Ok(Some((final_replay, patch_details)))
}

/// Creates the canonical backup once and verifies that an existing backup is
/// byte-for-byte the input about to be replaced. A stale backup blocks mutation
/// instead of giving the user a misleading recovery point.
fn ensure_matching_backup(path: &Path, data: &[u8]) -> Result<PathBuf, String> {
    let mut backup_path = path.to_path_buf();
    backup_path.set_extension("replay.bak");

    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&backup_path)
    {
        Ok(mut backup) => {
            let result = backup.write_all(data).and_then(|_| backup.sync_all());
            if let Err(error) = result {
                drop(backup);
                let _ = fs::remove_file(&backup_path);
                return Err(format!("Could not write backup: {error}"));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = fs::read(&backup_path)
                .map_err(|read_error| format!("Could not read existing backup: {read_error}"))?;
            if existing != data {
                return Err(
                    "Existing backup does not match this replay; restore, move, or delete it before retrying."
                        .to_string(),
                );
            }
        }
        Err(error) => return Err(format!("Could not create backup: {error}")),
    }

    Ok(backup_path)
}

/// Helper to set AppState's hoops fixer status.
fn set_status(state: &AppState, status: &str) {
    if let Ok(mut lock) = state.hoops_fixer.hoops_fixer_status.lock() {
        *lock = status.to_string();
    }
}

/// Helper to append logs to AppState's hoops fixer log list.
fn append_log(state: &AppState, log: String) {
    if let Ok(mut lock) = state.hoops_fixer.hoops_fixer_logs.lock() {
        lock.push(log);
    }
}

/// Helper to clear AppState's hoops fixer logs.
fn clear_logs(state: &AppState) {
    if let Ok(mut lock) = state.hoops_fixer.hoops_fixer_logs.lock() {
        lock.clear();
    }
}

/// Spawns the background task to scan the replays folder and patch legacy hoops files in place.
pub fn start_folder_fix_task(state: Arc<AppState>) {
    if state
        .hoops_fixer
        .running
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        set_status(&state, "Error: Another hoops fixer task is already running");
        return;
    }
    tokio::task::spawn_blocking(move || {
        let state_clone = state.clone();
        run_folder_fix(state);
        state_clone
            .hoops_fixer
            .running
            .store(false, std::sync::atomic::Ordering::SeqCst);
    });
}

/// Main execution routine to scan and fix the replays folder.
fn run_folder_fix(state: Arc<AppState>) {
    let folder_str = {
        let config = state.system.config.load();
        config.replays_folder.trim().to_string()
    };

    if folder_str.is_empty() {
        set_status(&state, "Error: Replays folder unconfigured");
        return;
    }

    let replays_dir = PathBuf::from(&folder_str);
    if !replays_dir.exists() || !replays_dir.is_dir() {
        set_status(&state, "Error: Replays folder does not exist");
        return;
    }

    set_status(&state, "Scanning directory...");
    clear_logs(&state);

    let entries = match fs::read_dir(&replays_dir) {
        Ok(read) => read,
        Err(e) => {
            set_status(&state, &format!("Error: Could not read folder: {e}"));
            return;
        }
    };

    let mut replays_to_check = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("replay") {
            replays_to_check.push(path);
        }
    }

    if replays_to_check.is_empty() {
        set_status(&state, "Finished: No .replay files found");
        return;
    }

    let total = replays_to_check.len();
    let mut fixed_count = 0;

    for (i, path) in replays_to_check.into_iter().enumerate() {
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();
        set_status(
            &state,
            &format!("Checking [{}/{}] {}...", i + 1, total, filename),
        );

        let data = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                append_log(&state, format!("❌ {filename}: Failed to read file ({e})"));
                continue;
            }
        };

        match fix_single_replay(&data) {
            Ok(Some((fixed_bytes, patches))) => {
                // Replay needs patching!
                if let Err(error) = ensure_matching_backup(&path, &data) {
                    append_log(&state, format!("❌ {filename}: {error}"));
                    continue;
                }

                // Write fixed replay atomically
                let tmp_path = path.with_extension("replay.tmp");
                if let Err(e) = fs::write(&tmp_path, &fixed_bytes) {
                    append_log(
                        &state,
                        format!("❌ {filename}: Could not write temp file ({e})"),
                    );
                    let _ = fs::remove_file(&tmp_path);
                    continue;
                }
                if let Err(e) = fs::rename(&tmp_path, &path) {
                    append_log(
                        &state,
                        format!(
                            "❌ {filename}: Could not rename temp file to replace original ({e})"
                        ),
                    );
                    let _ = fs::remove_file(&tmp_path);
                    continue;
                }

                fixed_count += 1;
                append_log(&state, format!("✔ Fixed {filename}:"));
                for patch in patches {
                    append_log(
                        &state,
                        format!("    * {} (applied {}x)", patch.desc, patch.count),
                    );
                }
            }
            Ok(None) => {}
            Err(error) => {
                append_log(
                    &state,
                    format!("⚠ Skipped {filename}: replay failed validation ({error})"),
                );
            }
        }
    }

    set_status(
        &state,
        &format!("Success: Processed {total} files. Fixed {fixed_count} replays."),
    );
}

/// Spawns a background task to restore original replays from backup (.replay.bak) files.
pub fn start_restore_backups_task(state: Arc<AppState>) {
    if state
        .hoops_fixer
        .running
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        set_status(&state, "Error: Another hoops fixer task is already running");
        return;
    }
    tokio::task::spawn_blocking(move || {
        let state_clone = state.clone();
        run_restore_backups(state);
        state_clone
            .hoops_fixer
            .running
            .store(false, std::sync::atomic::Ordering::SeqCst);
    });
}

fn run_restore_backups(state: Arc<AppState>) {
    let folder_str = {
        let config = state.system.config.load();
        config.replays_folder.trim().to_string()
    };

    if folder_str.is_empty() {
        set_status(&state, "Error: Replays folder unconfigured");
        return;
    }

    let replays_dir = PathBuf::from(&folder_str);
    if !replays_dir.exists() || !replays_dir.is_dir() {
        set_status(&state, "Error: Replays folder does not exist");
        return;
    }

    set_status(&state, "Restoring backups...");
    clear_logs(&state);

    let entries = match fs::read_dir(&replays_dir) {
        Ok(read) => read,
        Err(e) => {
            set_status(&state, &format!("Error: Could not read folder: {e}"));
            return;
        }
    };

    let mut restored_count = 0;
    let mut err_count = 0;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("bak") {
            let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if filename.ends_with(".replay.bak")
                && let Some(original_name) = filename.strip_suffix(".bak")
            {
                let mut original_path = path.clone();
                original_path.set_file_name(original_name);

                let backup = match fs::read(&path) {
                    Ok(backup) => backup,
                    Err(error) => {
                        append_log(
                            &state,
                            format!("❌ Failed to read backup {filename}: {error}"),
                        );
                        err_count += 1;
                        continue;
                    }
                };
                if let Err(error) = crate::replay_metadata::validate_replay_bytes_strict(&backup) {
                    append_log(
                        &state,
                        format!("❌ Refused invalid backup {filename}: {error}"),
                    );
                    err_count += 1;
                    continue;
                }

                if let Err(e) = fs::copy(&path, &original_path) {
                    append_log(&state, format!("❌ Failed to restore {original_name}: {e}"));
                    err_count += 1;
                } else {
                    restored_count += 1;
                    append_log(&state, format!("✔ Restored {original_name} from backup"));
                }
            }
        }
    }

    if err_count > 0 {
        set_status(
            &state,
            &format!("Restored {restored_count} files with {err_count} errors."),
        );
    } else {
        set_status(
            &state,
            &format!("Success: Restored {restored_count} replays from backups."),
        );
    }
}

/// Spawns a background task to clean up and delete all backup (.replay.bak) files.
pub fn start_delete_backups_task(state: Arc<AppState>) {
    if state
        .hoops_fixer
        .running
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        set_status(&state, "Error: Another hoops fixer task is already running");
        return;
    }
    tokio::task::spawn_blocking(move || {
        let state_clone = state.clone();
        run_delete_backups(state);
        state_clone
            .hoops_fixer
            .running
            .store(false, std::sync::atomic::Ordering::SeqCst);
    });
}

fn run_delete_backups(state: Arc<AppState>) {
    let folder_str = {
        let config = state.system.config.load();
        config.replays_folder.trim().to_string()
    };

    if folder_str.is_empty() {
        set_status(&state, "Error: Replays folder unconfigured");
        return;
    }

    let replays_dir = PathBuf::from(&folder_str);
    if !replays_dir.exists() || !replays_dir.is_dir() {
        set_status(&state, "Error: Replays folder does not exist");
        return;
    }

    set_status(&state, "Deleting backups...");
    clear_logs(&state);

    let entries = match fs::read_dir(&replays_dir) {
        Ok(read) => read,
        Err(e) => {
            set_status(&state, &format!("Error: Could not read folder: {e}"));
            return;
        }
    };

    let mut deleted_count = 0;
    let mut err_count = 0;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("bak") {
            let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if filename.ends_with(".replay.bak") {
                if let Err(e) = fs::remove_file(&path) {
                    append_log(&state, format!("❌ Failed to delete {filename}: {e}"));
                    err_count += 1;
                } else {
                    deleted_count += 1;
                    append_log(&state, format!("✔ Deleted backup: {filename}"));
                }
            }
        }
    }

    if err_count > 0 {
        set_status(
            &state,
            &format!("Deleted {deleted_count} files with {err_count} errors."),
        );
    } else {
        set_status(
            &state,
            &format!("Success: Deleted {deleted_count} backup files."),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_replay(package: Option<&str>) -> Vec<u8> {
        fn push_i32(bytes: &mut Vec<u8>, value: i32) {
            bytes.extend_from_slice(&value.to_le_bytes());
        }

        let mut header = Vec::new();
        push_i32(&mut header, 868);
        push_i32(&mut header, 22);
        push_i32(&mut header, 10);
        header.extend_from_slice(&make_token("TAGame.Replay"));
        header.extend_from_slice(&make_token("None"));

        let mut body = Vec::new();
        // Levels, keyframes, network data, debug info, and tick marks.
        for _ in 0..5 {
            push_i32(&mut body, 0);
        }
        if let Some(package) = package {
            push_i32(&mut body, 1);
            body.extend_from_slice(&make_token(package));
        } else {
            push_i32(&mut body, 0);
        }
        // Objects, names, class index, and net cache.
        for _ in 0..4 {
            push_i32(&mut body, 0);
        }

        let mut replay = Vec::new();
        push_i32(&mut replay, header.len() as i32);
        replay.extend_from_slice(&calculate_ue3_crc(&header).to_le_bytes());
        replay.extend_from_slice(&header);
        push_i32(&mut replay, body.len() as i32);
        replay.extend_from_slice(&calculate_ue3_crc(&body).to_le_bytes());
        replay.extend_from_slice(&body);
        replay
    }

    fn body_prefix(replay: &[u8]) -> usize {
        8 + u32::from_le_bytes(replay[..4].try_into().unwrap()) as usize
    }

    #[test]
    fn test_unreal_crc_matches_expected() {
        // Simple verification that calculate_ue3_crc produces expected values.
        // Let's test with empty slice.
        let empty_crc = calculate_ue3_crc(&[]);
        assert_eq!(empty_crc, (!0xFE0D3410u32).swap_bytes());

        // Test with custom data
        let test_data = b"hello world";
        let crc = calculate_ue3_crc(test_data);
        assert!(crc > 0);
    }

    #[test]
    fn test_fix_single_replay_patches_tokens_and_updates_crcs() {
        let mock_replay = build_replay(Some("Archetypes.Ball.Ball_BasketBall_Mutator"));
        let h_sz = u32::from_le_bytes(mock_replay[..4].try_into().unwrap()) as usize;
        let body_prefix_pos = body_prefix(&mock_replay);
        let body_sz = u32::from_le_bytes(
            mock_replay[body_prefix_pos..body_prefix_pos + 4]
                .try_into()
                .unwrap(),
        ) as usize;

        // Run the fixer on the mock replay
        let (fixed_bytes, patches) = fix_single_replay(&mock_replay)
            .unwrap()
            .expect("expected replay to be patched");
        assert_eq!(patches.len(), 1);
        assert_eq!(
            patches[0].desc,
            "Ball_BasketBall_Mutator -> Ball_BasketBall"
        );
        assert_eq!(patches[0].count, 1);

        // Verify the fixed bytes
        let fixed_h_sz = u32::from_le_bytes([
            fixed_bytes[0],
            fixed_bytes[1],
            fixed_bytes[2],
            fixed_bytes[3],
        ]) as usize;
        assert_eq!(fixed_h_sz, h_sz);

        let fixed_h_crc = u32::from_le_bytes([
            fixed_bytes[4],
            fixed_bytes[5],
            fixed_bytes[6],
            fixed_bytes[7],
        ]);
        assert_eq!(fixed_h_crc, calculate_ue3_crc(&fixed_bytes[8..8 + h_sz]));

        let body_prefix_pos = 8 + fixed_h_sz;
        let fixed_body_sz = u32::from_le_bytes([
            fixed_bytes[body_prefix_pos],
            fixed_bytes[body_prefix_pos + 1],
            fixed_bytes[body_prefix_pos + 2],
            fixed_bytes[body_prefix_pos + 3],
        ]) as usize;

        // The new token is shorter by 8 bytes (Mutator has 8 characters), so body_sz should be shorter by 8 bytes
        assert_eq!(fixed_body_sz, body_sz - 8);

        let fixed_body_crc = u32::from_le_bytes([
            fixed_bytes[body_prefix_pos + 4],
            fixed_bytes[body_prefix_pos + 5],
            fixed_bytes[body_prefix_pos + 6],
            fixed_bytes[body_prefix_pos + 7],
        ]);

        let fixed_body_payload =
            &fixed_bytes[body_prefix_pos + 8..body_prefix_pos + 8 + fixed_body_sz];
        let calculated_fixed_body_crc = calculate_ue3_crc(fixed_body_payload);
        assert_eq!(fixed_body_crc, calculated_fixed_body_crc);

        // Check that the old token was replaced by the new token
        assert!(
            !fixed_body_payload
                .windows("Archetypes.Ball.Ball_BasketBall_Mutator".len())
                .any(|window| window == b"Archetypes.Ball.Ball_BasketBall_Mutator")
        );
        assert!(
            fixed_body_payload
                .windows("Archetypes.Ball.Ball_BasketBall".len())
                .any(|window| window == b"Archetypes.Ball.Ball_BasketBall")
        );
        assert!(crate::replay_metadata::validate_replay_bytes_strict(&fixed_bytes).is_ok());
    }

    #[test]
    fn fixer_rejects_bad_header_and_body_crcs() {
        let replay = build_replay(Some("Archetypes.Ball.Ball_BasketBall_Mutator"));

        let mut bad_header = replay.clone();
        bad_header[4] ^= 1;
        assert!(fix_single_replay(&bad_header).is_err());

        let mut bad_body = replay;
        let body_prefix = body_prefix(&bad_body);
        bad_body[body_prefix + 4] ^= 1;
        assert!(fix_single_replay(&bad_body).is_err());
    }

    #[test]
    fn fixer_ignores_marker_only_and_non_hoops_replays() {
        assert!(
            fix_single_replay(&build_replay(Some("HoopsStadium_Unrelated")))
                .unwrap()
                .is_none()
        );
        assert!(
            fix_single_replay(&build_replay(Some("Stadium_P")))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn fixer_is_idempotent() {
        let replay = build_replay(Some("Archetypes.Ball.Ball_BasketBall_Mutator"));
        let (fixed, _) = fix_single_replay(&replay).unwrap().unwrap();
        assert!(fix_single_replay(&fixed).unwrap().is_none());
    }

    #[test]
    fn backup_must_match_the_replay_being_replaced() {
        let temp = tempfile::tempdir().unwrap();
        let replay_path = temp.path().join("match.replay");
        let original = build_replay(Some("Archetypes.Ball.Ball_BasketBall_Mutator"));
        fs::write(&replay_path, &original).unwrap();

        let backup = ensure_matching_backup(&replay_path, &original).unwrap();
        assert_eq!(fs::read(&backup).unwrap(), original);
        ensure_matching_backup(&replay_path, &original).unwrap();

        let different = build_replay(Some("Archetypes.Ball.Ball_Basketball"));
        assert!(ensure_matching_backup(&replay_path, &different).is_err());
    }
}
