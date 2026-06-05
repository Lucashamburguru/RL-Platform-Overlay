use crate::state::{AppState, config_dir};
use gilrs::{Event, Gilrs};
use rdev::{EventType, listen};
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

const SETTINGS_TOGGLE_DEBOUNCE_MS: u128 = 200;

pub fn hotkey_debug_log_path() -> PathBuf {
    config_dir()
        .map(|dir| dir.join("hotkey_debug.log"))
        .unwrap_or_else(|| PathBuf::from("hotkey_debug.log"))
}

pub fn append_hotkey_debug_log(message: impl AsRef<str>) {
    let path = hotkey_debug_log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let now_ms = now_ms();
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{now_ms} {}", message.as_ref());
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

pub fn toggle_settings_hotkey(state: &Arc<AppState>, source: &str) {
    let event_ms = now_ms();
    let last = state.last_settings_hotkey_unix_ms.load(Ordering::SeqCst) as u128;
    let elapsed = event_ms.saturating_sub(last);
    if elapsed < SETTINGS_TOGGLE_DEBOUNCE_MS {
        append_hotkey_debug_log(format!(
            "settings_toggle_ignored_duplicate source={source} elapsed_ms={elapsed}"
        ));
        return;
    }

    state
        .last_settings_hotkey_unix_ms
        .store(event_ms as u64, Ordering::SeqCst);
    let current = state.is_settings_visible.load(Ordering::SeqCst);
    state.is_settings_visible.store(!current, Ordering::SeqCst);
    append_hotkey_debug_log(format!(
        "settings_toggle source={source} current={current} new={}",
        !current
    ));
    println!("Settings menu visibility toggled to: {}", !current);
}

pub fn start_input_tasks(state: Arc<AppState>) {
    let state_ctrl = state.clone();
    std::thread::spawn(move || match Gilrs::new() {
        Ok(mut gilrs) => {
            println!("Gamepad listener started.");
            let mut pressed_controller_hotkeys = HashSet::new();

            loop {
                // Poll for new events and update gilrs state
                while let Some(Event { id, event, .. }) = gilrs.next_event() {
                    match event {
                        gilrs::EventType::Connected => {
                            let pad = gilrs.gamepad(id);
                            println!("Controller Connected: {} (ID: {:?})", pad.name(), id);
                        }
                        gilrs::EventType::Disconnected => {
                            println!("Controller Disconnected (ID: {:?})", id);
                        }
                        gilrs::EventType::ButtonPressed(button, _) => {
                            let button_str = format!("{:?}", button);

                            if state_ctrl.is_recording_ctrl.load(Ordering::SeqCst) {
                                println!(
                                    "Hotkey Record detected: {} on Controller {:?}",
                                    button_str, id
                                );
                                let mut new_config = (**state_ctrl.config.load()).clone();
                                new_config.hotkey_ctrl = button_str.clone();
                                state_ctrl.save_config(new_config);
                                state_ctrl.is_recording_ctrl.store(false, Ordering::SeqCst);
                                println!("Controller hotkey updated: {}", button_str);
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

                            if state_ctrl.is_recording_ctrl.load(Ordering::SeqCst) && value >= 0.5 {
                                println!(
                                    "Hotkey Record detected: {} on Controller {:?}",
                                    button_str, id
                                );
                                let mut new_config = (**state_ctrl.config.load()).clone();
                                new_config.hotkey_ctrl = button_str.clone();
                                state_ctrl.save_config(new_config);
                                state_ctrl.is_recording_ctrl.store(false, Ordering::SeqCst);
                                println!("Controller hotkey updated: {}", button_str);
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
        _ => {
            eprintln!("Failed to initialize Gamepad listener.");
        }
    });

    let state_kb = state.clone();
    std::thread::spawn(move || {
        println!("Keyboard listener thread started.");
        append_hotkey_debug_log("keyboard_listener_started");
        let mut pressed_keyboard_hotkeys = HashSet::new();
        let callback = move |event: rdev::Event| match event.event_type {
            EventType::KeyPress(key) => {
                let key_debug = format!("{:?}", key);
                if state_kb.is_recording_kb.load(Ordering::SeqCst) {
                    let mut new_config = (**state_kb.config.load()).clone();
                    new_config.hotkey_kb = key_debug.clone();
                    state_kb.save_config(new_config);
                    state_kb.is_recording_kb.store(false, Ordering::SeqCst);
                    println!("Keyboard hotkey updated to: {:?}", key);
                    append_hotkey_debug_log(format!("record_keyboard_hotkey key={key_debug}"));
                } else if state_kb.is_recording_settings.load(Ordering::SeqCst) {
                    let mut new_config = (**state_kb.config.load()).clone();
                    new_config.hotkey_settings = key_debug.clone();
                    state_kb.save_config(new_config);
                    state_kb
                        .is_recording_settings
                        .store(false, Ordering::SeqCst);
                    println!("Settings hotkey updated to: {:?}", key);
                    append_hotkey_debug_log(format!("record_settings_hotkey key={key_debug}"));
                } else {
                    let config = state_kb.config.load();
                    let key_str = key_debug;
                    let first_press = pressed_keyboard_hotkeys.insert(key_str.clone());
                    let is_match = if key_str == config.hotkey_kb {
                        true
                    } else if key_str.starts_with("Kp") && config.hotkey_kb.starts_with("Num") {
                        // Alias Kp0..9 to Num0..9
                        key_str.len() == 3
                            && config.hotkey_kb.len() == 4
                            && key_str[2..] == config.hotkey_kb[3..]
                    } else {
                        false
                    };
                    let settings_before = state_kb.is_settings_visible.load(Ordering::SeqCst);
                    let hud_before = state_kb.is_visible.load(Ordering::SeqCst);
                    append_hotkey_debug_log(format!(
                        "keypress key={key_str} first_press={first_press} hud_match={is_match} settings_match={} settings_before={settings_before} hud_before={hud_before}",
                        key_str == config.hotkey_settings
                    ));

                    if first_press && is_match {
                        if config.hotkey_toggle {
                            let current = state_kb.is_visible.load(Ordering::SeqCst);
                            state_kb.is_visible.store(!current, Ordering::SeqCst);
                            append_hotkey_debug_log(format!(
                                "hud_toggle current={current} new={}",
                                !current
                            ));
                        } else {
                            state_kb.is_visible.store(true, Ordering::SeqCst);
                            append_hotkey_debug_log("hud_hold_visible true");
                        }
                    }

                    // Handle Settings Toggle Hotkey
                    if first_press && key_str == config.hotkey_settings {
                        toggle_settings_hotkey(&state_kb, "rdev");
                    }
                }
            }
            EventType::KeyRelease(key) => {
                let config = state_kb.config.load();
                let key_str = format!("{:?}", key);
                let was_pressed = pressed_keyboard_hotkeys.remove(&key_str);
                let is_match = if key_str == config.hotkey_kb {
                    true
                } else if key_str.starts_with("Kp") && config.hotkey_kb.starts_with("Num") {
                    key_str.len() == 3
                        && config.hotkey_kb.len() == 4
                        && key_str[2..] == config.hotkey_kb[3..]
                } else {
                    false
                };
                append_hotkey_debug_log(format!(
                    "keyrelease key={key_str} was_pressed={was_pressed} hud_match={is_match} settings_match={}",
                    key_str == config.hotkey_settings
                ));

                if !config.hotkey_toggle && is_match {
                    state_kb.is_visible.store(false, Ordering::SeqCst);
                    append_hotkey_debug_log("hud_hold_visible false");
                }
            }
            _ => {}
        };

        if let Err(e) = listen(callback) {
            eprintln!("Failed to initialize Keyboard listener: {:?}", e);
        }
    });
}

fn handle_controller_hotkey(
    state: &Arc<AppState>,
    pressed_controller_hotkeys: &mut HashSet<(gilrs::GamepadId, String)>,
    id: gilrs::GamepadId,
    button_str: String,
    pressed: bool,
) {
    let config = state.config.load();
    if button_str != config.hotkey_ctrl {
        return;
    }

    let key = (id, button_str);
    if pressed {
        if !pressed_controller_hotkeys.insert(key) {
            return;
        }

        if config.hotkey_toggle {
            let current = state.is_visible.load(Ordering::SeqCst);
            state.is_visible.store(!current, Ordering::SeqCst);
        } else {
            state.is_visible.store(true, Ordering::SeqCst);
        }
    } else if pressed_controller_hotkeys.remove(&key) && !config.hotkey_toggle {
        state.is_visible.store(false, Ordering::SeqCst);
    }
}
