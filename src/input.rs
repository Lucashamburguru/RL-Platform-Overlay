use crate::state::{AppState, config_dir};
use gilrs::{Event, Gilrs};
use rdev::{EventType, listen};
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

const SETTINGS_TOGGLE_DEBOUNCE_MS: u128 = 200;
const LAUNCH_TOGGLE_DEBOUNCE_MS: u128 = 200;

pub fn hotkey_debug_log_path() -> PathBuf {
    config_dir()
        .map(|dir| dir.join("hotkey_debug.log"))
        .unwrap_or_else(|| PathBuf::from("hotkey_debug.log"))
}

pub fn append_hotkey_debug_log(debug_enabled: bool, message: impl AsRef<str>) {
    if !debug_enabled {
        return;
    }
    let path = hotkey_debug_log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let now_ms = crate::stats_api::now_ms();
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{now_ms} {}", message.as_ref());
    }
}

fn hotkey_debug_logging_enabled(state: &AppState) -> bool {
    state.debug_logging_enabled.load(Ordering::SeqCst)
}

pub fn toggle_settings_hotkey(state: &Arc<AppState>, source: &str) {
    let event_ms = crate::stats_api::now_ms();
    let last = state
        .hotkeys
        .last_settings_hotkey_unix_ms
        .load(Ordering::SeqCst) as u128;
    let elapsed = event_ms.saturating_sub(last);
    if elapsed < SETTINGS_TOGGLE_DEBOUNCE_MS {
        append_hotkey_debug_log(
            state.debug_logging_enabled.load(Ordering::SeqCst),
            format!("settings_toggle_ignored_duplicate source={source} elapsed_ms={elapsed}"),
        );
        return;
    }

    state
        .hotkeys
        .last_settings_hotkey_unix_ms
        .store(event_ms as u64, Ordering::SeqCst);
    let current = state.flags.is_settings_visible.load(Ordering::SeqCst);
    state
        .flags
        .is_settings_visible
        .store(!current, Ordering::SeqCst);
    append_hotkey_debug_log(
        state.debug_logging_enabled.load(Ordering::SeqCst),
        format!(
            "settings_toggle source={source} current={current} new={}",
            !current
        ),
    );
    log::info!("Settings menu visibility toggled to: {}", !current);
}

pub fn toggle_launch_hotkey(state: &Arc<AppState>, source: &str) {
    let event_ms = crate::stats_api::now_ms();
    let last = state
        .hotkeys
        .last_launch_hotkey_unix_ms
        .load(Ordering::SeqCst) as u128;
    let elapsed = event_ms.saturating_sub(last);
    if elapsed < LAUNCH_TOGGLE_DEBOUNCE_MS {
        append_hotkey_debug_log(
            state.debug_logging_enabled.load(Ordering::SeqCst),
            format!("launch_toggle_ignored_duplicate source={source} elapsed_ms={elapsed}"),
        );
        return;
    }

    state
        .hotkeys
        .last_launch_hotkey_unix_ms
        .store(event_ms as u64, Ordering::SeqCst);
    let current = state.flags.is_launched.load(Ordering::SeqCst);
    let new = !current;
    state.flags.is_launched.store(new, Ordering::SeqCst);
    if new {
        let config = state.system.config.load();
        if config.dashboard_open_with_overlay && !config.dashboard_enabled {
            let mut config_edit = (**config).clone();
            config_edit.dashboard_enabled = true;
            state.save_config(config_edit);
        }
        state
            .flags
            .is_settings_visible
            .store(false, Ordering::SeqCst);
    }
    append_hotkey_debug_log(
        state.debug_logging_enabled.load(Ordering::SeqCst),
        format!("launch_toggle source={source} current={current} new={new}"),
    );
    log::info!("Overlay launched toggled to: {new}");
}

pub fn start_input_tasks(state: Arc<AppState>) {
    start_controller_listener(state.clone());
    start_keyboard_listener(state);
}

