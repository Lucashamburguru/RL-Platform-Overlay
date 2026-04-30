use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use arc_swap::ArcSwap;

#[derive(Clone, Debug, Default)]
pub struct PlayerInfo {
    pub name: String,
    pub platform: String,
    pub team: u8,
    pub is_bot: bool,
}

pub struct AppState {
    pub is_visible: AtomicBool,
    pub players: ArcSwap<HashMap<String, PlayerInfo>>,
}

impl AppState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            is_visible: AtomicBool::new(false),
            players: ArcSwap::from_pointee(HashMap::new()),
        })
    }
}
