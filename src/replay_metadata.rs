use crate::state::AppState;
use boxcars::HeaderProp;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

const MAX_REPLAY_HEADER_BYTES: usize = 16 * 1024 * 1024;
const MAX_HEADER_PROPERTY_DEPTH: usize = 64;
const MAX_HEADER_PROPERTIES: usize = 50_000;

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
    pub players: Vec<ReplayPlayerMetadata>,
    pub goals: Vec<ReplayGoalMetadata>,
    pub map_name: String,
    pub date: String,
    pub match_type: String,
    pub replay_id: String,
    pub team0_score: Option<i32>,
    pub team1_score: Option<i32>,
    pub duration_seconds: Option<u32>,
    pub frame_count: Option<i32>,
    pub file_size: u64,
    pub modified_unix_secs: Option<u64>,
    pub error: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayPlayerMetadata {
    pub name: String,
    pub team: Option<i32>,
    pub score: Option<i32>,
    pub goals: Option<i32>,
    pub assists: Option<i32>,
    pub saves: Option<i32>,
    pub shots: Option<i32>,
    pub is_bot: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayGoalMetadata {
    pub player_name: String,
    pub team: Option<i32>,
    pub frame: Option<i32>,
    pub elapsed_seconds: Option<u32>,
}

impl ReplayMetadataEntry {
    pub fn has_metadata(&self) -> bool {
        self.error.is_empty()
    }
}

#[derive(Debug, Default)]
pub struct MetadataScanControl {
    running: bool,
    pending_folder: Option<String>,
}

impl MetadataScanControl {
    fn request(&mut self, folder: String) -> Option<String> {
        if self.running {
            self.pending_folder = Some(folder);
            None
        } else {
            self.running = true;
            Some(folder)
        }
    }

    fn finish_scan(&mut self) -> Option<String> {
        if let Some(folder) = self.pending_folder.take() {
            Some(folder)
        } else {
            self.running = false;
            None
        }
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
    let first_folder = {
        let mut control = state
            .replays
            .metadata_scan_control
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let first_folder = control.request(folder);
        if first_folder.is_some() {
            state
                .replays
                .metadata_scan_running
                .store(true, Ordering::SeqCst);
        }
        first_folder
    };
    let Some(mut folder) = first_folder else {
        return;
    };
    if let Ok(mut status) = state.replays.metadata_status.lock() {
        *status = "Scanning replay metadata...".to_string();
    }

    tokio::spawn(async move {
        loop {
            let state_for_scan = state.clone();
            let scan_folder_path = folder.clone();
            let result = tokio::task::spawn_blocking(move || {
                scan_folder(&state_for_scan, &scan_folder_path)
            })
            .await;

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

            let next_folder = {
                let mut control = state
                    .replays
                    .metadata_scan_control
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                let next_folder = control.finish_scan();
                if next_folder.is_none() {
                    state
                        .replays
                        .metadata_scan_running
                        .store(false, Ordering::SeqCst);
                }
                next_folder
            };
            let Some(next_folder) = next_folder else {
                break;
            };
            folder = next_folder;
            if let Ok(mut status) = state.replays.metadata_status.lock() {
                *status = "Refreshing replay metadata...".to_string();
            }
        }
    });
}

pub fn merged_metadata_snapshot(state: &AppState) -> ReplayMetadataSnapshot {
    let local = state.replays.metadata_cache.load();
    let cloud = state.replays.cloud_metadata_cache.load();
    let mut merged = (**local).clone();

    for (filename, cloud_entry) in cloud.iter() {
        let mut entry = cloud_entry.clone();
        if let Some(local_entry) = local.entries.get(filename) {
            entry.file_size = local_entry.file_size;
            entry.modified_unix_secs = local_entry.modified_unix_secs;
            entry.players = local_entry.players.clone();
            entry.goals = local_entry.goals.clone();
            entry.duration_seconds = local_entry.duration_seconds;
            entry.frame_count = local_entry.frame_count;
        }
        merged.entries.insert(filename.clone(), entry);
    }
    merged
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

    let bytes = match read_replay_header_prefix(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            entry.error = format!("Could not read replay: {error}");
            return entry;
        }
    };

    let properties = match parse_header_properties(&bytes) {
        Ok(properties) => properties,
        Err(error) => {
            entry.error = format!("Could not parse replay header: {error}");
            return entry;
        }
    };

    apply_properties(&mut entry, &properties);
    if entry.display_name.trim().is_empty() {
        entry.display_name = filename.trim_end_matches(".replay").to_string();
    }
    entry
}

fn read_replay_header_prefix(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut file = fs::File::open(path)?;
    let mut prefix = [0_u8; 8];
    file.read_exact(&mut prefix)?;
    let header_len = i32::from_le_bytes(prefix[0..4].try_into().unwrap());
    if header_len < 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "negative replay header length",
        ));
    }
    let header_len = header_len as usize;
    if header_len > MAX_REPLAY_HEADER_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "replay header length exceeds parser limit",
        ));
    }
    let total_len = 8_usize
        .checked_add(header_len)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "header too large"))?;
    let file_len = file.metadata()?.len();
    if total_len as u64 > file_len {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "replay header length exceeds file size",
        ));
    }
    let mut bytes = Vec::with_capacity(total_len);
    bytes.extend_from_slice(&prefix);
    bytes.resize(total_len, 0);
    file.read_exact(&mut bytes[8..])?;
    Ok(bytes)
}

