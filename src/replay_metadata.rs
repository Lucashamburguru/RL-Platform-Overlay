use crate::state::AppState;
use boxcars::{HeaderProp, ParserBuilder};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

#[derive(Clone, Debug, Default)]
pub struct ReplayMetadataSnapshot {
    pub folder: String,
    pub entries: HashMap<String, ReplayMetadataEntry>,
    pub total_files: usize,
    pub parsed: usize,
    pub failed: usize,
    pub scanned_at_unix_ms: u128,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayMetadataEntry {
    pub filename: String,
    pub display_name: String,
    pub player_name: String,
    pub player_names: Vec<String>,
    pub map_name: String,
    pub date: String,
    pub match_type: String,
    pub replay_id: String,
    pub team0_score: Option<i32>,
    pub team1_score: Option<i32>,
    pub file_size: u64,
    pub modified_unix_secs: Option<u64>,
    pub error: String,
}

impl ReplayMetadataEntry {
    pub fn has_metadata(&self) -> bool {
        self.error.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileIdentity {
    size: u64,
    modified_unix_secs: Option<u64>,
}

impl ReplayMetadataEntry {
    fn identity(&self) -> FileIdentity {
        FileIdentity {
            size: self.file_size,
            modified_unix_secs: self.modified_unix_secs,
        }
    }
}

pub fn start_metadata_scan(state: Arc<AppState>, folder: String) {
    if folder.trim().is_empty() {
        return;
    }
    if state
        .replays
        .metadata_scan_running
        .swap(true, Ordering::SeqCst)
    {
        return;
    }
    if let Ok(mut status) = state.replays.metadata_status.lock() {
        *status = "Scanning replay metadata...".to_string();
    }

    tokio::spawn(async move {
        let state_for_scan = state.clone();
        let result =
            tokio::task::spawn_blocking(move || scan_folder(&state_for_scan, &folder)).await;

        match result {
            Ok(snapshot) => {
                let status = if snapshot.failed == 0 {
                    format!("Metadata ready: {} local replays parsed", snapshot.parsed)
                } else {
                    format!(
                        "Metadata ready: {} parsed, {} failed",
                        snapshot.parsed, snapshot.failed
                    )
                };
                state.replays.metadata_cache.store(Arc::new(snapshot));
                if let Ok(mut lock) = state.replays.metadata_status.lock() {
                    *lock = status;
                }
            }
            Err(error) => {
                if let Ok(mut lock) = state.replays.metadata_status.lock() {
                    *lock = format!("Metadata scan failed: {error}");
                }
            }
        }

        state
            .replays
            .metadata_scan_running
            .store(false, Ordering::SeqCst);
    });
}

fn scan_folder(state: &AppState, folder: &str) -> ReplayMetadataSnapshot {
    let previous = state.replays.metadata_cache.load();
    let path = PathBuf::from(folder);
    let mut snapshot = ReplayMetadataSnapshot {
        folder: folder.to_string(),
        scanned_at_unix_ms: crate::stats_api::now_ms(),
        ..Default::default()
    };

    let Ok(entries) = fs::read_dir(&path) else {
        snapshot.failed = 1;
        return snapshot;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("replay") {
            continue;
        }

        snapshot.total_files += 1;
        let filename = match path.file_name().and_then(|s| s.to_str()) {
            Some(filename) => filename.to_string(),
            None => continue,
        };
        let identity = file_identity(&path);

        if previous.folder == folder
            && let Some(cached) = previous.entries.get(&filename)
            && cached.identity() == identity
        {
            if cached.has_metadata() {
                snapshot.parsed += 1;
            } else {
                snapshot.failed += 1;
            }
            snapshot.entries.insert(filename, cached.clone());
            continue;
        }

        let parsed = parse_metadata_file(&path, &filename, &identity);
        if parsed.has_metadata() {
            snapshot.parsed += 1;
        } else {
            snapshot.failed += 1;
        }
        snapshot.entries.insert(filename, parsed);
    }

    // Keep cloud metadata entries from previous scan
    for (filename, entry) in &previous.entries {
        if !snapshot.entries.contains_key(filename) && entry.file_size == 0 {
            snapshot.entries.insert(filename.clone(), entry.clone());
        }
    }

    snapshot
}

fn file_identity(path: &Path) -> FileIdentity {
    match fs::metadata(path) {
        Ok(metadata) => FileIdentity {
            size: metadata.len(),
            modified_unix_secs: metadata
                .modified()
                .ok()
                .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs()),
        },
        Err(_) => FileIdentity {
            size: 0,
            modified_unix_secs: None,
        },
    }
}

fn parse_metadata_file(
    path: &Path,
    filename: &str,
    identity: &FileIdentity,
) -> ReplayMetadataEntry {
    let mut entry = ReplayMetadataEntry {
        filename: filename.to_string(),
        file_size: identity.size,
        modified_unix_secs: identity.modified_unix_secs,
        ..Default::default()
    };

    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            entry.error = format!("Could not read replay: {error}");
            return entry;
        }
    };

