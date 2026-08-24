use crate::session::{SessionOverlayDisplay, SessionState};
use crate::setup::StatsApiSetupResult;
use crate::stats_api::StatsApiTransport;
use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum TeammateBoostDisplay {
    #[default]
    Bars,
    Circles,
    Compact,
    Numbers,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum LobbyTheme {
    #[default]
    Glass,
    Solid,
    Modern,
    Minimalist,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum LobbyDisplayMode {
    Compact,
    #[default]
    Expanded,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum DashboardPlayerLayout {
    #[default]
    Table,
    Cards,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub transparency: u8,
    pub ui_scale: f32,
    pub show_bots: bool,
    pub window_size: [f32; 2],
    pub lobby_theme: LobbyTheme,
    pub lobby_display_mode: LobbyDisplayMode,
    pub monitor_index: usize,
    pub dashboard_enabled: bool,
    pub dashboard_monitor_index: usize,
    pub dashboard_fullscreen: bool,
    pub dashboard_open_with_overlay: bool,
    pub dashboard_keep_overlay_enabled: bool,
    pub dashboard_show_boost: bool,
    pub dashboard_show_ranks: bool,
    pub dashboard_show_team_comparison: bool,
    pub dashboard_show_event_feed: bool,
    pub dashboard_show_replay_upload: bool,
    pub dashboard_player_layout: DashboardPlayerLayout,
    pub dashboard_scale: f32,
    pub debounce_touch_counters: bool,
    pub estimate_teammate_bumps: bool,
    pub hotkey_kb: String,
    pub hotkey_ctrl: String,
    pub hotkey_settings: String,
    pub hotkey_launch: String,
    pub hotkey_toggle: bool,
    pub show_stats: bool,
    pub show_teammate_boost: bool,
    pub show_lobby_matches: bool,
    pub show_lobby_ranks: bool,
    pub history_enabled: bool,
    pub lobby_history_indicators_enabled: bool,
    pub debug_logging_enabled: bool,
    pub teammate_hud_scale: f32,
    pub teammate_boost_display: TeammateBoostDisplay,
    pub rocket_league_path: String,
    pub stats_api_packet_send_rate: u16,
    pub alpha_boost_enabled: bool,
    pub session_overlay_enabled: bool,
    pub session_overlay_scale: f32,
    pub session_overlay_opacity: u8,
    pub session_overlay_display: SessionOverlayDisplay,
    pub session_overlay_follow_lobby_hotkey: bool,
    pub session_expanded_show_streaks: bool,
    pub session_expanded_show_breakdown: bool,
    pub session_expanded_show_mmr_delta: bool,
    pub lobby_manual_position: Option<[f32; 2]>,
    pub teammate_boost_manual_position: Option<[f32; 2]>,
    pub session_manual_position: Option<[f32; 2]>,
    pub layout_mode: bool,
    pub cached_local_player_identity: LocalPlayerIdentity,
    pub lock_local_player: bool,
    pub ballchasing_enabled: bool,
    pub ballchasing_api_key: String,
    pub ballchasing_visibility: String,
    pub replays_folder: String,
    pub uploaded_replays: Vec<String>,
    pub auto_gg: bool,
    pub auto_gg_sequence: String,
    pub auto_freeplay: bool,
    pub auto_freeplay_sequence: String,
}

pub fn detect_rocket_league_path() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        let candidates = [
            "C:\\Program Files\\Epic Games\\rocketleague",
            "C:\\Program Files (x86)\\Steam\\steamapps\\common\\rocketleague",
        ];
        for candidate in candidates {
            let path = std::path::Path::new(candidate);
            if path.join("TAGame").join("CookedPCConsole").exists() {
                return Some(candidate.to_string());
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
            let candidates = [
                home.join(".local/share/Steam/steamapps/common/rocketleague"),
                home.join(
                    ".var/app/com.valvesoftware.Steam/data/Steam/steamapps/common/rocketleague",
                ),
                home.join(".steam/steam/steamapps/common/rocketleague"),
                home.join(".steam/root/steamapps/common/rocketleague"),
            ];
            for candidate in candidates {
                if candidate.join("TAGame").join("CookedPCConsole").exists() {
                    return Some(candidate.to_string_lossy().into_owned());
                }
            }
        }
    }

    None
}

pub fn detect_replays_path() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        if let Some(user_profile) = env::var_os("USERPROFILE").map(PathBuf::from) {
            let candidates = [
                user_profile
                    .join("Documents")
                    .join("My Games")
                    .join("Rocket League")
                    .join("TAGame")
                    .join("Demos"),
                user_profile
                    .join("Documents")
                    .join("My Games")
                    .join("Rocket League")
                    .join("TAGame")
                    .join("DemosEpic"),
                user_profile
                    .join("OneDrive")
                    .join("Documents")
                    .join("My Games")
                    .join("Rocket League")
                    .join("TAGame")
                    .join("Demos"),
                user_profile
                    .join("OneDrive")
                    .join("Documents")
                    .join("My Games")
                    .join("Rocket League")
                    .join("TAGame")
                    .join("DemosEpic"),
            ];
            for candidate in candidates {
                if candidate.exists() {
                    return Some(candidate.to_string_lossy().into_owned());
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
            let candidates = [
                home.join(".local/share/Steam/steamapps/compatdata/252950/pfx/drive_c/users/steamuser/Documents/My Games/Rocket League/TAGame/Demos"),
                home.join(".local/share/Steam/steamapps/compatdata/252950/pfx/drive_c/users/steamuser/Documents/My Games/Rocket League/TAGame/DemosEpic"),
                home.join(".steam/steam/steamapps/compatdata/252950/pfx/drive_c/users/steamuser/Documents/My Games/Rocket League/TAGame/Demos"),
                home.join(".steam/steam/steamapps/compatdata/252950/pfx/drive_c/users/steamuser/Documents/My Games/Rocket League/TAGame/DemosEpic"),
                home.join(".steam/root/steamapps/compatdata/252950/pfx/drive_c/users/steamuser/Documents/My Games/Rocket League/TAGame/Demos"),
                home.join(".steam/root/steamapps/compatdata/252950/pfx/drive_c/users/steamuser/Documents/My Games/Rocket League/TAGame/DemosEpic"),
                home.join(".var/app/com.valvesoftware.Steam/data/Steam/steamapps/compatdata/252950/pfx/drive_c/users/steamuser/Documents/My Games/Rocket League/TAGame/Demos"),
                home.join(".var/app/com.valvesoftware.Steam/data/Steam/steamapps/compatdata/252950/pfx/drive_c/users/steamuser/Documents/My Games/Rocket League/TAGame/DemosEpic"),
            ];
            for candidate in candidates {
                if candidate.exists() {
                    return Some(candidate.to_string_lossy().into_owned());
                }
            }
        }
    }

    None
}

impl Default for Config {
    fn default() -> Self {
        let rocket_league_path = detect_rocket_league_path().unwrap_or_default();
        Self {
            transparency: 150,
            ui_scale: 2.2,
            show_bots: true,
            window_size: [1920.0, 1080.0],
            lobby_theme: LobbyTheme::default(),
            lobby_display_mode: LobbyDisplayMode::default(),
            monitor_index: 0,
            dashboard_enabled: false,
            dashboard_monitor_index: 0,
            dashboard_fullscreen: true,
            dashboard_open_with_overlay: false,
            dashboard_keep_overlay_enabled: true,
            dashboard_show_boost: true,
            dashboard_show_ranks: true,
            dashboard_show_team_comparison: true,
            dashboard_show_event_feed: true,
            dashboard_show_replay_upload: true,
            dashboard_player_layout: DashboardPlayerLayout::Table,
            dashboard_scale: 1.0,
            debounce_touch_counters: false,
            estimate_teammate_bumps: false,
            hotkey_kb: "Backspace".to_string(),
            hotkey_ctrl: "Select".to_string(),
            hotkey_settings: "F1".to_string(),
            hotkey_launch: "F4".to_string(),
            hotkey_toggle: false,
            show_stats: true,
            show_teammate_boost: false,
            show_lobby_matches: false,
            show_lobby_ranks: true,
            history_enabled: false,
            lobby_history_indicators_enabled: true,
            debug_logging_enabled: false,
            teammate_hud_scale: 2.2,
            teammate_boost_display: TeammateBoostDisplay::Bars,
            rocket_league_path,
            stats_api_packet_send_rate: 30,
            alpha_boost_enabled: false,
            session_overlay_enabled: false,
            session_overlay_scale: 1.4,
            session_overlay_opacity: 170,
            session_overlay_display: SessionOverlayDisplay::Compact,
            session_overlay_follow_lobby_hotkey: false,
            session_expanded_show_streaks: true,
            session_expanded_show_breakdown: true,
            session_expanded_show_mmr_delta: false,
            lobby_manual_position: None,
            teammate_boost_manual_position: None,
            session_manual_position: None,
            layout_mode: false,
            cached_local_player_identity: LocalPlayerIdentity::default(),
            lock_local_player: false,
            ballchasing_enabled: false,
            ballchasing_api_key: "".to_string(),
            ballchasing_visibility: "public".to_string(),
            replays_folder: detect_replays_path().unwrap_or_default(),
            uploaded_replays: Vec::new(),
            auto_gg: false,
            auto_gg_sequence: "T,G,G,Enter".to_string(),
            auto_freeplay: false,
            auto_freeplay_sequence: "Escape,Delay400,Down,Delay200,Return,Delay200,Down,Delay200,Return,Delay600,Return,Delay200,Return,Delay200,Return,Delay200,Return".to_string(),
        }
    }
}

impl Config {
    pub fn load() -> (Self, ConfigStatus) {
        let path = config_path();
        Self::load_from_path(path)
    }

    pub fn load_from_path(path: PathBuf) -> (Self, ConfigStatus) {
        let mut status = ConfigStatus::new(path.clone());

        if path.exists() {
            match load_config_file(&path) {
                Ok(config) => return (config, status),
                Err(error) => status.last_error = error,
            }
        } else if let Ok(config) = load_config_file(&PathBuf::from("config.toml")) {
            return (config, status);
        }

        let config = Config::default();
        (config, status)
    }

    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        self.save_to_path(&path)
    }

    pub fn save_to_path(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create config directory: {error}"))?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|error| format!("Could not serialize config: {error}"))?;
        atomic_write_config(path, content.as_bytes())
    }
}

fn atomic_write_config(path: &Path, content: &[u8]) -> Result<(), String> {
    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.toml");
    let temp_path = parent.join(format!(
        ".{file_name}.tmp-{}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));

    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .map_err(|error| format!("Could not create temporary config: {error}"))?;
        file.write_all(content)
            .map_err(|error| format!("Could not write temporary config: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Could not flush temporary config: {error}"))?;
        fs::rename(&temp_path, path).map_err(|error| format!("Could not replace config: {error}"))
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn load_config_file(path: &PathBuf) -> Result<Config, String> {
    let content =
        fs::read_to_string(path).map_err(|error| format!("Could not read config: {error}"))?;
    toml::from_str(&content).map_err(|error| format!("Could not parse config: {error}"))
}

fn config_path() -> PathBuf {
    if cfg!(test) || std::env::var("RL_OVERLAY_TEST").is_ok() {
        std::env::temp_dir().join(format!(
            "rl_platform_overlay_config_test_{}.toml",
            std::process::id()
        ))
    } else {
        config_dir().map_or_else(
            || PathBuf::from("config.toml"),
            |dir| dir.join("config.toml"),
        )
    }
}

pub fn config_dir() -> Option<PathBuf> {
    #[cfg(test)]
    {
        Some(std::env::temp_dir().join(format!("rl_platform_overlay_test_{}", std::process::id())))
    }

    #[cfg(all(not(test), target_os = "windows"))]
    {
        env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("RL-Platform-Overlay"))
    }

    #[cfg(all(not(test), not(target_os = "windows")))]
    {
        if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
            return Some(PathBuf::from(path).join("rl-platform-overlay"));
        }
        env::var_os("HOME")
            .map(PathBuf::from)
            .map(|path| path.join(".config").join("rl-platform-overlay"))
    }
}

fn release_url() -> String {
    #[cfg(not(feature = "microsoft-store"))]
    {
        crate::update::LATEST_RELEASE_URL.to_string()
    }
    #[cfg(feature = "microsoft-store")]
    {
        String::new()
    }
}

fn release_signing_public_key() -> String {
    #[cfg(not(feature = "microsoft-store"))]
    {
        crate::update::RELEASE_SIGNING_PUBLIC_KEY_B64.to_string()
    }
    #[cfg(feature = "microsoft-store")]
    {
        String::new()
    }
}

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub config_path: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> Self {
        let config_dir = app_config_dir().unwrap_or_else(|| PathBuf::from("."));
        let config_path = config_dir.join("config.toml");
        Self {
            config_dir,
            config_path,
        }
    }
}

fn app_config_dir() -> Option<PathBuf> {
    #[cfg(test)]
    {
        let temp_dir = tempfile::Builder::new()
            .prefix("rl_platform_overlay_state_")
            .tempdir()
            .ok()?;
        Some(temp_dir.keep())
    }

    #[cfg(not(test))]
    {
        config_dir()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.transparency, 150);
        assert_eq!(config.ui_scale, 2.2);
        assert!(config.show_bots);
        assert_eq!(config.lobby_display_mode, LobbyDisplayMode::Expanded);
        assert!(!config.dashboard_enabled);
        assert_eq!(config.dashboard_monitor_index, 0);
        assert!(config.dashboard_fullscreen);
        assert!(!config.dashboard_open_with_overlay);
        assert!(config.dashboard_keep_overlay_enabled);
        assert!(config.dashboard_show_boost);
        assert!(config.dashboard_show_ranks);
        assert!(config.dashboard_show_team_comparison);
        assert!(config.dashboard_show_event_feed);
        assert!(config.dashboard_show_replay_upload);
        assert_eq!(config.dashboard_player_layout, DashboardPlayerLayout::Table);
        assert_eq!(config.dashboard_scale, 1.0);
        assert!(!config.cached_local_player_identity.is_known());
    }

    #[test]
    fn local_player_identity_compares_accounts_case_insensitively() {
        let a = LocalPlayerIdentity {
            name: "Me".to_string(),
            primary_id: "Steam|ABC|0".to_string(),
            platform: "Steam".to_string(),
        };
        let b = LocalPlayerIdentity {
            name: "DifferentName".to_string(),
            primary_id: "steam|abc|0".to_string(),
            platform: "steam".to_string(),
        };

        assert!(a.same_account(&b));
    }

    #[test]
    fn update_local_player_identity_reports_first_known_identity() {
        let state = AppState::new();
        state
            .game
            .local_player_identity
            .store(Arc::new(LocalPlayerIdentity::default()));
        let first = LocalPlayerIdentity {
            name: "Me".to_string(),
            primary_id: "Steam|1|0".to_string(),
            platform: "Steam".to_string(),
        };
        let renamed = LocalPlayerIdentity {
            name: "MeAgain".to_string(),
            primary_id: "Steam|1|0".to_string(),
            platform: "Steam".to_string(),
        };

        assert!(state.update_local_player_identity(first.clone()));
        assert!(!state.update_local_player_identity(first));
        assert!(!state.update_local_player_identity(renamed));
    }

    #[test]
    fn update_local_player_identity_ignores_updates_when_locked() {
        let state = AppState::new();
        let mut config = state.system.config.load().as_ref().clone();
        config.lock_local_player = true;
        state.replace_config(config);

        state
            .game
            .local_player_identity
            .store(Arc::new(LocalPlayerIdentity {
                name: "LockedPlayer".to_string(),
                primary_id: "Steam|1|0".to_string(),
                platform: "Steam".to_string(),
            }));

        let new_identity = LocalPlayerIdentity {
            name: "NewPlayer".to_string(),
            primary_id: "Epic|2|0".to_string(),
            platform: "Epic".to_string(),
        };

        assert!(!state.update_local_player_identity(new_identity));
        assert_eq!(state.game.local_player_identity.load().name, "LockedPlayer");
    }

    #[test]
    fn test_detect_replays_path_runs_without_panic() {
        let _ = detect_replays_path();
    }

    #[test]
    fn app_state_uses_isolated_test_paths() {
        let first = AppState::new();
        let second = AppState::new();

        assert_ne!(first.paths.config_dir, second.paths.config_dir);
        assert_ne!(first.paths.config_path, second.paths.config_path);
        assert_ne!(
            first.paths.config_dir.join("history.sqlite3"),
            second.paths.config_dir.join("history.sqlite3")
        );
        assert_ne!(
            first.paths.config_dir.join("update"),
            second.paths.config_dir.join("update")
        );
    }

    #[test]
    fn test_config_compatibility_missing_lobby_display_mode() {
        let toml_str = r#"
            transparency = 150
            ui_scale = 2.2
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.lobby_display_mode, LobbyDisplayMode::Expanded);
        assert!(!config.debug_logging_enabled);
        assert!(!config.dashboard_enabled);
        assert_eq!(config.dashboard_monitor_index, 0);
        assert!(config.dashboard_fullscreen);
        assert!(!config.dashboard_open_with_overlay);
        assert!(config.dashboard_keep_overlay_enabled);
        assert!(config.dashboard_show_boost);
        assert!(config.dashboard_show_ranks);
        assert!(config.dashboard_show_team_comparison);
        assert!(config.dashboard_show_event_feed);
        assert!(config.dashboard_show_replay_upload);
        assert_eq!(config.dashboard_player_layout, DashboardPlayerLayout::Table);
        assert_eq!(config.dashboard_scale, 1.0);
        assert!(!config.debounce_touch_counters);
        assert!(!config.estimate_teammate_bumps);
    }

    #[test]
    fn test_config_dashboard_fields_round_trip() {
        let config = Config {
            dashboard_enabled: true,
            dashboard_monitor_index: 2,
            dashboard_fullscreen: false,
            dashboard_open_with_overlay: true,
            dashboard_keep_overlay_enabled: false,
            dashboard_show_boost: false,
            dashboard_show_ranks: false,
            dashboard_show_team_comparison: false,
            dashboard_show_event_feed: false,
            dashboard_show_replay_upload: false,
            dashboard_player_layout: DashboardPlayerLayout::Cards,
            estimate_teammate_bumps: true,
            ..Default::default()
        };

        let encoded = toml::to_string(&config).unwrap();
        let decoded: Config = toml::from_str(&encoded).unwrap();

        assert!(decoded.dashboard_enabled);
        assert_eq!(decoded.dashboard_monitor_index, 2);
        assert!(!decoded.dashboard_fullscreen);
        assert!(decoded.dashboard_open_with_overlay);
        assert!(!decoded.dashboard_keep_overlay_enabled);
        assert!(!decoded.dashboard_show_boost);
        assert!(!decoded.dashboard_show_ranks);
        assert!(!decoded.dashboard_show_team_comparison);
        assert!(!decoded.dashboard_show_event_feed);
        assert!(!decoded.dashboard_show_replay_upload);
        assert_eq!(
            decoded.dashboard_player_layout,
            DashboardPlayerLayout::Cards
        );
        assert!(decoded.estimate_teammate_bumps);
    }

    #[test]
    fn concurrent_config_updates_merge_and_persist() {
        let state = AppState::new();
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let first_state = state.clone();
        let first_barrier = barrier.clone();
        let first = std::thread::spawn(move || {
            first_barrier.wait();
            first_state.update_config(|config| config.hotkey_kb = "F8".to_string());
        });
        let second_state = state.clone();
        let second_barrier = barrier.clone();
        let second = std::thread::spawn(move || {
            second_barrier.wait();
            second_state.update_config(|config| config.dashboard_enabled = true);
        });

        barrier.wait();
        first.join().unwrap();
        second.join().unwrap();
        state.flush_config().unwrap();

        let memory = state.system.config.load();
        assert_eq!(memory.hotkey_kb, "F8");
        assert!(memory.dashboard_enabled);
        let persisted = load_config_file(&state.paths.config_path).unwrap();
        assert_eq!(persisted.hotkey_kb, "F8");
        assert!(persisted.dashboard_enabled);
    }
}