pub(crate) fn validate_replay_file(path: &Path) -> Result<(), String> {
    let bytes = read_replay_header_prefix(path)
        .map_err(|error| format!("Could not read replay header: {error}"))?;
    parse_header_properties(&bytes).map(|_| ())
}

pub(crate) fn validate_replay_bytes(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < 8 {
        return Err("Replay is shorter than its header prefix.".to_string());
    }
    let header_len = i32::from_le_bytes(
        bytes[0..4]
            .try_into()
            .map_err(|_| "Replay header prefix is invalid.".to_string())?,
    );
    if header_len < 0 {
        return Err("Replay header length is negative.".to_string());
    }
    let header_len = header_len as usize;
    if header_len > MAX_REPLAY_HEADER_BYTES {
        return Err("Replay header length exceeds parser limit.".to_string());
    }
    let header_end = 8_usize
        .checked_add(header_len)
        .ok_or_else(|| "Replay header length overflowed.".to_string())?;
    let header = bytes
        .get(..header_end)
        .ok_or_else(|| "Replay header length exceeds downloaded file size.".to_string())?;
    parse_header_properties(header).map(|_| ())
}

fn parse_header_properties(bytes: &[u8]) -> Result<Vec<(String, HeaderProp)>, String> {
    if bytes.len() < 8 {
        return Err("header prefix is too short".to_string());
    }
    let header_len = i32::from_le_bytes(bytes[0..4].try_into().unwrap());
    if header_len < 0 {
        return Err("negative header length".to_string());
    }
    let header_len = header_len as usize;
    if header_len > MAX_REPLAY_HEADER_BYTES {
        return Err("header length exceeds parser limit".to_string());
    }
    let header_end = 8_usize
        .checked_add(header_len)
        .ok_or_else(|| "header length overflowed".to_string())?;
    let header = bytes
        .get(8..header_end)
        .ok_or_else(|| "header bytes are incomplete".to_string())?;
    let mut parser = HeaderParser::new(header);
    let major_version = parser.take_i32("major version")?;
    let minor_version = parser.take_i32("minor version")?;
    if major_version > 865 && minor_version > 17 {
        parser.take_i32("net version")?;
    }
    parser.parse_text("game type")?;
    let mut property_count = 0;
    parser.parse_properties(0, &mut property_count)
}