fn start_controller_listener(state_ctrl: Arc<AppState>) {
    std::thread::spawn(move || match Gilrs::new() {
        Ok(mut gilrs) => {
            log::info!("Gamepad listener started.");
            let mut pressed_controller_hotkeys = HashSet::new();

            loop {
                // Poll for new events and update gilrs state
                while let Some(Event { id, event, .. }) = gilrs.next_event() {
                    match event {
                        gilrs::EventType::Connected => {
                            let pad = gilrs.gamepad(id);
                            log::info!("Controller Connected: {} (ID: {:?})", pad.name(), id);
                        }
                        gilrs::EventType::Disconnected => {
                            log::info!("Controller Disconnected (ID: {:?})", id);
                        }
                        gilrs::EventType::ButtonPressed(button, _) => {
                            let button_str = format!("{:?}", button);

                            if state_ctrl.hotkeys.is_recording_ctrl.load(Ordering::SeqCst) {
                                record_controller_hotkey(&state_ctrl, id, &button_str);
                            } else {
                                handle_controller_hotkey(
                                    &state_ctrl,
                                    &mut pressed_controller_hotkeys,
                                    id,
                                    button_str,
                                    true,
                                );
                            }
                        }
                        gilrs::EventType::ButtonReleased(button, _) => {
                            handle_controller_hotkey(
                                &state_ctrl,
                                &mut pressed_controller_hotkeys,
                                id,
                                format!("{:?}", button),
                                false,
                            );
                        }
                        gilrs::EventType::ButtonChanged(button, value, _) => {
                            let button_str = format!("{:?}", button);

                            if state_ctrl.hotkeys.is_recording_ctrl.load(Ordering::SeqCst)
                                && value >= 0.5
                            {
                                record_controller_hotkey(&state_ctrl, id, &button_str);
                            } else {
                                handle_controller_hotkey(
                                    &state_ctrl,
                                    &mut pressed_controller_hotkeys,
                                    id,
                                    button_str,
                                    value >= 0.5,
                                );
                            }
                        }
                        _ => {}
                    }
                }

                // On Windows, some backends may require explicit polling or status updates
                // though next_event usually handles it. We sleep a bit less to stay responsive.
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        Err(error) => {
            log::error!("Failed to initialize Gamepad listener: {error}");
        }
    });
}

fn start_keyboard_listener(state_kb: Arc<AppState>) {
    std::thread::spawn(move || {
        log::info!("Keyboard listener thread started.");
        append_hotkey_debug_log(
            hotkey_debug_logging_enabled(&state_kb),
            "keyboard_listener_started",
        );
        let mut pressed_keyboard_hotkeys = HashSet::new();
        let callback = move |event: rdev::Event| match event.event_type {
            EventType::KeyPress(key) => {
                let key_debug = format!("{:?}", key);
                if state_kb.hotkeys.is_recording_kb.load(Ordering::SeqCst) {
                    let mut new_config = (**state_kb.system.config.load()).clone();
                    new_config.hotkey_kb = key_debug.clone();
                    state_kb.save_config(new_config);
                    state_kb
                        .hotkeys
                        .is_recording_kb
                        .store(false, Ordering::SeqCst);
                    log::info!("Keyboard hotkey updated to: {:?}", key);
                    append_hotkey_debug_log(
                        hotkey_debug_logging_enabled(&state_kb),
                        format!("record_keyboard_hotkey key={key_debug}"),
                    );
                } else if state_kb
                    .hotkeys
                    .is_recording_settings
                    .load(Ordering::SeqCst)
                {
                    let mut new_config = (**state_kb.system.config.load()).clone();
                    new_config.hotkey_settings = key_debug.clone();
                    state_kb.save_config(new_config);
                    state_kb
                        .hotkeys
                        .is_recording_settings
                        .store(false, Ordering::SeqCst);
                    log::info!("Settings hotkey updated to: {:?}", key);
                    append_hotkey_debug_log(
                        hotkey_debug_logging_enabled(&state_kb),
                        format!("record_settings_hotkey key={key_debug}"),
                    );
                } else if state_kb.hotkeys.is_recording_launch.load(Ordering::SeqCst) {
                    let mut new_config = (**state_kb.system.config.load()).clone();
                    new_config.hotkey_launch = key_debug.clone();
                    state_kb.save_config(new_config);
                    state_kb
                        .hotkeys
                        .is_recording_launch
                        .store(false, Ordering::SeqCst);
                    log::info!("Launch hotkey updated to: {:?}", key);
                    append_hotkey_debug_log(
                        hotkey_debug_logging_enabled(&state_kb),
                        format!("record_launch_hotkey key={key_debug}"),
                    );
                } else {
                    let config = state_kb.system.config.load();
                    let key_str = key_debug;
                    let first_press = pressed_keyboard_hotkeys.insert(key_str.clone());
                    let is_match = keyboard_hotkey_matches(&key_str, &config.hotkey_kb);
                    let settings_before = state_kb.flags.is_settings_visible.load(Ordering::SeqCst);
                    let hud_before = state_kb.flags.is_visible.load(Ordering::SeqCst);
                    append_hotkey_debug_log(
                        hotkey_debug_logging_enabled(&state_kb),
                        format!(
                            "keypress key={key_str} first_press={first_press} hud_match={is_match} settings_match={} settings_before={settings_before} hud_before={hud_before}",
                            key_str == config.hotkey_settings
                        ),
                    );

                    if first_press && is_match {
                        if config.hotkey_toggle {
                            let current = state_kb.flags.is_visible.load(Ordering::SeqCst);
                            state_kb.flags.is_visible.store(!current, Ordering::SeqCst);
                            append_hotkey_debug_log(
                                hotkey_debug_logging_enabled(&state_kb),
                                format!("hud_toggle current={current} new={}", !current),
                            );
                        } else {
                            state_kb.flags.is_visible.store(true, Ordering::SeqCst);
                            append_hotkey_debug_log(
                                hotkey_debug_logging_enabled(&state_kb),
                                "hud_hold_visible true",
                            );
                        }
                    }

                    // Handle Settings Toggle Hotkey
                    if first_press && key_str == config.hotkey_settings {
                        toggle_settings_hotkey(&state_kb, "rdev");
                    }

                    if first_press && key_str == config.hotkey_launch {
                        toggle_launch_hotkey(&state_kb, "rdev");
                    }
                }
            }
            EventType::KeyRelease(key) => {
                let config = state_kb.system.config.load();
                let key_str = format!("{:?}", key);
                let was_pressed = pressed_keyboard_hotkeys.remove(&key_str);
                let is_match = keyboard_hotkey_matches(&key_str, &config.hotkey_kb);
                append_hotkey_debug_log(
                    hotkey_debug_logging_enabled(&state_kb),
                    format!(
                        "keyrelease key={key_str} was_pressed={was_pressed} hud_match={is_match} settings_match={}",
                        key_str == config.hotkey_settings
                    ),
                );

                if !config.hotkey_toggle && is_match {
                    state_kb.flags.is_visible.store(false, Ordering::SeqCst);
                    append_hotkey_debug_log(
                        hotkey_debug_logging_enabled(&state_kb),
                        "hud_hold_visible false",
                    );
                }
            }
            _ => {}
        };

        if let Err(e) = listen(callback) {
            log::error!("Failed to initialize Keyboard listener: {:?}", e);
        }
    });
}

fn record_controller_hotkey(state: &Arc<AppState>, id: gilrs::GamepadId, button_str: &str) {
    log::info!(
        "Hotkey Record detected: {} on Controller {:?}",
        button_str,
        id
    );
    let mut new_config = (**state.system.config.load()).clone();
    new_config.hotkey_ctrl = button_str.to_string();
    state.save_config(new_config);
    state
        .hotkeys
        .is_recording_ctrl
        .store(false, Ordering::SeqCst);
    log::info!("Controller hotkey updated: {}", button_str);
}

fn keyboard_hotkey_matches(key_str: &str, configured_hotkey: &str) -> bool {
    if key_str == configured_hotkey {
        true
    } else if key_str.starts_with("Kp") && configured_hotkey.starts_with("Num") {
        key_str.len() == 3 && configured_hotkey.len() == 4 && key_str[2..] == configured_hotkey[3..]
    } else {
        false
    }
}

fn handle_controller_hotkey(
    state: &Arc<AppState>,
    pressed_controller_hotkeys: &mut HashSet<(gilrs::GamepadId, String)>,
    id: gilrs::GamepadId,
    button_str: String,
    pressed: bool,
) {
    let config = state.system.config.load();
    if button_str != config.hotkey_ctrl {
        return;
    }

    let key = (id, button_str);
    if pressed {
        if !pressed_controller_hotkeys.insert(key) {
            return;
        }

        if config.hotkey_toggle {
            let current = state.flags.is_visible.load(Ordering::SeqCst);
            state.flags.is_visible.store(!current, Ordering::SeqCst);
        } else {
            state.flags.is_visible.store(true, Ordering::SeqCst);
        }
    } else if pressed_controller_hotkeys.remove(&key) && !config.hotkey_toggle {
        state.flags.is_visible.store(false, Ordering::SeqCst);
    }
}
