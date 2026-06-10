use crate::session::{SessionOverlayDisplay, SessionState};
use crate::setup::StatsApiSetupResult;
use crate::stats_api::StatsApiTransport;
use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
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
    pub hotkey_kb: String,
    pub hotkey_ctrl: String,
    pub hotkey_settings: String,
    pub hotkey_launch: String,
    pub hotkey_toggle: bool,
    pub show_stats: bool,
    pub show_teammate_boost: bool,
    pub show_lobby_matches: bool,
    pub show_lobby_ranks: bool,
    pub teammate_hud_scale: f32,
    pub teammate_boost_display: TeammateBoostDisplay,
    pub rocket_league_path: String,
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
    pub ballchasing_enabled: bool,
    pub ballchasing_api_key: String,
    pub ballchasing_visibility: String,
    pub replays_folder: String,
    pub uploaded_replays: std::collections::HashSet<String>,
    pub auto_gg: bool,
    pub auto_gg_sequence: String,
    pub auto_freeplay: bool,
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
            hotkey_kb: "Backspace".to_string(),
            hotkey_ctrl: "Select".to_string(),
            hotkey_settings: "F1".to_string(),
            hotkey_launch: "F4".to_string(),
            hotkey_toggle: false,
            show_stats: true,
            show_teammate_boost: false,
            show_lobby_matches: false,
            show_lobby_ranks: true,
            teammate_hud_scale: 2.2,
            teammate_boost_display: TeammateBoostDisplay::Bars,
            rocket_league_path,
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
            ballchasing_enabled: false,
            ballchasing_api_key: "".to_string(),
            ballchasing_visibility: "public".to_string(),
            replays_folder: detect_replays_path().unwrap_or_default(),
            uploaded_replays: std::collections::HashSet::new(),
            auto_gg: false,
            auto_gg_sequence: "T,G,G,Enter".to_string(),
            auto_freeplay: false,
        }
    }
}

impl Config {
    pub fn load() -> (Self, ConfigStatus) {
        let path = config_path();
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
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create config directory: {error}"))?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|error| format!("Could not serialize config: {error}"))?;
        fs::write(&path, content).map_err(|error| format!("Could not save config: {error}"))
    }
}

fn load_config_file(path: &PathBuf) -> Result<Config, String> {
    let content =
        fs::read_to_string(path).map_err(|error| format!("Could not read config: {error}"))?;
    toml::from_str(&content).map_err(|error| format!("Could not parse config: {error}"))
}

fn config_path() -> PathBuf {
    if std::env::var("RL_OVERLAY_TEST").is_ok() {
        return std::env::temp_dir().join("rl_platform_overlay_config_test.toml");
    }
    #[cfg(test)]
    {
        std::env::temp_dir().join("rl_platform_overlay_config_test.toml")
    }
    #[cfg(not(test))]
    {
        config_dir().map_or_else(
            || PathBuf::from("config.toml"),
            |dir| dir.join("config.toml"),
        )
    }
}

pub fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("RL-Platform-Overlay"))
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
            return Some(PathBuf::from(path).join("rl-platform-overlay"));
        }
        env::var_os("HOME")
            .map(PathBuf::from)
            .map(|path| path.join(".config").join("rl-platform-overlay"))
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
    fn test_detect_replays_path_runs_without_panic() {
        let _ = detect_replays_path();
    }

    #[test]
    fn test_config_compatibility_missing_lobby_display_mode() {
        let toml_str = r#"
            transparency = 150
            ui_scale = 2.2
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.lobby_display_mode, LobbyDisplayMode::Expanded);
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

#[derive(Clone, Debug, Default)]
pub struct VersionCheck {
    pub checked: bool,
    pub update_available: bool,
    pub latest_tag: String,
    pub release_url: String,
    pub windows_download_url: String,
    pub windows_checksum_url: String,
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
pub struct PlayerInfo {
    pub name: String,
    pub primary_id: String,
    pub platform: String,
    pub team: u8,
    pub is_bot: bool,
    pub is_local: bool,
    pub boost: u8,
    pub score: u32,
    pub goals: u32,
    pub saves: u32,
    pub touches: u32,
    pub car_touches: u32,
    pub demos: u32,
    pub mmr: Option<TrackerSnapshot>,
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
}

pub struct BoostState {
    pub boost_swap_status: Arc<std::sync::Mutex<String>>,
}

pub struct HoopsFixerState {
    pub hoops_fixer_status: Arc<std::sync::Mutex<String>>,
    pub hoops_fixer_logs: Arc<std::sync::Mutex<Vec<String>>>,
}

pub struct MmrState {
    pub mmr_client: Arc<wreq::Client>,
    pub xuid_gamertag_cache: Arc<std::sync::Mutex<HashMap<String, String>>>,
    pub debug_scrape_status: Arc<std::sync::Mutex<String>>,
    pub debug_tracker_logs: Arc<std::sync::Mutex<Vec<String>>>,
    pub local_mmr: ArcSwap<LocalMmrState>,
}

pub struct AppState {
    pub debug_enabled: bool,
    pub is_visible: AtomicBool,
    pub is_settings_visible: AtomicBool,
    pub is_connected: AtomicBool,
    pub is_launched: AtomicBool,
    pub is_recording_kb: AtomicBool,
    pub is_recording_ctrl: AtomicBool,
    pub is_recording_settings: AtomicBool,
    pub is_recording_launch: AtomicBool,
    pub last_settings_hotkey_unix_ms: AtomicU64,
    pub last_launch_hotkey_unix_ms: AtomicU64,
    pub local_player_name: ArcSwap<String>,
    pub local_player_identity: ArcSwap<LocalPlayerIdentity>,
    pub local_team: std::sync::atomic::AtomicU8,
    pub players: ArcSwap<HashMap<String, PlayerInfo>>,
    pub config: ArcSwap<Config>,
    pub config_status: ArcSwap<ConfigStatus>,
    pub version_check: ArcSwap<VersionCheck>,
    pub auto_update_status: ArcSwap<AutoUpdateStatus>,
    pub network_diagnostics: ArcSwap<NetworkDiagnostics>,
    pub stats_api_setup_result: ArcSwap<StatsApiSetupResult>,
    pub session: ArcSwap<SessionState>,