struct HeaderParser<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> HeaderParser<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, count: usize, section: &str) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| format!("{section} offset overflowed"))?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| format!("{section} is truncated"))?;
        self.offset = end;
        Ok(slice)
    }

    fn take_i32(&mut self, section: &str) -> Result<i32, String> {
        let bytes: [u8; 4] = self.take(4, section)?.try_into().unwrap();
        Ok(i32::from_le_bytes(bytes))
    }

    fn take_u32(&mut self, section: &str) -> Result<u32, String> {
        let bytes: [u8; 4] = self.take(4, section)?.try_into().unwrap();
        Ok(u32::from_le_bytes(bytes))
    }

    fn parse_str(&mut self, section: &str) -> Result<String, String> {
        let len = self.take_i32(section)?;
        if !(0..=10_000).contains(&len) {
            return Err(format!("{section} length is invalid"));
        }
        let bytes = self.take(len as usize, section)?;
        Ok(decode_windows_1252(bytes)
            .trim_end_matches('\0')
            .to_string())
    }

    fn parse_text(&mut self, section: &str) -> Result<String, String> {
        let characters = self.take_i32(section)?;
        if !(-10_000..=10_000).contains(&characters) {
            return Err(format!("{section} length is too large"));
        }
        if characters < 0 {
            let byte_len = characters
                .checked_mul(-2)
                .ok_or_else(|| format!("{section} length overflowed"))?;
            let bytes = self.take(byte_len as usize, section)?;
            #[allow(unknown_lints)]
            #[allow(clippy::chunks_exact_to_as_chunks)]
            let code_units: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .take_while(|unit| *unit != 0)
                .collect();
            String::from_utf16(&code_units).map_err(|_| format!("{section} is invalid UTF-16"))
        } else {
            let bytes = self.take(characters as usize, section)?;
            Ok(decode_windows_1252(bytes)
                .trim_end_matches('\0')
                .to_string())
        }
    }

    fn parse_properties(
        &mut self,
        depth: usize,
        property_count: &mut usize,
    ) -> Result<Vec<(String, HeaderProp)>, String> {
        if depth > MAX_HEADER_PROPERTY_DEPTH {
            return Err("property nesting is too deep".to_string());
        }
        let mut properties = Vec::new();
        loop {
            let key = self.parse_str("property key")?;
            if key == "None" {
                break;
            }
            *property_count = (*property_count)
                .checked_add(1)
                .ok_or_else(|| "property count overflowed".to_string())?;
            if *property_count > MAX_HEADER_PROPERTIES {
                return Err("header has too many properties".to_string());
            }
            let kind = self.parse_str("property kind")?;
            let _size = self.take_u32("property size")?;
            self.take(4, "property index")?;
            let value = match kind.as_str() {
                "BoolProperty" => HeaderProp::Bool(self.take(1, "bool property")?[0] == 1),
                "ByteProperty" => {
                    let kind = self.parse_str("byte property kind")?;
                    if kind == "None" {
                        self.take(1, "byte property terminator")?;
                        continue;
                    }
                    HeaderProp::Byte {
                        kind,
                        value: Some(self.parse_str("byte property value")?),
                    }
                }
                "ArrayProperty" => {
                    let count = self.take_i32("array property count")?;
                    if !(0..=25_000).contains(&count) {
                        return Err("array property count is invalid".to_string());
                    }
                    let mut items = Vec::with_capacity(count as usize);
                    for _ in 0..count {
                        items.push(self.parse_properties(depth + 1, property_count)?);
                    }
                    HeaderProp::Array(items)
                }
                "FloatProperty" => {
                    let bytes: [u8; 4] = self.take(4, "float property")?.try_into().unwrap();
                    HeaderProp::Float(f32::from_le_bytes(bytes))
                }
                "IntProperty" => HeaderProp::Int(self.take_i32("int property")?),
                "QWordProperty" => {
                    let bytes: [u8; 8] = self.take(8, "qword property")?.try_into().unwrap();
                    HeaderProp::QWord(u64::from_le_bytes(bytes))
                }
                "NameProperty" => HeaderProp::Name(self.parse_text("name property")?),
                "StrProperty" => HeaderProp::Str(self.parse_text("str property")?),
                "StructProperty" => {
                    let name = self.parse_str("struct property name")?;
                    let fields = self.parse_properties(depth + 1, property_count)?;
                    HeaderProp::Struct { name, fields }
                }
                _ => return Err(format!("unexpected header property kind {kind}")),
            };
            properties.push((key, value));
        }
        Ok(properties)
    }
}

