# Rocket League Platform Overlay Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a lightweight, cross-platform Rust overlay that shows player platforms in Rocket League lobbies, triggered by holding "Backspace" or "Select".

**Architecture:** A multi-threaded Rust application using `eframe` for the UI, `tokio` for async WebSocket communication with the game's Stats API, and global input listeners for the visibility trigger.

**Tech Stack:** Rust, eframe (egui), tokio, tokio-tungstenite, gilrs, rdev, serde.

---

### Task 1: Project Initialization

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/state.rs`

- [ ] **Step 1: Create Cargo.toml with dependencies**
```toml
[package]
name = "rl-platform-overlay"
version = "0.1.0"
edition = "2021"

[dependencies]
eframe = "0.27.2"
egui = "0.27.2"
tokio = { version = "1.37", features = ["full"] }
tokio-tungstenite = "0.21.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
gilrs = "0.10"
rdev = "0.5.3"
arc-swap = "1.7"
futures-util = "0.3"
```

- [ ] **Step 2: Initialize src/state.rs**
```rust
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
```

- [ ] **Step 3: Commit initialization**
```bash
git add Cargo.toml src/state.rs
git commit -m "chore: project initialization"
```

---

### Task 2: Network Layer (WebSocket Client)

**Files:**
- Create: `src/network.rs`

- [ ] **Step 1: Implement WebSocket listener and parser**
```rust
use crate::state::{AppState, PlayerInfo};
use std::collections::HashMap;
use std::sync::Arc;
use tokio_tungstenite::connect_async;
use futures_util::StreamExt;
use serde_json::Value;

pub async fn start_network_task(state: Arc<AppState>) {
    let url = "ws://127.0.0.1:49123";
    loop {
        if let Ok((mut ws_stream, _)) = connect_async(url).await {
            while let Some(msg) = ws_stream.next().await {
                if let Ok(msg) = msg {
                    if let Ok(text) = msg.to_text() {
                        if let Ok(json) = serde_json::from_str::<Value>(text) {
                            if json["Event"] == "UpdateState" {
                                handle_update_state(&state, &json["Data"]);
                            }
                        }
                    }
                }
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

fn handle_update_state(state: &Arc<AppState>, data: &Value) {
    let mut new_players = HashMap::new();
    if let Some(players) = data["Players"].as_array() {
        for p in players {
            let name = p["Name"].as_str().unwrap_or("Unknown").to_string();
            let primary_id = p["PrimaryId"].as_str().unwrap_or("");
            let (platform, is_bot) = parse_platform(primary_id);
            let team = p["TeamNum"].as_u64().unwrap_or(0) as u8;
            
            new_players.insert(name.clone(), PlayerInfo {
                name,
                platform,
                team,
                is_bot,
            });
        }
    }
    state.players.store(Arc::new(new_players));
}

fn parse_platform(id: &str) -> (String, bool) {
    let parts: Vec<&str> = id.split('|').collect();
    if parts.is_empty() { return ("Unknown".to_string(), false); }
    let platform = parts[0];
    match platform {
        "Steam" => ("Steam".to_string(), false),
        "Epic" => ("Epic".to_string(), false),
        "Ps4" | "Ps5" => ("PlayStation".to_string(), false),
        "Xbox" | "XBoxOne" => ("Xbox".to_string(), false),
        "Switch" => ("Switch".to_string(), false),
        "Bot" => ("BOT".to_string(), true),
        _ => (platform.to_string(), false),
    }
}
```

- [ ] **Step 2: Commit network layer**
```bash
git add src/network.rs
git commit -m "feat: implement websocket network task"
```

---

### Task 3: Input Layer (Keyboard & Controller)

**Files:**
- Create: `src/input.rs`

- [ ] **Step 1: Implement Input Listeners**
```rust
use crate::state::AppState;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use gilrs::{Gilrs, Event, Button};
use rdev::{listen, EventType, Key};

pub fn start_input_tasks(state: Arc<AppState>) {
    let state_ctrl = state.clone();
    std::thread::spawn(move || {
        let mut gilrs = Gilrs::new().unwrap();
        loop {
            while let Some(Event { event, .. }) = gilrs.next_event() {
                match event {
                    gilrs::EventType::ButtonPressed(Button::Select, _) => {
                        state_ctrl.is_visible.store(true, Ordering::SeqCst);
                    }
                    gilrs::EventType::ButtonReleased(Button::Select, _) => {
                        state_ctrl.is_visible.store(false, Ordering::SeqCst);
                    }
                    _ => {}
                }
            }
        }
    });

    let state_kb = state.clone();
    std::thread::spawn(move || {
        listen(move |event| {
            match event.event_type {
                EventType::KeyPress(Key::Backspace) => {
                    state_kb.is_visible.store(true, Ordering::SeqCst);
                }
                EventType::KeyRelease(Key::Backspace) => {
                    state_kb.is_visible.store(false, Ordering::SeqCst);
                }
                _ => {}
            }
        }).unwrap();
    });
}
```

- [ ] **Step 2: Commit input layer**
```bash
git add src/input.rs
git commit -m "feat: implement global input listeners"
```

---

### Task 4: UI Layer (egui/eframe)

**Files:**
- Create: `src/ui.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Implement UI Rendering**
```rust
use crate::state::AppState;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use eframe::egui;

pub struct OverlayApp {
    state: Arc<AppState>,
}

impl OverlayApp {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

impl eframe::App for OverlayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.state.is_visible.load(Ordering::SeqCst) {
            ctx.request_repaint();
            return;
        }

        let players = self.state.players.load();
        
        egui::Area::new("overlay")
            .anchor(egui::Align2::RIGHT_CENTER, egui::vec2(-20.0, 0.0))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(egui::Color32::from_black_alpha(150))
                    .rounding(5.0)
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.heading("Lobby Platforms");
                        ui.add_space(5.0);
                        
                        let mut sorted_players: Vec<_> = players.values().collect();
                        sorted_players.sort_by_key(|p| p.team);

                        for p in sorted_players {
                            ui.horizontal(|ui| {
                                let color = if p.team == 0 {
                                    egui::Color32::from_rgb(0, 100, 255)
                                } else {
                                    egui::Color32::from_rgb(255, 140, 0)
                                };
                                ui.label(egui::RichText::new("■").color(color));
                                ui.label(&p.name);
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(egui::RichText::new(&p.platform).strong());
                                });
                            });
                        }
                    });
            });
        
        ctx.request_repaint();
    }
}
```

- [ ] **Step 2: Finish main.rs wiring**
```rust
mod state;
mod network;
mod input;
mod ui;

use crate::state::AppState;
use crate::ui::OverlayApp;
use eframe::egui;

#[tokio::main]
async fn main() -> eframe::Result<()> {
    let state = AppState::new();
    
    // Start background tasks
    let net_state = state.clone();
    tokio::spawn(async move {
        network::start_network_task(net_state).await;
    });
    
    input::start_input_tasks(state.clone());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_transparent(true)
            .with_always_on_top()
            .with_decorations(false)
            .with_active(false) // Don't steal focus
            .with_mouse_passthrough(true),
        ..Default::default()
    };

    eframe::run_native(
        "RL Platform Overlay",
        options,
        Box::new(|_cc| Box::new(OverlayApp::new(state))),
    )
}
```

- [ ] **Step 3: Commit UI layer and main**
```bash
git add src/ui.rs src/main.rs
git commit -m "feat: implement overlay UI and main loop"
```
