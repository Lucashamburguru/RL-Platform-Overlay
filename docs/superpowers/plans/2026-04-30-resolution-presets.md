# Resolution Presets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a resolution preset dropdown to the settings window.

**Architecture:** Add a list of presets to `Config`. Update `ui.rs` to allow selecting these presets and applying the `window_size`.

**Tech Stack:** Rust, eframe, egui.

---

### Task 1: Implement Resolution Selection

**Files:**
- Modify: `src/ui.rs`

- [ ] **Step 1: Define presets and update UI**
```rust
// Inside MainApp::update
ui.horizontal(|ui| {
    ui.label("Resolution:");
    let current_res = config.window_size;
    let res_text = format!("{}x{}", current_res[0], current_res[1]);
    
    egui::ComboBox::from_id_source("res_presets")
        .selected_text(res_text)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut config.window_size, [1920.0, 1080.0], "1080p");
            ui.selectable_value(&mut config.window_size, [2560.0, 1440.0], "1440p");
            ui.selectable_value(&mut config.window_size, [3840.0, 2160.0], "4K");
            ui.selectable_value(&mut config.window_size, [3440.0, 1440.0], "Ultrawide");
        });
        
    if config.window_size != current_res {
        changed = true;
    }
});
```

- [ ] **Step 2: Ensure Overlay Viewport uses the selected resolution**
Update the `ViewportBuilder` in `ui.rs` to use `config.window_size`.

- [ ] **Step 3: Commit changes**
```bash
git add src/ui.rs
git commit -m "feat: add resolution preset dropdown"
```
