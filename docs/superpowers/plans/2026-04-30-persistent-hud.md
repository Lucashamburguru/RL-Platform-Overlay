# Persistent HUD & Advanced Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor the UI to use a persistent overlay window and add monitor/position controls.

**Architecture:** Add `is_launched` to AppState. Modify `ui.rs` to keep the viewport alive and implement the new positioning logic.

**Tech Stack:** Rust, eframe, egui.

---

### Task 1: Update Config and State

**Files:**
- Modify: `src/state.rs`

- [ ] **Step 1: Add new fields to Config and AppState**
```rust
#[derive(Clone, Debug, PartialEq)]
pub enum AnchorPos {
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
    pub monitor_index: usize,
    pub anchor: AnchorPos,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            transparency: 150,
            ui_scale: 1.0,
            show_bots: true,
            window_size: [1920.0, 1080.0],
            monitor_index: 0,
            anchor: AnchorPos::TopRight,
        }
    }
}

pub struct AppState {
    pub is_visible: AtomicBool,
    pub is_connected: AtomicBool,
    pub is_launched: AtomicBool, // NEW
    pub players: ArcSwap<HashMap<String, PlayerInfo>>,
    pub config: ArcSwap<Config>,
}
```

- [ ] **Step 2: Commit State changes**
```bash
git add src/state.rs
git commit -m "feat: add positioning and launch state to AppState"
```

---

### Task 2: Implement Persistent Viewport & Settings UI

**Files:**
- Modify: `src/ui.rs`

- [ ] **Step 1: Update MainApp for persistence and new controls**
```rust
impl eframe::App for MainApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("RL Overlay Settings");
            
            let mut config = (*self.state.config.load()).clone();
            let mut config_changed = false;

            // 1. Controls
            ui.add(egui::Slider::new(&mut config.transparency, 0..=255).text("Transparency"));
            ui.add(egui::Slider::new(&mut config.ui_scale, 0.5..=3.0).text("UI Scale"));
            
            egui::ComboBox::from_label("Anchor Position")
                .selected_text(format!("{:?}", config.anchor))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut config.anchor, AnchorPos::TopLeft, "Top Left");
                    ui.selectable_value(&mut config.anchor, AnchorPos::TopRight, "Top Right");
                    ui.selectable_value(&mut config.anchor, AnchorPos::BottomLeft, "Bottom Left");
                    ui.selectable_value(&mut config.anchor, AnchorPos::BottomRight, "Bottom Right");
                    ui.selectable_value(&mut config.anchor, AnchorPos::CenterRight, "Center Right");
                });

            if ui.button("Apply Settings").clicked() {
                self.state.config.store(Arc::new(config));
            }

            ui.separator();

            // 2. Launch Button
            let launched = self.state.is_launched.load(Ordering::SeqCst);
            if launched {
                ui.label("Overlay is ACTIVE");
                if ui.button("Stop Overlay").clicked() {
                    self.state.is_launched.store(false, Ordering::SeqCst);
                }
            } else {
                if ui.button("Launch Overlay").clicked() {
                    self.state.is_launched.store(true, Ordering::SeqCst);
                }
            }
        });

        // 3. Persistent Viewport
        if self.state.is_launched.load(Ordering::SeqCst) {
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("overlay"),
                egui::ViewportBuilder::default()
                    .with_transparent(true)
                    .with_always_on_top()
                    .with_decorations(false)
                    .with_mouse_passthrough(true),
                |ctx, _class| {
                    // Apply UI Scale only here
                    ctx.set_pixels_per_point(self.state.config.load().ui_scale);
                    
                    if self.state.is_visible.load(Ordering::SeqCst) {
                        render_overlay(ctx, &self.state);
                    }
                }
            );
        }
    }
}
```

- [ ] **Step 2: Update render_overlay positioning logic**
```rust
fn render_overlay(ctx: &egui::Context, state: &Arc<AppState>) {
    let config = state.config.load();
    let egui_anchor = match config.anchor {
        AnchorPos::TopLeft => egui::Align2::LEFT_TOP,
        AnchorPos::TopRight => egui::Align2::RIGHT_TOP,
        AnchorPos::BottomLeft => egui::Align2::LEFT_BOTTOM,
        AnchorPos::BottomRight => egui::Align2::RIGHT_BOTTOM,
        AnchorPos::CenterRight => egui::Align2::RIGHT_CENTER,
    };
    
    let offset = match config.anchor {
        AnchorPos::TopLeft => egui::vec2(20.0, 20.0),
        AnchorPos::TopRight => egui::vec2(-20.0, 20.0),
        AnchorPos::BottomLeft => egui::vec2(20.0, -20.0),
        AnchorPos::BottomRight => egui::vec2(-20.0, -20.0),
        AnchorPos::CenterRight => egui::vec2(-20.0, 0.0),
    };

    egui::Area::new("overlay_content".into())
        .anchor(egui_anchor, offset)
        .show(ctx, |ui| {
            // ... existing list rendering logic ...
        });
}
```

- [ ] **Step 3: Commit UI changes**
```bash
git add src/ui.rs
git commit -m "feat: persistent overlay window and anchor controls"
```
