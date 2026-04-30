use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::fs;
use std::path::Path;
use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};

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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            transparency: 150,
            ui_scale: 1.0,
            show_bots: true,
            window_size: [1920.0, 1080.0],
            anchor: AnchorPos::CenterRight,
            monitor_index: 0,
            hotkey_kb: "Backspace".to_string(),
            hotkey_ctrl: "Select".to_string(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = Path::new("config.toml");
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(config) = toml::from_str(&content) {
                    return config;
                }
            }
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

#[derive(Clone, Debug, Default)]
pub struct PlayerInfo {
    pub name: String,
    pub platform: String,
    pub team: u8,
    pub is_bot: bool,
}

pub struct AppState {
    pub is_visible: AtomicBool,
    pub is_connected: AtomicBool,
    pub is_launched: AtomicBool,
    pub is_recording_kb: AtomicBool,
    pub is_recording_ctrl: AtomicBool,
    pub players: ArcSwap<HashMap<String, PlayerInfo>>,
    pub config: ArcSwap<Config>,
}

impl AppState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            is_visible: AtomicBool::new(false),
            is_connected: AtomicBool::new(false),
            is_launched: AtomicBool::new(false),
            is_recording_kb: AtomicBool::new(false),
            is_recording_ctrl: AtomicBool::new(false),
            players: ArcSwap::from_pointee(HashMap::new()),
            config: ArcSwap::from_pointee(Config::load()),
        })
    }
}
