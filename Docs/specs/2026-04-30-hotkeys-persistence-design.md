# Design Spec: Hotkeys & Persistence

Enabling users to customize their toggle keys/buttons and ensuring settings are saved between sessions.

## 1. Goal
Provide a way to change the default "Backspace" and "Select" triggers to user-defined ones and persist all settings to a `config.toml` file.

## 2. Configurable Hotkeys
- **Keyboard:** Any standard key detected by `rdev`.
- **Controller:** Any standard button detected by `gilrs`.
- **UI:** "Record" buttons that wait for the next input event.

## 3. Persistence
- **Storage:** `config.toml`.
- **Serialization:** Use `serde` and `toml` crates.
- **Workflow:** 
  - Load on startup.
  - Save whenever settings are modified.

## 4. Implementation Plan
1. Update `Config` to include serialized versions of keys/buttons.
2. Implement `load()` and `save()` logic in `src/state.rs`.
3. Update `src/input.rs` to dynamically check against the configured hotkeys.
4. Add the "Recording" UI state to `src/ui.rs`.