#[derive(Clone, Debug, Default)]
pub struct ConfigStatus {
    pub path: String,
    pub last_error: String,
}

impl ConfigStatus {
    fn new(path: PathBuf) -> Self {
        Self {
            path: path.display().to_string(),
            last_error: String::new(),
        }
    }
}

enum ConfigWriterMessage {
    Persist { revision: u64, config: Box<Config> },
    Flush(std::sync::mpsc::SyncSender<Result<(), String>>),
}

struct ConfigWriter {
    sender: std::sync::mpsc::Sender<ConfigWriterMessage>,
}

impl ConfigWriter {
    fn start(path: PathBuf, status: Arc<ArcSwap<ConfigStatus>>) -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || config_writer_loop(receiver, path, status));
        Self { sender }
    }

    fn persist(&self, revision: u64, config: Config) -> Result<(), String> {
        self.sender
            .send(ConfigWriterMessage::Persist {
                revision,
                config: Box::new(config),
            })
            .map_err(|_| "Config writer is unavailable.".to_string())
    }

    fn flush(&self) -> Result<(), String> {
        let (sender, receiver) = std::sync::mpsc::sync_channel(0);
        self.sender
            .send(ConfigWriterMessage::Flush(sender))
            .map_err(|_| "Config writer is unavailable.".to_string())?;
        receiver
            .recv()
            .map_err(|_| "Config writer stopped before flushing.".to_string())?
    }
}