    let replay = match ParserBuilder::new(&bytes)
        .on_error_check_crc()
        .never_parse_network_data()
        .parse()
    {
        Ok(replay) => replay,
        Err(error) => {
            entry.error = format!("Could not parse replay header: {error}");
            return entry;
        }
    };

    apply_properties(&mut entry, &replay.properties);
    if entry.display_name.trim().is_empty() {
        entry.display_name = filename.trim_end_matches(".replay").to_string();
    }
    entry
}

pub fn apply_properties(entry: &mut ReplayMetadataEntry, properties: &[(String, HeaderProp)]) {
    entry.player_name = string_property(properties, "PlayerName").unwrap_or_default();
    entry.player_names = player_names(properties);
    entry.map_name = string_property(properties, "MapName").unwrap_or_default();
    entry.date = string_property(properties, "Date").unwrap_or_default();
    entry.match_type = string_property(properties, "MatchType").unwrap_or_default();
    entry.replay_id = string_property(properties, "Id").unwrap_or_default();
    entry.team0_score = int_property(properties, "Team0Score");
    entry.team1_score = int_property(properties, "Team1Score");

    entry.display_name = ["ReplayName", "ReplayTitle", "Title", "Name"]
        .iter()
        .find_map(|key| string_property(properties, key))
        .or_else(|| {
            if entry.player_name.trim().is_empty() {
                None
            } else {
                Some(entry.player_name.clone())
            }
        })
        .or_else(|| entry.player_names.first().cloned())
        .unwrap_or_default();
}

fn string_property(properties: &[(String, HeaderProp)], key: &str) -> Option<String> {
    properties
        .iter()
        .find(|(name, _)| name == key)
        .and_then(|(_, prop)| prop_as_string(prop))
        .filter(|value| !value.trim().is_empty())
}

fn int_property(properties: &[(String, HeaderProp)], key: &str) -> Option<i32> {
    properties
        .iter()
        .find(|(name, _)| name == key)
        .and_then(|(_, prop)| match prop {
            HeaderProp::Int(value) => Some(*value),
            _ => None,
        })
}

fn prop_as_string(prop: &HeaderProp) -> Option<String> {
    match prop {
        HeaderProp::Name(value) | HeaderProp::Str(value) => Some(value.clone()),
        _ => None,
    }
}

fn player_names(properties: &[(String, HeaderProp)]) -> Vec<String> {
    properties
        .iter()
        .find(|(name, _)| name == "PlayerStats")
        .and_then(|(_, prop)| prop.as_array())
        .map(|players| {
            let mut names = Vec::new();
            for player in players {
                if let Some(name) = string_property(player, "Name")
                    && !names.iter().any(|existing| existing == &name)
                {
                    names.push(name);
                }
            }
            names
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn str_prop(key: &str, value: &str) -> (String, HeaderProp) {
        (key.to_string(), HeaderProp::Str(value.to_string()))
    }

    #[test]
    fn display_name_prefers_replay_title() {
        let mut entry = ReplayMetadataEntry::default();
        apply_properties(
            &mut entry,
            &[
                str_prop("ReplayName", "Game 7"),
                str_prop("PlayerName", "LocalPlayer"),
            ],
        );

        assert_eq!(entry.display_name, "Game 7");
        assert_eq!(entry.player_name, "LocalPlayer");
    }

    #[test]
    fn display_name_falls_back_to_player_name() {
        let mut entry = ReplayMetadataEntry::default();
        apply_properties(&mut entry, &[str_prop("PlayerName", "Nadir")]);

        assert_eq!(entry.display_name, "Nadir");
    }

    #[test]
    fn display_name_falls_back_to_first_player_stat_name() {
        let mut entry = ReplayMetadataEntry::default();
        let player_stats = HeaderProp::Array(vec![
            vec![str_prop("Name", "Blue One")],
            vec![str_prop("Name", "Orange One")],
        ]);
        apply_properties(&mut entry, &[("PlayerStats".to_string(), player_stats)]);

        assert_eq!(entry.display_name, "Blue One");
        assert_eq!(entry.player_names, vec!["Blue One", "Orange One"]);
    }
}
