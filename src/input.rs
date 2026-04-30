use crate::state::AppState;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use gilrs::{Gilrs, Event, Button};
use rdev::{listen, EventType, Key};

pub fn start_input_tasks(state: Arc<AppState>) {
    let state_ctrl = state.clone();
    std::thread::spawn(move || {
        let mut gilrs = Gilrs::new().unwrap();
        loop {
            while let Some(Event { event, .. }) = gilrs.next_event() {
                match event {
                    gilrs::EventType::ButtonPressed(Button::Select, _) => {
                        state_ctrl.is_visible.store(true, Ordering::SeqCst);
                    }
                    gilrs::EventType::ButtonReleased(Button::Select, _) => {
                        state_ctrl.is_visible.store(false, Ordering::SeqCst);
                    }
                    _ => {}
                }
            }
        }
    });

    let state_kb = state.clone();
    std::thread::spawn(move || {
        listen(move |event| {
            match event.event_type {
                EventType::KeyPress(Key::Backspace) => {
                    state_kb.is_visible.store(true, Ordering::SeqCst);
                }
                EventType::KeyRelease(Key::Backspace) => {
                    state_kb.is_visible.store(false, Ordering::SeqCst);
                }
                _ => {}
            }
        }).unwrap();
    });
}
