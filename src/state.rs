use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
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

#[derive(Clone, Debug, Serialize, Deserialize)]
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
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = Path::new("config.toml");
        if path.exists()
            && let Ok(content) = fs::read_to_string(path)
            && let Ok(config) = toml::from_str(&content)
        {
            return config;
        }
        let config = Config::default();
        config.save();
        config
    }

    pub fn save(&self) {
        if let Ok(content) = toml::to_string_pretty(self) {
            let _ = fs::write("config.toml", content);
        }
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
pub struct PlayerInfo {
    pub name: String,
    pub platform: String,
    pub team: u8,
    pub is_bot: bool,
    pub is_local: bool,
    pub boost: u8,
    pub score: u32,
    pub goals: u32,
    pub saves: u32,
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
}

impl AppState {
    pub fn new() -> Arc<Self> {
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
            config: ArcSwap::from_pointee(Config::load()),
        })
    }
}
