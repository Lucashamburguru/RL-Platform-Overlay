use crate::state::AppState;
use gilrs::{Event, Gilrs};
use rdev::{EventType, listen};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;

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
                                new_config.save();
                                state_ctrl.config.store(Arc::new(new_config));
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
                                new_config.save();
                                state_ctrl.config.store(Arc::new(new_config));
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
        let callback = move |event: rdev::Event| match event.event_type {
            EventType::KeyPress(key) => {
                if state_kb.is_recording_kb.load(Ordering::SeqCst) {
                    let mut new_config = (**state_kb.config.load()).clone();
                    new_config.hotkey_kb = format!("{:?}", key);
                    new_config.save();
                    state_kb.config.store(Arc::new(new_config));
                    state_kb.is_recording_kb.store(false, Ordering::SeqCst);
                    println!("Keyboard hotkey updated to: {:?}", key);
                } else if state_kb.is_recording_settings.load(Ordering::SeqCst) {
                    let mut new_config = (**state_kb.config.load()).clone();
                    new_config.hotkey_settings = format!("{:?}", key);
                    new_config.save();
                    state_kb.config.store(Arc::new(new_config));
                    state_kb
                        .is_recording_settings
                        .store(false, Ordering::SeqCst);
                    println!("Settings hotkey updated to: {:?}", key);
                } else {
                    let config = state_kb.config.load();
                    let key_str = format!("{:?}", key);
                    let is_match = if key_str == config.hotkey_kb {
                        true
                    } else if key_str.starts_with("Kp") && config.hotkey_kb.starts_with("Num") {
                        // Alias Kp0..9 to Num0..9
                        key_str.len() == 3
                            && config.hotkey_kb.len() == 4
                            && &key_str[2..] == &config.hotkey_kb[3..]
                    } else {
                        false
                    };

                    if is_match {
                        if config.hotkey_toggle {
                            let current = state_kb.is_visible.load(Ordering::SeqCst);
                            state_kb.is_visible.store(!current, Ordering::SeqCst);
                        } else {
                            state_kb.is_visible.store(true, Ordering::SeqCst);
                        }
                    }

                    // Handle Settings Toggle Hotkey
                    if key_str == config.hotkey_settings {
                        let current = state_kb.is_settings_visible.load(Ordering::SeqCst);
                        state_kb
                            .is_settings_visible
                            .store(!current, Ordering::SeqCst);
                        println!("Settings menu visibility toggled to: {}", !current);
                    }
                }
            }
            EventType::KeyRelease(key) => {
                let config = state_kb.config.load();
                let key_str = format!("{:?}", key);
                let is_match = if key_str == config.hotkey_kb {
                    true
                } else if key_str.starts_with("Kp") && config.hotkey_kb.starts_with("Num") {
                    key_str.len() == 3
                        && config.hotkey_kb.len() == 4
                        && &key_str[2..] == &config.hotkey_kb[3..]
                } else {
                    false
                };

                if !config.hotkey_toggle && is_match {
                    state_kb.is_visible.store(false, Ordering::SeqCst);
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
