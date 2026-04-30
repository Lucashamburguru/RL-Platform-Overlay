use crate::state::AppState;
use gilrs::{Event, Gilrs};
use rdev::{EventType, listen};
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub fn start_input_tasks(state: Arc<AppState>) {
    let state_ctrl = state.clone();
    std::thread::spawn(move || match Gilrs::new() {
        Ok(mut gilrs) => {
            println!("Gamepad listener started.");
            loop {
                while let Some(Event { event, .. }) = gilrs.next_event() {
                    match event {
                        gilrs::EventType::ButtonPressed(button, _) => {
                            if state_ctrl.is_recording_ctrl.load(Ordering::SeqCst) {
                                let mut new_config = (**state_ctrl.config.load()).clone();
                                new_config.hotkey_ctrl = format!("{:?}", button);
                                new_config.save();
                                state_ctrl.config.store(Arc::new(new_config));
                                state_ctrl.is_recording_ctrl.store(false, Ordering::SeqCst);
                                println!("Controller hotkey updated to: {:?}", button);
                            } else {
                                let config = state_ctrl.config.load();
                                if format!("{:?}", button) == config.hotkey_ctrl {
                                    state_ctrl.is_visible.store(true, Ordering::SeqCst);
                                }
                            }
                        }
                        gilrs::EventType::ButtonReleased(button, _) => {
                            let config = state_ctrl.config.load();
                            if format!("{:?}", button) == config.hotkey_ctrl {
                                state_ctrl.is_visible.store(false, Ordering::SeqCst);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        _ => {
            eprintln!("Failed to initialize Gamepad listener.");
        }
    });

    let state_kb = state.clone();
    std::thread::spawn(move || {
        println!("Keyboard listener started.");
        if let Err(e) = listen(move |event| match event.event_type {
            EventType::KeyPress(key) => {
                if state_kb.is_recording_kb.load(Ordering::SeqCst) {
                    let mut new_config = (**state_kb.config.load()).clone();
                    new_config.hotkey_kb = format!("{:?}", key);
                    new_config.save();
                    state_kb.config.store(Arc::new(new_config));
                    state_kb.is_recording_kb.store(false, Ordering::SeqCst);
                    println!("Keyboard hotkey updated to: {:?}", key);
                } else {
                    let config = state_kb.config.load();
                    if format!("{:?}", key) == config.hotkey_kb {
                        state_kb.is_visible.store(true, Ordering::SeqCst);
                    }
                }
            }
            EventType::KeyRelease(key) => {
                let config = state_kb.config.load();
                if format!("{:?}", key) == config.hotkey_kb {
                    state_kb.is_visible.store(false, Ordering::SeqCst);
                }
            }
            _ => {}
        }) {
            eprintln!("Failed to initialize Keyboard listener: {:?}", e);
        }
    });
}
