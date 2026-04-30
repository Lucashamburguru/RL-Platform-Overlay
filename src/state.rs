use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use arc_swap::ArcSwap;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum AnchorPos {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    CenterRight,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub transparency: u8,
    pub ui_scale: f32,
    pub show_bots: bool,
    pub window_size: [f32; 2],
    pub anchor: AnchorPos,
    pub monitor_index: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            transparency: 150,
            ui_scale: 1.0,
            show_bots: true,
            window_size: [1920.0, 1080.0],
            anchor: AnchorPos::TopLeft,
            monitor_index: 0,
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
    pub players: ArcSwap<HashMap<String, PlayerInfo>>,
    pub config: ArcSwap<Config>,
}

impl AppState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            is_visible: AtomicBool::new(false),
            is_connected: AtomicBool::new(false),
            is_launched: AtomicBool::new(false),
            players: ArcSwap::from_pointee(HashMap::new()),
            config: ArcSwap::from_pointee(Config::default()),
        })
    }
}
