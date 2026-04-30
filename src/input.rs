use crate::state::AppState;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use gilrs::{Gilrs, Event, Button};
use rdev::{listen, EventType, Key};

pub fn start_input_tasks(state: Arc<AppState>) {
    let state_ctrl = state.clone();
    std::thread::spawn(move || {
        if let Ok(mut gilrs) = Gilrs::new() {
            println!("Gamepad listener started.");
            loop {
                while let Some(Event { event, .. }) = gilrs.next_event() {
                    match event {
                        gilrs::EventType::ButtonPressed(Button::Select, _) => {
                            println!("Controller Select pressed");
                            state_ctrl.is_visible.store(true, Ordering::SeqCst);
                        }
                        gilrs::EventType::ButtonReleased(Button::Select, _) => {
                            println!("Controller Select released");
                            state_ctrl.is_visible.store(false, Ordering::SeqCst);
                        }
                        _ => {}
                    }
                }
            }
        } else {
            eprintln!("Failed to initialize Gamepad listener.");
        }
    });

    let state_kb = state.clone();
    std::thread::spawn(move || {
        println!("Keyboard listener started.");
        if let Err(e) = listen(move |event| {
            match event.event_type {
                EventType::KeyPress(Key::Backspace) => {
                    println!("Keyboard Backspace pressed");
                    state_kb.is_visible.store(true, Ordering::SeqCst);
                }
                EventType::KeyRelease(Key::Backspace) => {
                    println!("Keyboard Backspace released");
                    state_kb.is_visible.store(false, Ordering::SeqCst);
                }
                _ => {}
            }
        }) {
            eprintln!("Failed to initialize Keyboard listener: {:?}", e);
        }
    });
}
