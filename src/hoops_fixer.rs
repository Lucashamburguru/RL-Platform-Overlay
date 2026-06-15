use crate::state::AppState;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

#[derive(Clone, Debug)]
pub struct PatchDetail {
    pub desc: String,
    pub count: usize,
}

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

/// Checks if the haystack contains the needle text.
fn contains_bytes(haystack: &[u8], needle: &str) -> bool {
    let needle_bytes = needle.as_bytes();
    if needle_bytes.len() > haystack.len() {
        return false;
    }
    haystack
        .windows(needle_bytes.len())
        .any(|w| w == needle_bytes)
}

/// Scans the body byte-by-byte and replaces any occurrences of old_token with new_token.
fn replace_token(body: &[u8], old_token: &[u8], new_token: &[u8]) -> (Vec<u8>, usize) {
    let mut result = Vec::with_capacity(body.len());
    let mut count = 0;
    let mut i = 0;
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
    (result, count)
}

/// Fixes a single Rocket League replay file's legacy hoops tags and updates both header and body CRCs.
/// Returns Some((fixed_bytes, logs)) if a change was successfully made, or None if no fix was needed.
pub fn fix_single_replay(data: &[u8]) -> Option<(Vec<u8>, Vec<PatchDetail>)> {
    if data.len() < 8 {
        return None;
    }

    // Read header size (first 4 bytes)
    let mut h_sz_bytes = [0u8; 4];
    h_sz_bytes.copy_from_slice(&data[0..4]);
    let h_sz = u32::from_le_bytes(h_sz_bytes) as usize;
    if h_sz > data.len() || h_sz + 8 > data.len() {
        return None;
    }

    // Header data payload
    let h_data = &data[8..8 + h_sz];
    let h_crc = calculate_ue3_crc(h_data);

    // Body prefix position
    let body_prefix_pos = 8 + h_sz;
    if body_prefix_pos + 8 > data.len() {
        return None;
    }

    let mut body_size_bytes = [0u8; 4];
    body_size_bytes.copy_from_slice(&data[body_prefix_pos..body_prefix_pos + 4]);
    let body_size = u32::from_le_bytes(body_size_bytes) as usize;

    if body_prefix_pos + 8 + body_size > data.len() {
        return None;
    }

    // Check if this is a legacy hoops/mutator replay
    let is_hoops = contains_bytes(data, "Archetypes.Ball.Ball_BasketBall")
        || contains_bytes(data, "Archetypes.Ball.Ball_Basketball")
        || contains_bytes(data, "HoopsStadium")
        || contains_bytes(data, "hoopsStreet")
        || contains_bytes(data, "GameEvent_Basketball")
        || contains_bytes(data, ".upk");

    let body_data = &data[body_prefix_pos + 8..body_prefix_pos + 8 + body_size];
    let mut final_body_data = body_data.to_vec();
    let mut patch_details = Vec::new();

    if is_hoops {
        let candidates = get_candidates();
        for candidate in candidates {
            let (new_body, count) = replace_token(&final_body_data, &candidate.old, &candidate.new);
            if count > 0 {
                patch_details.push(PatchDetail {
                    desc: candidate.desc.clone(),
                    count,
                });
                final_body_data = new_body;
            }
        }
    }

    // Verify if changes were made or if original CRCs were mismatching
    let old_h_crc = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let old_body_crc = u32::from_le_bytes([
        data[body_prefix_pos + 4],
        data[body_prefix_pos + 5],
        data[body_prefix_pos + 6],
        data[body_prefix_pos + 7],
    ]);

    let new_body_crc = calculate_ue3_crc(&final_body_data);

    let changed = final_body_data.len() != body_size
        || final_body_data != body_data
        || h_crc != old_h_crc
        || new_body_crc != old_body_crc;

    if !changed {
        return None;
    }

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

    Some((final_replay, patch_details))
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
    tokio::task::spawn_blocking(move || {
        run_folder_fix(state);
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

        // Basic replay validation check
        let is_valid = data.len() >= 100
            && (contains_bytes(&data[0..100.min(data.len())], "RL_REPLAY")
                || contains_bytes(&data[0..100.min(data.len())], "Replay"));

        if !is_valid {
            // Skip silently or log invalid files? Let's skip invalid files as they aren't replays.
            continue;
        }

        if let Some((fixed_bytes, patches)) = fix_single_replay(&data) {
            // Replay needs patching!
            // First save backup as .replay.bak (preserving original if it already exists)
            let mut backup_path = path.clone();
            backup_path.set_extension("replay.bak");

            if !backup_path.exists()
                && let Err(e) = fs::write(&backup_path, &data)
            {
                append_log(
                    &state,
                    format!("❌ {filename}: Could not write backup ({e})"),
                );
                continue;
            }

            // Write fixed replay in place
            if let Err(e) = fs::write(&path, &fixed_bytes) {
                append_log(
                    &state,
                    format!("❌ {filename}: Could not save fixed replay ({e})"),
                );
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
    }

    set_status(
        &state,
        &format!("Success: Processed {total} files. Fixed {fixed_count} replays."),
    );
}

/// Spawns a background task to restore original replays from backup (.replay.bak) files.
pub fn start_restore_backups_task(state: Arc<AppState>) {
    tokio::task::spawn_blocking(move || {
        run_restore_backups(state);
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
    tokio::task::spawn_blocking(move || {
        run_delete_backups(state);
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
        // Build a mock replay byte buffer:
        // Bytes 0..4: Header size (little-endian u32) = 12
        // Bytes 4..8: Header CRC (little-endian u32)
        // Bytes 8..20: Header payload (12 bytes, containing "RL_REPLAY")
        // Bytes 20..24: Body size (little-endian u32)
        // Bytes 24..28: Body CRC (little-endian u32)
        // Bytes 28..: Body payload (containing a legacy hoops token)

        let header_payload = b"RL_REPLAY_123"; // 13 bytes
        let h_sz = header_payload.len();
        let h_crc = calculate_ue3_crc(header_payload);

        let old_token = make_token("Archetypes.Ball.Ball_BasketBall_Mutator");
        let _new_token = make_token("Archetypes.Ball.Ball_BasketBall");

        let mut body_payload = Vec::new();
        body_payload.extend_from_slice(b"some prefix bytes...");
        body_payload.extend_from_slice(&old_token);
        body_payload.extend_from_slice(b"...some suffix bytes");

        let body_sz = body_payload.len();
        let body_crc = calculate_ue3_crc(&body_payload);

        let mut mock_replay = Vec::new();
        mock_replay.extend_from_slice(&(h_sz as u32).to_le_bytes());
        mock_replay.extend_from_slice(&h_crc.to_le_bytes());
        mock_replay.extend_from_slice(header_payload);
        mock_replay.extend_from_slice(&(body_sz as u32).to_le_bytes());
        mock_replay.extend_from_slice(&body_crc.to_le_bytes());
        mock_replay.extend_from_slice(&body_payload);

        // Extra trailing data
        let trailing_data = b"some keyframe data at the end";
        mock_replay.extend_from_slice(trailing_data);

        // Run the fixer on the mock replay
        let result = fix_single_replay(&mock_replay);
        assert!(result.is_some(), "Expected replay to be patched");

        let (fixed_bytes, patches) = result.unwrap();
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
        assert_eq!(fixed_h_crc, h_crc);

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
        assert!(!contains_bytes(
            fixed_body_payload,
            "Archetypes.Ball.Ball_BasketBall_Mutator"
        ));
        assert!(contains_bytes(
            fixed_body_payload,
            "Archetypes.Ball.Ball_BasketBall"
        ));

        // Check trailing data is still intact
        let fixed_trailing_offset = body_prefix_pos + 8 + fixed_body_sz;
        let fixed_trailing = &fixed_bytes[fixed_trailing_offset..];
        assert_eq!(fixed_trailing, trailing_data);
    }
}
