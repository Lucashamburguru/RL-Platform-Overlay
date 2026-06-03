use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum AnchorPos {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    CenterRight,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum TeammateBoostDisplay {
    #[default]
    Bars,
    Circles,
    Compact,
    Numbers,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub transparency: u8,
    pub ui_scale: f32,
    pub show_bots: bool,
    pub window_size: [f32; 2],
    pub anchor: AnchorPos,
    pub monitor_index: usize,
    pub hotkey_kb: String,
    pub hotkey_ctrl: String,
    pub hotkey_settings: String,
    pub hotkey_toggle: bool,
    pub show_stats: bool,
    pub show_teammate_boost: bool,
    pub teammate_boost_offset: f32,
    pub teammate_boost_horizontal_offset: f32,
    pub teammate_hud_scale: f32,
    pub teammate_boost_display: TeammateBoostDisplay,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            transparency: 150,
            ui_scale: 2.2,
            show_bots: true,
            window_size: [1920.0, 1080.0],
            anchor: AnchorPos::CenterRight,
            monitor_index: 0,
            hotkey_kb: "Backspace".to_string(),
            hotkey_ctrl: "Select".to_string(),
            hotkey_settings: "F1".to_string(),
            hotkey_toggle: false,
            show_stats: true,
            show_teammate_boost: false,
            teammate_boost_offset: 180.0,
            teammate_boost_horizontal_offset: 110.0,
            teammate_hud_scale: 2.2,
            teammate_boost_display: TeammateBoostDisplay::Bars,
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
    config_dir().map_or_else(
        || PathBuf::from("config.toml"),
        |dir| dir.join("config.toml"),
    )
}

fn config_dir() -> Option<PathBuf> {
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
        assert_eq!(config.anchor, AnchorPos::CenterRight);
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
    pub error: String,
}

use crate::mmr::TrackerSnapshot;

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
    pub mmr: Option<TrackerSnapshot>,
}

pub struct AppState {
    pub is_visible: AtomicBool,
    pub is_settings_visible: AtomicBool,
    pub is_connected: AtomicBool,
    pub is_launched: AtomicBool,
    pub is_recording_kb: AtomicBool,
    pub is_recording_ctrl: AtomicBool,
    pub is_recording_settings: AtomicBool,
    pub local_player_name: ArcSwap<String>,
    pub local_team: std::sync::atomic::AtomicU8,
    pub players: ArcSwap<HashMap<String, PlayerInfo>>,
    pub config: ArcSwap<Config>,
    pub config_status: ArcSwap<ConfigStatus>,
    pub version_check: ArcSwap<VersionCheck>,
}

impl AppState {
    pub fn new() -> Arc<Self> {
        let (config, config_status) = Config::load();
        Arc::new(Self {
            is_visible: AtomicBool::new(false),
            is_settings_visible: AtomicBool::new(true),
            is_connected: AtomicBool::new(false),
            is_launched: AtomicBool::new(false),
            is_recording_kb: AtomicBool::new(false),
            is_recording_ctrl: AtomicBool::new(false),
            is_recording_settings: AtomicBool::new(false),
            local_player_name: ArcSwap::from_pointee("".to_string()),
            local_team: std::sync::atomic::AtomicU8::new(255),
            players: ArcSwap::from_pointee(HashMap::new()),
            config: ArcSwap::from_pointee(config),
            config_status: ArcSwap::from_pointee(config_status),
            version_check: ArcSwap::from_pointee(VersionCheck::default()),
        })
    }

    pub fn save_config(&self, config: Config) {
        let mut status = ConfigStatus::new(config_path());
        if let Err(error) = config.save() {
            status.last_error = error;
        }
        self.config_status.store(Arc::new(status));
        self.config.store(Arc::new(config));
    }
}
