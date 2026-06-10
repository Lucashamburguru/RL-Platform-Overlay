# Hotkeys & Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement custom hotkey recording and save/load configuration to/from `config.toml`.

**Architecture:** Use `serde` for serialization. Update `input.rs` to use shared state for hotkey comparison. Add a "recording" state to the UI.

**Tech Stack:** Rust, serde, toml, rdev, gilrs.

---

### Task 1: Update State & Persistence Logic

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/state.rs`

- [ ] **Step 1: Add toml dependency**
```toml
[dependencies]
toml = "0.8"
```

- [ ] **Step 2: Update Config for serialization and add hotkey fields**
```rust
use rdev::Key;
use gilrs::Button;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub transparency: u8,
    pub ui_scale: f32,
    pub show_bots: bool,
    pub window_size: [f32; 2],
    pub anchor: AnchorPos,
    pub monitor_index: usize,
    pub hotkey_kb: String, // Stringified rdev::Key
    pub hotkey_ctrl: String, // Stringified gilrs::Button
}

impl Config {
    pub fn load() -> Self {
        std::fs::read_to_string("config.toml")
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }
    
    pub fn save(&self) {
        if let Ok(s) = toml::to_string_pretty(self) {
            let _ = std::fs::write("config.toml", s);
        }
    }
}
```

- [ ] **Step 3: Commit state changes**
```bash
git add Cargo.toml src/state.rs
git commit -m "feat: add hotkey fields and toml persistence to Config"
```

---

### Task 2: Dynamic Input Listeners

**Files:**
- Modify: `src/input.rs`

- [ ] **Step 1: Update listeners to check against Config**
Modify the matching logic to parse the `hotkey_kb` and `hotkey_ctrl` strings back into enums for comparison.

- [ ] **Step 2: Add recording flags to AppState**
```rust
pub struct AppState {
    pub is_recording_kb: AtomicBool,
    pub is_recording_ctrl: AtomicBool,
    // ... existing ...
}
```

- [ ] **Step 3: Commit input changes**
```bash
git add src/input.rs src/state.rs
git commit -m "feat: update input listeners to use configurable hotkeys"
```

---

### Task 3: Hotkey Recording UI

**Files:**
- Modify: `src/ui.rs`

- [ ] **Step 1: Add "Record" buttons to Settings window**
- [ ] **Step 2: Implement "Listening..." state that captures the next input**
- [ ] **Step 3: Commit UI changes**
```bash
git add src/ui.rs
git commit -m "feat: add hotkey recording UI"
```
