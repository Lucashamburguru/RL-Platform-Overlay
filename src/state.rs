use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use arc_swap::ArcSwap;

#[derive(Clone, Debug)]
pub struct Config {
    pub transparency: u8,
    pub ui_scale: f32,
    pub show_bots: bool,
    pub window_size: [f32; 2],
}

impl Default for Config {
    fn default() -> Self {
        Self {
            transparency: 150,
            ui_scale: 1.0,
            show_bots: true,
            window_size: [1920.0, 1080.0],
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
    pub players: ArcSwap<HashMap<String, PlayerInfo>>,
    pub config: ArcSwap<Config>,
}

impl AppState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            is_visible: AtomicBool::new(false),
            is_connected: AtomicBool::new(false),
            players: ArcSwap::from_pointee(HashMap::new()),
            config: ArcSwap::from_pointee(Config::default()),
        })
    }
}