fn config_writer_loop(
    receiver: std::sync::mpsc::Receiver<ConfigWriterMessage>,
    path: PathBuf,
    status: Arc<ArcSwap<ConfigStatus>>,
) {
    let mut last_result = Ok(());
    while let Ok(first) = receiver.recv() {
        let mut batch = vec![first];
        batch.extend(receiver.try_iter());
        let mut pending: Option<(u64, Box<Config>)> = None;

        for message in batch {
            match message {
                ConfigWriterMessage::Persist { revision, config } => {
                    pending = Some((revision, config));
                }
                ConfigWriterMessage::Flush(ack) => {
                    if let Some((revision, config)) = pending.take() {
                        last_result = persist_config_revision(&path, &status, revision, &config);
                    }
                    let _ = ack.send(last_result.clone());
                }
            }
        }

        if let Some((revision, config)) = pending {
            last_result = persist_config_revision(&path, &status, revision, &config);
        }
    }
}

fn persist_config_revision(
    path: &Path,
    status: &ArcSwap<ConfigStatus>,
    revision: u64,
    config: &Config,
) -> Result<(), String> {
    let mut next_status = ConfigStatus::new(path.to_path_buf());
    let result = config
        .save_to_path(path)
        .map_err(|error| format!("Revision {revision}: {error}"));
    next_status.last_error = result.as_ref().err().cloned().unwrap_or_default();
    status.store(Arc::new(next_status));
    result
}

