# Settings GUI & Multi-Window Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a secondary "Settings" window that allows real-time configuration of the overlay's transparency, scale, and trigger keys.

**Architecture:** Use `egui` Viewports to manage two windows from a single process. Expand `AppState` to include `ArcSwap<Config>` for atomic updates of settings across windows.

**Tech Stack:** Rust, eframe, egui, arc-swap.

---

### Task 1: Expand AppState with Config

**Files:**
- Modify: `src/state.rs`

- [ ] **Step 1: Define Config struct and update AppState**
```rust
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
```

- [ ] **Step 2: Commit State changes**
```bash
git add src/state.rs
git commit -m "feat: add Config to AppState"
```

---

### Task 2: Multi-Window Main Loop

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Update main to handle two windows**
```rust
#[tokio::main]
async fn main() -> eframe::Result<()> {
    let state = AppState::new();
    
    // ... background tasks (keep as is) ...

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("RL Overlay Settings")
            .with_inner_size([400.0, 300.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Rocket League Overlay",
        options,
        Box::new(|cc| {
            // This is the primary window (Settings)
            // We will spawn the secondary window (Overlay) inside the app loop
            Ok(Box::new(ui::MainApp::new(state)))
        }),
    )
}
```

- [ ] **Step 2: Commit Main wiring**
```bash
git add src/main.rs
git commit -m "chore: prepare main for multi-window"
```

---

### Task 3: Settings & Overlay UI

**Files:**
- Modify: `src/ui.rs`

- [ ] **Step 1: Split UI into MainApp (Settings) and Overlay Window**
```rust
pub struct MainApp {
    state: Arc<AppState>,
}

impl MainApp {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

impl eframe::App for MainApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. Show Settings UI
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Overlay Settings");
            ui.separator();

            let mut config = (*self.state.config.load()).clone();
            let mut changed = false;

            ui.add(egui::Slider::new(&mut config.transparency, 0..=255).text("Transparency"));
            ui.add(egui::Slider::new(&mut config.ui_scale, 0.5..=2.0).text("UI Scale"));
            ui.checkbox(&mut config.show_bots, "Show Bots");

            if ui.button("Apply Changes").clicked() {
                self.state.config.store(Arc::new(config));
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                if ui.button("Quit App").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });

        // 2. Show Overlay Viewport (The HUD)
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("overlay"),
            egui::ViewportBuilder::default()
                .with_title("RL Overlay HUD")
                .with_transparent(true)
                .with_always_on_top()
                .with_decorations(false)
                .with_mouse_passthrough(true)
                .with_inner_size(self.state.config.load().window_size),
            |ctx, class| {
                assert!(class == egui::ViewportClass::Immediate);
                
                // Clear color MUST be transparent for overlay
                let visuals = egui::Visuals::dark();
                ctx.set_visuals(visuals);
                
                if self.state.is_visible.load(Ordering::SeqCst) {
                    render_overlay(ctx, &self.state);
                }
            },
        );
    }
}

fn render_overlay(ctx: &egui::Context, state: &Arc<AppState>) {
    // Move the existing overlay rendering logic here
    // ... respect config.transparency and config.ui_scale ...
}
```

- [ ] **Step 2: Commit UI changes**
```bash
git add src/ui.rs
git commit -m "feat: implement settings gui and immediate viewport overlay"
```
