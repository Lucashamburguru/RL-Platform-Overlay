use crate::state::AppState;
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub(super) fn render_hotkey_settings_section(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &Arc<AppState>,
    config_edit: &mut crate::state::Config,
    changed: &mut bool,
) {
    ui.group(|ui| {
        ui.heading("Hotkeys");
        render_keyboard_hotkey_row(ui, ctx, state, config_edit);
        render_controller_hotkey_row(ui, state, config_edit);
        render_settings_hotkey_row(ui, ctx, state, config_edit);

        if ui
            .checkbox(
                &mut config_edit.hotkey_toggle,
                "Toggle Hotkey (Instead of Hold)",
            )
            .changed()
        {
            *changed = true;
        }
    });
}

fn render_keyboard_hotkey_row(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &Arc<AppState>,
    config_edit: &mut crate::state::Config,
) {
    ui.horizontal(|ui| {
        ui.label("Keyboard:");
        if state.is_recording_kb.load(Ordering::SeqCst) {
            ui.colored_label(egui::Color32::YELLOW, "Listening...");
            if ui.button("Cancel").clicked() {
                state.is_recording_kb.store(false, Ordering::SeqCst);
            }
            if let Some(name) = capture_egui_key(ctx) {
                config_edit.hotkey_kb = name;
                state.save_config(config_edit.clone());
                state.is_recording_kb.store(false, Ordering::SeqCst);
            }
        } else {
            ui.label(format!("[ {} ]", format_key_name(&config_edit.hotkey_kb)));
            if ui.button("Record").clicked() {
                state.is_recording_kb.store(true, Ordering::SeqCst);
                state.is_recording_ctrl.store(false, Ordering::SeqCst);
                state.is_recording_settings.store(false, Ordering::SeqCst);
            }
        }
    });
}

fn render_controller_hotkey_row(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config_edit: &crate::state::Config,
) {
    ui.horizontal(|ui| {
        ui.label("Controller:");
        if state.is_recording_ctrl.load(Ordering::SeqCst) {
            ui.colored_label(egui::Color32::YELLOW, "Listening...");
            if ui.button("Cancel").clicked() {
                state.is_recording_ctrl.store(false, Ordering::SeqCst);
            }
        } else {
            ui.label(format!("[ {} ]", config_edit.hotkey_ctrl));
            if ui.button("Record").clicked() {
                state.is_recording_ctrl.store(true, Ordering::SeqCst);
                state.is_recording_kb.store(false, Ordering::SeqCst);
                state.is_recording_settings.store(false, Ordering::SeqCst);
            }
        }
    });
}

fn render_settings_hotkey_row(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &Arc<AppState>,
    config_edit: &mut crate::state::Config,
) {
    ui.horizontal(|ui| {
        ui.label("Settings Toggle:");
        if state.is_recording_settings.load(Ordering::SeqCst) {
            ui.colored_label(egui::Color32::YELLOW, "Listening...");
            if ui.button("Cancel").clicked() {
                state.is_recording_settings.store(false, Ordering::SeqCst);
            }
            if let Some(name) = capture_egui_key(ctx) {
                config_edit.hotkey_settings = name;
                state.save_config(config_edit.clone());
                state.is_recording_settings.store(false, Ordering::SeqCst);
            }
        } else {
            ui.label(format!(
                "[ {} ]",
                format_key_name(&config_edit.hotkey_settings)
            ));
            if ui.button("Record").clicked() {
                state.is_recording_settings.store(true, Ordering::SeqCst);
                state.is_recording_kb.store(false, Ordering::SeqCst);
                state.is_recording_ctrl.store(false, Ordering::SeqCst);
            }
        }
    });
}

fn capture_egui_key(ctx: &egui::Context) -> Option<String> {
    let mut captured_name = None;
    ctx.input(|i| {
        if i.modifiers.ctrl {
            captured_name = Some("ControlLeft".to_string());
        } else if i.modifiers.shift {
            captured_name = Some("ShiftLeft".to_string());
        } else if i.modifiers.alt {
            captured_name = Some("Alt".to_string());
        } else if i.modifiers.command {
            captured_name = Some("MetaLeft".to_string());
        }

        for event in &i.events {
            if let egui::Event::Key {
                key, pressed: true, ..
            } = event
                && let Some(name) = egui_to_rdev_key(*key)
            {
                captured_name = Some(name);
            }
        }
    });
    captured_name
}

fn format_key_name(key: &str) -> &str {
    match key {
        "Insert" => "Num0 / Insert",
        "End" => "Num1 / End",
        "DownArrow" => "Num2 / Down",
        "PageDown" => "Num3 / PgDn",
        "LeftArrow" => "Num4 / Left",
        "RightArrow" => "Num6 / Right",
        "Home" => "Num7 / Home",
        "UpArrow" => "Num8 / Up",
        "PageUp" => "Num9 / PgUp",
        "Delete" => "Num. / Del",
        s => s,
    }
}

pub(super) fn egui_to_rdev_key(key: egui::Key) -> Option<String> {
    use egui::Key::*;
    let s = match key {
        A => "KeyA",
        B => "KeyB",
        C => "KeyC",
        D => "KeyD",
        E => "KeyE",
        F => "KeyF",
        G => "KeyG",
        H => "KeyH",
        I => "KeyI",
        J => "KeyJ",
        K => "KeyK",
        L => "KeyL",
        M => "KeyM",
        N => "KeyN",
        O => "KeyO",
        P => "KeyP",
        Q => "KeyQ",
        R => "KeyR",
        S => "KeyS",
        T => "KeyT",
        U => "KeyU",
        V => "KeyV",
        W => "KeyW",
        X => "KeyX",
        Y => "KeyY",
        Z => "KeyZ",
        Num0 => "Num0",
        Num1 => "Num1",
        Num2 => "Num2",
        Num3 => "Num3",
        Num4 => "Num4",
        Num5 => "Num5",
        Num6 => "Num6",
        Num7 => "Num7",
        Num8 => "Num8",
        Num9 => "Num9",
        F1 => "F1",
        F2 => "F2",
        F3 => "F3",
        F4 => "F4",
        F5 => "F5",
        F6 => "F6",
        F7 => "F7",
        F8 => "F8",
        F9 => "F9",
        F10 => "F10",
        F11 => "F11",
        F12 => "F12",
        F13 => "F13",
        F14 => "F14",
        F15 => "F15",
        F16 => "F16",
        F17 => "F17",
        F18 => "F18",
        F19 => "F19",
        F20 => "F20",
        ArrowDown => "DownArrow",
        ArrowLeft => "LeftArrow",
        ArrowRight => "RightArrow",
        ArrowUp => "UpArrow",
        Escape => "Escape",
        Tab => "Tab",
        Backspace => "Backspace",
        Enter => "Return",
        Space => "Space",
        Insert => "Insert",
        Delete => "Delete",
        Home => "Home",
        End => "End",
        PageUp => "PageUp",
        PageDown => "PageDown",
        Semicolon | Colon => "Semicolon",
        Comma => "Comma",
        Period => "Dot",
        Slash | Questionmark => "Slash",
        Backslash | Pipe => "Backslash",
        Backtick => "Backquote",
        Minus => "Minus",
        Equals | Plus => "Equal",
        OpenBracket | OpenCurlyBracket => "LeftBracket",
        CloseBracket | CloseCurlyBracket => "RightBracket",
        Quote => "Quote",
        _ => return None,
    };
    Some(s.to_string())
}