fn decode_windows_1252(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| match *byte {
            0x80 => '\u{20ac}',
            0x82 => '\u{201a}',
            0x83 => '\u{0192}',
            0x84 => '\u{201e}',
            0x85 => '\u{2026}',
            0x86 => '\u{2020}',
            0x87 => '\u{2021}',
            0x88 => '\u{02c6}',
            0x89 => '\u{2030}',
            0x8a => '\u{0160}',
            0x8b => '\u{2039}',
            0x8c => '\u{0152}',
            0x8e => '\u{017d}',
            0x91 => '\u{2018}',
            0x92 => '\u{2019}',
            0x93 => '\u{201c}',
            0x94 => '\u{201d}',
            0x95 => '\u{2022}',
            0x96 => '\u{2013}',
            0x97 => '\u{2014}',
            0x98 => '\u{02dc}',
            0x99 => '\u{2122}',
            0x9a => '\u{0161}',
            0x9b => '\u{203a}',
            0x9c => '\u{0153}',
            0x9e => '\u{017e}',
            0x9f => '\u{0178}',
            other => char::from(other),
        })
        .collect()
}

pub fn apply_properties(entry: &mut ReplayMetadataEntry, properties: &[(String, HeaderProp)]) {
    entry.player_name = string_property(properties, "PlayerName").unwrap_or_default();
    entry.players = player_stats(properties);
    entry.player_names = entry.players.iter().fold(Vec::new(), |mut names, player| {
        if !player.name.trim().is_empty() && !names.contains(&player.name) {
            names.push(player.name.clone());
        }
        names
    });
    let record_fps = replay_record_fps(properties);
    entry.goals = replay_goals(properties, record_fps);
    entry.map_name = string_property(properties, "MapName").unwrap_or_default();
    entry.date = string_property(properties, "Date").unwrap_or_default();
    entry.match_type = string_property(properties, "MatchType").unwrap_or_default();
    entry.replay_id = string_property(properties, "Id").unwrap_or_default();
    entry.team0_score = int_property(properties, "Team0Score");
    entry.team1_score = int_property(properties, "Team1Score");
    entry.frame_count = int_property(properties, "ReplayLastFrame")
        .or_else(|| int_property(properties, "NumFrames"));
    entry.duration_seconds = replay_duration_seconds(entry.frame_count, record_fps);

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

fn bool_property(properties: &[(String, HeaderProp)], key: &str) -> Option<bool> {
    properties
        .iter()
        .find(|(name, _)| name == key)
        .and_then(|(_, prop)| prop.as_bool())
}

fn float_property(properties: &[(String, HeaderProp)], key: &str) -> Option<f32> {
    properties
        .iter()
        .find(|(name, _)| name == key)
        .and_then(|(_, prop)| prop.as_float())
}

fn player_stats(properties: &[(String, HeaderProp)]) -> Vec<ReplayPlayerMetadata> {
    properties
        .iter()
        .find(|(name, _)| name == "PlayerStats")
        .and_then(|(_, prop)| prop.as_array())
        .map(|players| {
            let mut stats = Vec::new();
            for player in players {
                let name = string_property(player, "Name").unwrap_or_default();
                if !name.trim().is_empty() {
                    stats.push(ReplayPlayerMetadata {
                        name,
                        team: int_property(player, "Team"),
                        score: int_property(player, "Score"),
                        goals: int_property(player, "Goals"),
                        assists: int_property(player, "Assists"),
                        saves: int_property(player, "Saves"),
                        shots: int_property(player, "Shots"),
                        is_bot: bool_property(player, "bBot"),
                    });
                }
            }
            stats
        })
        .unwrap_or_default()
}

fn replay_goals(
    properties: &[(String, HeaderProp)],
    record_fps: Option<f32>,
) -> Vec<ReplayGoalMetadata> {
    properties
        .iter()
        .find(|(name, _)| name == "Goals")
        .and_then(|(_, prop)| prop.as_array())
        .map(|goals| {
            goals
                .iter()
                .map(|goal| {
                    let frame = int_property(goal, "frame");
                    ReplayGoalMetadata {
                        player_name: string_property(goal, "PlayerName").unwrap_or_default(),
                        team: int_property(goal, "PlayerTeam"),
                        frame,
                        elapsed_seconds: frame.and_then(|frame| {
                            seconds_from_frames(frame, record_fps?).filter(|_| frame >= 0)
                        }),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn replay_record_fps(properties: &[(String, HeaderProp)]) -> Option<f32> {
    float_property(properties, "RecordFPS")
        .or_else(|| int_property(properties, "RecordFPS").map(|fps| fps as f32))
        .filter(|fps| fps.is_finite() && *fps > 0.0)
}

fn replay_duration_seconds(frame_count: Option<i32>, record_fps: Option<f32>) -> Option<u32> {
    seconds_from_frames(frame_count.filter(|frames| *frames >= 0)?, record_fps?)
}

fn seconds_from_frames(frames: i32, fps: f32) -> Option<u32> {
    let seconds = (frames as f32 / fps).round();
    (seconds.is_finite() && seconds >= 0.0 && seconds <= u32::MAX as f32).then_some(seconds as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn str_prop(key: &str, value: &str) -> (String, HeaderProp) {
        (key.to_string(), HeaderProp::Str(value.to_string()))
    }

    fn int_prop(key: &str, value: i32) -> (String, HeaderProp) {
        (key.to_string(), HeaderProp::Int(value))
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

    #[test]
    fn extracts_player_box_scores_goal_timeline_and_duration() {
        let mut entry = ReplayMetadataEntry::default();
        let player_stats = HeaderProp::Array(vec![vec![
            str_prop("Name", "Blue One"),
            int_prop("Team", 0),
            int_prop("Score", 515),
            int_prop("Goals", 2),
            int_prop("Assists", 1),
            int_prop("Saves", 3),
            int_prop("Shots", 4),
            ("bBot".to_string(), HeaderProp::Bool(false)),
        ]]);
        let goals = HeaderProp::Array(vec![vec![
            str_prop("PlayerName", "Blue One"),
            int_prop("PlayerTeam", 0),
            int_prop("frame", 900),
        ]]);

        apply_properties(
            &mut entry,
            &[
                ("PlayerStats".to_string(), player_stats),
                ("Goals".to_string(), goals),
                int_prop("ReplayLastFrame", 1_800),
                ("RecordFPS".to_string(), HeaderProp::Float(30.0)),
            ],
        );

        assert_eq!(entry.duration_seconds, Some(60));
        assert_eq!(entry.players[0].score, Some(515));
        assert_eq!(entry.players[0].saves, Some(3));
        assert_eq!(entry.goals[0].player_name, "Blue One");
        assert_eq!(entry.goals[0].elapsed_seconds, Some(30));
    }

    fn push_i32(bytes: &mut Vec<u8>, value: i32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn push_str(bytes: &mut Vec<u8>, value: &str) {
        push_i32(bytes, value.len() as i32 + 1);
        bytes.extend_from_slice(value.as_bytes());
        bytes.push(0);
    }

    fn push_text(bytes: &mut Vec<u8>, value: &str) {
        push_str(bytes, value);
    }

    fn push_prop_header(bytes: &mut Vec<u8>, key: &str, kind: &str, size: u32) {
        push_str(bytes, key);
        push_str(bytes, kind);
        push_u32(bytes, size);
        push_u32(bytes, 0);
    }

    fn synthetic_header() -> Vec<u8> {
        let mut header = Vec::new();
        push_i32(&mut header, 868);
        push_i32(&mut header, 22);
        push_i32(&mut header, 10);
        push_text(&mut header, "TAGame.Replay");
        push_prop_header(&mut header, "ReplayName", "StrProperty", 12);
        push_text(&mut header, "Ranked Doubles");
        push_prop_header(&mut header, "MapName", "StrProperty", 9);
        push_text(&mut header, "Stadium_P");
        push_prop_header(&mut header, "Team0Score", "IntProperty", 4);
        push_i32(&mut header, 3);
        push_prop_header(&mut header, "Team1Score", "IntProperty", 4);
        push_i32(&mut header, 2);
        push_str(&mut header, "None");

        let mut replay = Vec::new();
        push_i32(&mut replay, header.len() as i32);
        push_u32(&mut replay, 0);
        replay.extend_from_slice(&header);
        replay
    }

    fn replay_from_header_payload(header: Vec<u8>) -> Vec<u8> {
        let mut replay = Vec::new();
        push_i32(&mut replay, header.len() as i32);
        push_u32(&mut replay, 0);
        replay.extend_from_slice(&header);
        replay
    }

    #[test]
    fn header_prefix_parser_preserves_display_metadata() {
        let properties = parse_header_properties(&synthetic_header()).unwrap();
        let mut entry = ReplayMetadataEntry::default();

        apply_properties(&mut entry, &properties);

        assert_eq!(entry.display_name, "Ranked Doubles");
        assert_eq!(entry.map_name, "Stadium_P");
        assert_eq!(entry.team0_score, Some(3));
        assert_eq!(entry.team1_score, Some(2));
    }

    #[test]
    fn replay_header_read_is_bounded_to_header_prefix() {
        let temp = tempfile::tempdir().unwrap();
        let replay_path = temp.path().join("match.replay");
        let mut bytes = synthetic_header();
        let expected_len = bytes.len();
        bytes.extend(std::iter::repeat_n(7_u8, 4096));
        fs::write(&replay_path, bytes).unwrap();

        let read = read_replay_header_prefix(&replay_path).unwrap();

        assert_eq!(read.len(), expected_len);
    }

    #[test]
    fn replay_header_reader_rejects_oversized_advertised_header() {
        let temp = tempfile::tempdir().unwrap();
        let replay_path = temp.path().join("oversized.replay");
        let mut bytes = Vec::new();
        push_i32(&mut bytes, i32::MAX);
        push_u32(&mut bytes, 0);
        fs::write(&replay_path, bytes).unwrap();

        let error = read_replay_header_prefix(&replay_path).unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn replay_header_reader_rejects_header_longer_than_file() {
        let temp = tempfile::tempdir().unwrap();
        let replay_path = temp.path().join("truncated.replay");
        let mut bytes = Vec::new();
        push_i32(&mut bytes, 4096);
        push_u32(&mut bytes, 0);
        fs::write(&replay_path, bytes).unwrap();

        let error = read_replay_header_prefix(&replay_path).unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn replay_header_parser_rejects_excessive_property_nesting() {
        let mut header = Vec::new();
        push_i32(&mut header, 868);
        push_i32(&mut header, 22);
        push_i32(&mut header, 10);
        push_text(&mut header, "TAGame.Replay");

        for depth in 0..=MAX_HEADER_PROPERTY_DEPTH {
            push_prop_header(&mut header, &format!("Nested{depth}"), "StructProperty", 0);
            push_str(&mut header, "Replay.Struct");
        }
        for _ in 0..=MAX_HEADER_PROPERTY_DEPTH {
            push_str(&mut header, "None");
        }

        let error = parse_header_properties(&replay_from_header_payload(header)).unwrap_err();

        assert!(error.contains("property nesting is too deep"));
    }

    #[test]
    fn scan_control_coalesces_to_latest_pending_folder() {
        let mut control = MetadataScanControl::default();

        assert_eq!(
            control.request("first".to_string()),
            Some("first".to_string())
        );
        assert_eq!(control.request("second".to_string()), None);
        assert_eq!(control.request("latest".to_string()), None);
        assert_eq!(control.finish_scan(), Some("latest".to_string()));
        assert_eq!(control.finish_scan(), None);
        assert!(!control.running);
    }

    #[test]
    fn merged_snapshot_preserves_local_identity_and_cloud_details() {
        let state = AppState::new();
        state
            .replays
            .metadata_cache
            .store(Arc::new(ReplayMetadataSnapshot {
                folder: "replays".to_string(),
                entries: HashMap::from([(
                    "match.replay".to_string(),
                    ReplayMetadataEntry {
                        filename: "match.replay".to_string(),
                        display_name: "Local".to_string(),
                        file_size: 42,
                        modified_unix_secs: Some(7),
                        ..Default::default()
                    },
                )]),
                total_files: 1,
                parsed: 1,
                ..Default::default()
            }));
        state
            .replays
            .cloud_metadata_cache
            .store(Arc::new(HashMap::from([(
                "match.replay".to_string(),
                ReplayMetadataEntry {
                    filename: "match.replay".to_string(),
                    display_name: "Cloud title".to_string(),
                    replay_id: "cloud-id".to_string(),
                    ..Default::default()
                },
            )])));

        let merged = merged_metadata_snapshot(&state);
        let entry = &merged.entries["match.replay"];
        assert_eq!(entry.display_name, "Cloud title");
        assert_eq!(entry.replay_id, "cloud-id");
        assert_eq!(entry.file_size, 42);
        assert_eq!(entry.modified_unix_secs, Some(7));
    }
}