    // Grouped Sub-states
    pub diagnostics: DiagnosticsState,
    pub replays: ReplaysState,
    pub boost: BoostState,
    pub hoops_fixer: HoopsFixerState,
    pub mmr: MmrState,
}

impl AppState {
    #[cfg(test)]
    pub fn new() -> Arc<Self> {
        Self::new_with_debug(false)
    }

    pub fn new_with_debug(debug_enabled: bool) -> Arc<Self> {
        let (config, config_status) = Config::load();
        let cached_local_player_identity = config.cached_local_player_identity.clone();

        let mmr_client = wreq::Client::builder()
            .timeout(std::time::Duration::from_secs(7))
            .emulation(wreq_util::Emulation::Chrome128)
            .build()
            .expect("Failed to build MMR HTTP client with Chrome emulation");

        let resource_tracker = Arc::new(crate::diagnostics::ResourceTracker::new());
        let resource_poller = Arc::new(std::sync::Mutex::new(
            crate::diagnostics::ResourcePoller::new(resource_tracker.clone()),
        ));

        Arc::new(Self {
            debug_enabled,
            is_visible: AtomicBool::new(false),
            is_settings_visible: AtomicBool::new(true),
            is_connected: AtomicBool::new(false),
            is_launched: AtomicBool::new(false),
            is_recording_kb: AtomicBool::new(false),
            is_recording_ctrl: AtomicBool::new(false),
            is_recording_settings: AtomicBool::new(false),
            is_recording_launch: AtomicBool::new(false),
            last_settings_hotkey_unix_ms: AtomicU64::new(0),
            last_launch_hotkey_unix_ms: AtomicU64::new(0),
            local_player_name: ArcSwap::from_pointee("".to_string()),
            local_player_identity: ArcSwap::from_pointee(cached_local_player_identity),
            local_team: std::sync::atomic::AtomicU8::new(NO_TEAM),
            players: ArcSwap::from_pointee(HashMap::new()),
            config: ArcSwap::from_pointee(config),
            config_status: ArcSwap::from_pointee(config_status),
            version_check: ArcSwap::from_pointee(VersionCheck::default()),
            auto_update_status: ArcSwap::from_pointee(AutoUpdateStatus::default()),
            network_diagnostics: ArcSwap::from_pointee(NetworkDiagnostics::default()),
            stats_api_setup_result: ArcSwap::from_pointee(StatsApiSetupResult::default()),
            session: ArcSwap::from_pointee(SessionState::default()),
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
            },
            boost: BoostState {
                boost_swap_status: Arc::new(std::sync::Mutex::new("Idle".to_string())),
            },
            hoops_fixer: HoopsFixerState {
                hoops_fixer_status: Arc::new(std::sync::Mutex::new("Idle".to_string())),
                hoops_fixer_logs: Arc::new(std::sync::Mutex::new(Vec::new())),
            },
            mmr: MmrState {
                mmr_client: Arc::new(mmr_client),
                xuid_gamertag_cache: Arc::new(std::sync::Mutex::new(HashMap::new())),
                debug_scrape_status: Arc::new(std::sync::Mutex::new("Idle".to_string())),
                debug_tracker_logs: Arc::new(std::sync::Mutex::new(Vec::new())),
                local_mmr: ArcSwap::from_pointee(LocalMmrState::default()),
            },
        })
    }

    pub fn save_config(&self, config: Config) {
        let mut status = ConfigStatus::new(config_path());
        if let Err(error) = config.save() {
            status.last_error = error;
        }
        self.config.store(Arc::new(config));
        self.config_status.store(Arc::new(status));
    }

    pub fn update_local_player_identity(&self, identity: LocalPlayerIdentity) -> bool {
        if !identity.is_known() {
            return false;
        }

        let current_identity = self.local_player_identity.load();
        if current_identity.is_known()
            && current_identity.same_account(&identity)
            && current_identity.name == identity.name
        {
            return false;
        }
        let first_known_identity = !current_identity.is_known();

        self.local_player_identity.store(Arc::new(identity.clone()));

        let mut config = (**self.config.load()).clone();
        if !config.cached_local_player_identity.is_known()
            || !config.cached_local_player_identity.same_account(&identity)
            || config.cached_local_player_identity.name != identity.name
        {
            config.cached_local_player_identity = identity;
            self.save_config(config);
        }

        first_known_identity
    }
}