#[derive(Clone, Debug, Default)]
pub struct VersionCheck {
    pub checked: bool,
    pub update_available: bool,
    pub latest_tag: String,
    pub release_url: String,
    pub windows_download_url: String,
    pub windows_checksum_url: String,
    pub windows_signature_url: String,
    pub error: String,
}

#[derive(Clone, Debug, Default)]
pub struct AutoUpdateStatus {
    pub running: bool,
    pub message: String,
    pub error: String,
}

use crate::mmr::TrackerSnapshot;

#[derive(Clone, Debug, Default)]
pub struct NetworkDiagnostics {
    pub transport: StatsApiTransport,
    pub last_event: String,
    pub last_event_unix_ms: u128,
    pub last_event_rate_estimate: String,
    pub last_roster_signature_change_unix_ms: u128,
    pub last_match_guid: String,
    pub last_result_signature: String,
    pub last_duplicate_result_suppression_reason: String,
    pub last_parse_error: String,
    pub last_connection_error: String,
}

#[derive(Clone, Debug, Default)]
pub struct DebugCaptureStatus {
    pub running: bool,
    pub seconds: u64,
    pub last_output_path: String,
    pub message: String,
    pub error: String,
}

#[derive(Clone, Debug, Default)]
pub struct ReplayUploadProgress {
    pub running: bool,
    pub paused: bool,
    pub stop_requested: bool,
    pub total: usize,
    pub processed: usize,
    pub uploaded: usize,
    pub skipped: usize,
    pub failed: usize,
    pub current_file: String,
    pub last_error: String,
    pub recent_events: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct PlayerInfo {
    pub name: String,
    pub primary_id: String,
    pub platform: String,
    pub team: u8,
    pub is_bot: bool,
    pub is_local: bool,
    pub boost: u8,
    pub boost_known: bool,
    pub score: u32,
    pub goals: u32,
    pub assists: u32,
    pub saves: u32,
    pub shots: u32,
    pub touches: u32,
    pub car_touches: u32,
    pub demos: u32,
    pub mmr: Option<TrackerSnapshot>,
}

#[derive(Clone, Debug, Default)]
pub struct DashboardMatchSnapshot {
    pub match_guid: String,
    pub players: HashMap<String, PlayerInfo>,
    pub session: SessionState,
    pub local_team: Option<u8>,
    pub team_bumps: [u32; 2],
}

#[derive(Clone, Copy, Debug)]
pub struct TouchCounterDebounce {
    pub accepted_touches: u32,
    pub last_touch_increment_at: Option<std::time::Instant>,
    pub accepted_car_touches: u32,
    pub last_car_touch_increment_at: Option<std::time::Instant>,
}

#[derive(Clone, Copy, Debug)]
pub struct TeammateBumpTouch {
    pub team: u8,
    pub at: std::time::Instant,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ReplayTouchOffset {
    pub touches: u32,
    pub car_touches: u32,
}

#[derive(Clone, Debug, Default)]
pub struct ReplayTouchOffsetState {
    pub match_guid: String,
    pub player_offsets: HashMap<String, ReplayTouchOffset>,
}

#[derive(Clone, Debug, Default)]
pub struct TeammateBumpEstimateState {
    pub pending: HashMap<String, TeammateBumpTouch>,
    pub team_bumps: [u32; 2],
}

impl TouchCounterDebounce {
    pub fn new(player: &PlayerInfo) -> Self {
        Self {
            accepted_touches: player.touches,
            last_touch_increment_at: None,
            accepted_car_touches: player.car_touches,
            last_car_touch_increment_at: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalPlayerIdentity {
    pub name: String,
    pub primary_id: String,
    pub platform: String,
}

impl LocalPlayerIdentity {
    pub fn is_known(&self) -> bool {
        !self.name.trim().is_empty()
            && !self.name.eq_ignore_ascii_case("player")
            && !self.primary_id.trim().is_empty()
            && !self.platform.trim().is_empty()
            && self.platform != "Unknown"
            && self.platform != "BOT"
    }

    pub fn same_account(&self, other: &Self) -> bool {
        self.primary_id.eq_ignore_ascii_case(&other.primary_id)
            && self.platform.eq_ignore_ascii_case(&other.platform)
    }
}

#[derive(Clone, Debug, Default)]
pub struct LocalMmrState {
    pub current: Option<TrackerSnapshot>,
    pub previous: Option<TrackerSnapshot>,
    pub fetching: bool,
    pub last_updated_unix_ms: u128,
    pub error: String,
}

pub const NO_TEAM: u8 = 255;

pub struct DiagnosticsState {
    pub frame_tracker: Arc<crate::diagnostics::SharedFrameTracker>,
    pub foreground_tracker: Arc<crate::diagnostics::ForegroundTracker>,
    pub resource_tracker: Arc<crate::diagnostics::ResourceTracker>,
    pub resource_poller: Arc<std::sync::Mutex<crate::diagnostics::ResourcePoller>>,
    pub alt_tab_diagnostics_status: Arc<std::sync::Mutex<String>>,
    pub debug_capture_status: ArcSwap<DebugCaptureStatus>,
}

pub struct ReplaysState {
    pub ballchasing_status: Arc<std::sync::Mutex<String>>,
    pub ballchasing_cloud_count: std::sync::atomic::AtomicU32,
    pub upload_progress: ArcSwap<ReplayUploadProgress>,
    pub upload_paused: AtomicBool,
    pub upload_stop_requested: AtomicBool,
    pub download_active: AtomicBool,
    pub metadata_cache: ArcSwap<crate::replay_metadata::ReplayMetadataSnapshot>,
    pub cloud_metadata_cache: ArcSwap<HashMap<String, crate::replay_metadata::ReplayMetadataEntry>>,
    pub metadata_scan_control: std::sync::Mutex<crate::replay_metadata::MetadataScanControl>,
    pub metadata_scan_running: AtomicBool,
    pub metadata_status: Arc<std::sync::Mutex<String>>,
    pub upload_running: AtomicBool,
    pub auto_upload_running: AtomicBool,
    pub sync_running: AtomicBool,
    pub initial_cache_sync_started: AtomicBool,
}

pub struct BoostState {
    pub boost_swap_status: Arc<std::sync::Mutex<String>>,
    pub boost_swap_inspection: ArcSwap<crate::assets::BoostSwapInspectionSnapshot>,
    pub inspection_running: AtomicBool,
}

pub struct HoopsFixerState {
    pub hoops_fixer_status: Arc<std::sync::Mutex<String>>,
    pub hoops_fixer_logs: Arc<std::sync::Mutex<Vec<String>>>,
    pub running: AtomicBool,
}

pub struct MmrState {
    pub xuid_gamertag_cache: Arc<std::sync::Mutex<HashMap<String, String>>>,
    pub debug_scrape_status: Arc<std::sync::Mutex<String>>,
    pub debug_tracker_logs: Arc<std::sync::Mutex<VecDeque<String>>>,
    pub local_mmr: ArcSwap<LocalMmrState>,
}

pub struct HistoryState {
    pub player_summaries: ArcSwap<HashMap<String, crate::history::PlayerHistorySummary>>,
    pub all_players_snapshot: ArcSwap<crate::history::HistoryPlayersSnapshot>,
    pub all_players_refresh_running: AtomicBool,
    pub totals: ArcSwap<crate::history::HistoryTotals>,
    pub status: Arc<std::sync::Mutex<String>>,
    pub revision: AtomicU64,
    pub conn: std::sync::Mutex<Option<rusqlite::Connection>>,
}

pub struct AppFlags {
    pub is_visible: AtomicBool,
    pub is_settings_visible: AtomicBool,
    pub is_connected: AtomicBool,
    pub is_launched: AtomicBool,
    pub should_exit: AtomicBool,
    pub is_watching_replay: AtomicBool,
}

pub struct HotkeyRecordingState {
    pub is_recording_kb: AtomicBool,
    pub is_recording_ctrl: AtomicBool,
    pub is_recording_settings: AtomicBool,
    pub is_recording_launch: AtomicBool,
    pub last_settings_hotkey_unix_ms: AtomicU64,
    pub last_launch_hotkey_unix_ms: AtomicU64,
    pub hud_keyboard_down: AtomicBool,
}

pub struct GameLobbyState {
    pub local_player_name: ArcSwap<String>,
    pub local_player_identity: ArcSwap<LocalPlayerIdentity>,
    pub local_team: std::sync::atomic::AtomicU8,
    pub players: ArcSwap<HashMap<String, PlayerInfo>>,
    pub touch_counter_debounce: std::sync::Mutex<HashMap<String, TouchCounterDebounce>>,
    pub teammate_bump_estimator: std::sync::Mutex<TeammateBumpEstimateState>,
    pub replay_touch_offsets: std::sync::Mutex<ReplayTouchOffsetState>,
    pub dashboard_match_snapshot: ArcSwap<DashboardMatchSnapshot>,
    pub match_roster: ArcSwap<HashMap<String, PlayerInfo>>,
    pub match_roster_guid: ArcSwap<String>,
    pub session: ArcSwap<SessionState>,
}

pub struct SystemState {
    pub config: ArcSwap<Config>,
    pub config_status: Arc<ArcSwap<ConfigStatus>>,
    pub version_check: ArcSwap<VersionCheck>,
    pub auto_update_status: ArcSwap<AutoUpdateStatus>,
    pub network_diagnostics: ArcSwap<NetworkDiagnostics>,
    pub stats_api_setup_status: ArcSwap<crate::setup::StatsApiSetupStatus>,
    pub stats_api_setup_refresh_running: AtomicBool,
    pub stats_api_setup_result: ArcSwap<StatsApiSetupResult>,
    pub http_client: Arc<wreq::Client>,
    pub ballchasing_client: Arc<wreq::Client>,
    pub release_url: ArcSwap<String>,
    pub release_public_key: ArcSwap<String>,
    pub is_simulating_input: AtomicBool,
}

pub struct AppState {
    pub paths: AppPaths,
    pub debug_enabled: bool,
    pub debug_logging_enabled: AtomicBool,
    pub flags: AppFlags,
    pub hotkeys: HotkeyRecordingState,
    pub game: GameLobbyState,
    pub system: SystemState,

    // Grouped Sub-states
    pub diagnostics: DiagnosticsState,
    pub replays: ReplaysState,
    pub boost: BoostState,
    pub hoops_fixer: HoopsFixerState,
    pub mmr: MmrState,
    pub history: HistoryState,

    config_update_mutex: std::sync::Mutex<()>,
    config_revision: AtomicU64,
    config_writer: ConfigWriter,
}

impl AppState {
    #[cfg(test)]
    pub fn new() -> Arc<Self> {
        Self::new_with_debug(false)
    }

    pub fn new_with_debug(debug_enabled: bool) -> Arc<Self> {
        let paths = AppPaths::resolve();
        let (config, config_status) = Config::load_from_path(paths.config_path.clone());
        let config_status = Arc::new(ArcSwap::from_pointee(config_status));
        let config_writer = ConfigWriter::start(paths.config_path.clone(), config_status.clone());
        let cached_local_player_identity = config.cached_local_player_identity.clone();
        let debug_logging_enabled = config.debug_logging_enabled;

        let http_client = Arc::new(
            wreq::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .emulation(wreq_util::Emulation::Chrome128)
                .redirect(wreq::redirect::Policy::limited(10))
                .build()
                .expect("Failed to build shared HTTP client"),
        );
        let ballchasing_client = Arc::new(
            wreq::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .emulation(wreq_util::Emulation::Chrome128)
                .redirect(wreq::redirect::Policy::none())
                .build()
                .expect("Failed to build Ballchasing HTTP client"),
        );

        let resource_tracker = Arc::new(crate::diagnostics::ResourceTracker::new());
        let resource_poller = Arc::new(std::sync::Mutex::new(
            crate::diagnostics::ResourcePoller::new(resource_tracker.clone()),
        ));

        let mut history_status = "History disabled.".to_string();
        let conn =
            match crate::history::initialize_database_at_with_recovery(paths.config_dir.clone()) {
                Ok((c, recovery_message)) => {
                    if let Some(message) = recovery_message {
                        history_status = message;
                    }
                    Some(c)
                }
                Err(e) => {
                    log::error!("Failed to initialize history database: {e}");
                    None
                }
            };

        Arc::new(Self {
            paths,
            debug_enabled,
            debug_logging_enabled: AtomicBool::new(debug_logging_enabled),
            flags: AppFlags {
                is_visible: AtomicBool::new(false),
                is_settings_visible: AtomicBool::new(true),
                is_connected: AtomicBool::new(false),
                is_launched: AtomicBool::new(false),
                should_exit: AtomicBool::new(false),
                is_watching_replay: AtomicBool::new(false),
            },
            hotkeys: HotkeyRecordingState {
                is_recording_kb: AtomicBool::new(false),
                is_recording_ctrl: AtomicBool::new(false),
                is_recording_settings: AtomicBool::new(false),
                is_recording_launch: AtomicBool::new(false),
                last_settings_hotkey_unix_ms: AtomicU64::new(0),
                last_launch_hotkey_unix_ms: AtomicU64::new(0),
                hud_keyboard_down: AtomicBool::new(false),
            },
            game: GameLobbyState {
                local_player_name: ArcSwap::from_pointee("".to_string()),
                local_player_identity: ArcSwap::from_pointee(cached_local_player_identity),
                local_team: std::sync::atomic::AtomicU8::new(NO_TEAM),
                players: ArcSwap::from_pointee(HashMap::new()),
                touch_counter_debounce: std::sync::Mutex::new(HashMap::new()),
                teammate_bump_estimator: std::sync::Mutex::new(TeammateBumpEstimateState::default()),
                replay_touch_offsets: std::sync::Mutex::new(ReplayTouchOffsetState::default()),
                dashboard_match_snapshot: ArcSwap::from_pointee(DashboardMatchSnapshot::default()),
                match_roster: ArcSwap::from_pointee(HashMap::new()),
                match_roster_guid: ArcSwap::from_pointee(String::new()),
                session: ArcSwap::from_pointee(SessionState::default()),
            },
            system: SystemState {
                config: ArcSwap::from_pointee(config),
                config_status,
                version_check: ArcSwap::from_pointee(VersionCheck::default()),
                auto_update_status: ArcSwap::from_pointee(AutoUpdateStatus::default()),
                network_diagnostics: ArcSwap::from_pointee(NetworkDiagnostics::default()),
                stats_api_setup_status: ArcSwap::from_pointee(crate::setup::StatsApiSetupStatus {
                    message: "Checking Stats API config...".to_string(),
                    ..Default::default()
                }),
                stats_api_setup_refresh_running: AtomicBool::new(false),
                stats_api_setup_result: ArcSwap::from_pointee(StatsApiSetupResult::default()),
                http_client,
                ballchasing_client,
                release_url: ArcSwap::from_pointee(release_url()),
                release_public_key: ArcSwap::from_pointee(release_signing_public_key()),
                is_simulating_input: AtomicBool::new(false),
            },
            diagnostics: DiagnosticsState {
                frame_tracker: Arc::new(crate::diagnostics::SharedFrameTracker::new(60)),
                foreground_tracker: Arc::new(crate::diagnostics::ForegroundTracker::new()),
                resource_tracker,
                resource_poller,
                alt_tab_diagnostics_status: Arc::new(std::sync::Mutex::new("Idle".to_string())),
                debug_capture_status: ArcSwap::from_pointee(DebugCaptureStatus::default()),
            },
            replays: ReplaysState {
                ballchasing_status: Arc::new(std::sync::Mutex::new("Idle".to_string())),
                ballchasing_cloud_count: std::sync::atomic::AtomicU32::new(0),
                upload_progress: ArcSwap::from_pointee(ReplayUploadProgress::default()),
                upload_paused: AtomicBool::new(false),
                upload_stop_requested: AtomicBool::new(false),
                download_active: AtomicBool::new(false),
                metadata_cache: ArcSwap::from_pointee(
                    crate::replay_metadata::ReplayMetadataSnapshot::default(),
                ),
                cloud_metadata_cache: ArcSwap::from_pointee(HashMap::new()),
                metadata_scan_control: std::sync::Mutex::new(
                    crate::replay_metadata::MetadataScanControl::default(),
                ),
                metadata_scan_running: AtomicBool::new(false),
                metadata_status: Arc::new(std::sync::Mutex::new("Not scanned".to_string())),
                upload_running: AtomicBool::new(false),
                auto_upload_running: AtomicBool::new(false),
                sync_running: AtomicBool::new(false),
                initial_cache_sync_started: AtomicBool::new(false),
            },
            boost: BoostState {
                boost_swap_status: Arc::new(std::sync::Mutex::new("Idle".to_string())),
                boost_swap_inspection: ArcSwap::from_pointee(
                    crate::assets::BoostSwapInspectionSnapshot::default(),
                ),
                inspection_running: AtomicBool::new(false),
            },
            hoops_fixer: HoopsFixerState {
                hoops_fixer_status: Arc::new(std::sync::Mutex::new("Idle".to_string())),
                hoops_fixer_logs: Arc::new(std::sync::Mutex::new(Vec::new())),
                running: AtomicBool::new(false),
            },
            mmr: MmrState {
                xuid_gamertag_cache: Arc::new(std::sync::Mutex::new(HashMap::new())),
                debug_scrape_status: Arc::new(std::sync::Mutex::new("Idle".to_string())),
                debug_tracker_logs: Arc::new(std::sync::Mutex::new(VecDeque::new())),
                local_mmr: ArcSwap::from_pointee(LocalMmrState::default()),
            },
            history: HistoryState {
                player_summaries: ArcSwap::from_pointee(HashMap::new()),
                all_players_snapshot: ArcSwap::from_pointee(
                    crate::history::HistoryPlayersSnapshot::default(),
                ),
                all_players_refresh_running: AtomicBool::new(false),
                totals: ArcSwap::from_pointee(crate::history::HistoryTotals::default()),
                status: Arc::new(std::sync::Mutex::new(history_status)),
                revision: AtomicU64::new(0),
                conn: std::sync::Mutex::new(conn),
            },
            config_update_mutex: std::sync::Mutex::new(()),
            config_revision: AtomicU64::new(0),
            config_writer,
        })
    }

    pub fn update_config<R>(&self, update: impl FnOnce(&mut Config) -> R) -> R {
        let _guard = self
            .config_update_mutex
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut config = (**self.system.config.load()).clone();
        let result = update(&mut config);
        self.publish_config(config);
        result
    }

    pub fn replace_config(&self, config: Config) {
        let _guard = self
            .config_update_mutex
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        self.publish_config(config);
    }

    pub(crate) fn begin_config_edit(&self) -> ConfigEditSession<'_> {
        let guard = self
            .config_update_mutex
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let config = (**self.system.config.load()).clone();
        ConfigEditSession {
            state: self,
            _guard: guard,
            config,
        }
    }

    pub fn flush_config(&self) -> Result<(), String> {
        self.config_writer.flush()
    }

    fn publish_config(&self, config: Config) {
        self.debug_logging_enabled.store(
            config.debug_logging_enabled,
            std::sync::atomic::Ordering::SeqCst,
        );
        self.system.config.store(Arc::new(config.clone()));
        let revision = self
            .config_revision
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        if let Err(error) = self.config_writer.persist(revision, config) {
            let mut status = ConfigStatus::new(self.paths.config_path.clone());
            status.last_error = error;
            self.system.config_status.store(Arc::new(status));
        }
    }

    pub fn update_local_player_identity(self: &Arc<Self>, identity: LocalPlayerIdentity) -> bool {
        if !identity.is_known() {
            return false;
        }

        let config = self.system.config.load();
        if config.lock_local_player {
            return false;
        }

        let current_identity = self.game.local_player_identity.load();
        if current_identity.is_known()
            && current_identity.same_account(&identity)
            && current_identity.name == identity.name
        {
            return false;
        }
        let first_known_identity = !current_identity.is_known();

        self.game
            .local_player_identity
            .store(Arc::new(identity.clone()));

        self.update_config(|config| {
            if !config.cached_local_player_identity.is_known()
                || !config.cached_local_player_identity.same_account(&identity)
                || config.cached_local_player_identity.name != identity.name
            {
                config.cached_local_player_identity = identity;
            }
        });

        first_known_identity
    }
}

pub(crate) struct ConfigEditSession<'a> {
    state: &'a AppState,
    _guard: std::sync::MutexGuard<'a, ()>,
    config: Config,
}

impl ConfigEditSession<'_> {
    pub fn snapshot(&self) -> &Config {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    pub fn commit(self) {
        self.state.publish_config(self.config);
    }
}
