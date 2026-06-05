use crate::state::{AppState, DebugCaptureStatus};
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::common::debug_status_row;

pub(super) fn render_debug_settings_tab(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    is_launched: bool,
    is_rl_running: bool,
    rl_process_detection_detail: &str,
) {
    ui.group(|ui| {
        ui.heading("Parsed State");
        debug_status_row(
            ui,
            "Overlay",
            if is_launched { "Launched" } else { "Settings" },
        );
        debug_status_row(
            ui,
            "Connection",
            if state.is_connected.load(Ordering::SeqCst) {
                "Connected"
            } else {
                "Disconnected"
            },
        );
        let process_status = if rl_process_detection_detail.trim().is_empty() {
            if is_rl_running {
                "Detected".to_string()
            } else {
                "Not detected".to_string()
            }
        } else if is_rl_running {
            format!("Detected ({rl_process_detection_detail})")
        } else {
            format!("Not detected ({rl_process_detection_detail})")
        };
        debug_status_row(ui, "Rocket League Process", &process_status);
        let local_name = state.local_player_name.load();
        debug_status_row(ui, "Local Player", local_name.as_str());
        let local_team = state.local_team.load(Ordering::SeqCst);
        let team_text = if local_team == 255 {
            "Unknown".to_string()
        } else {
            local_team.to_string()
        };
        debug_status_row(ui, "Local Team", &team_text);

        let players = state.players.load();
        debug_status_row(ui, "Players", &players.len().to_string());
        debug_status_row(
            ui,
            "Hotkey Log",
            &crate::input::hotkey_debug_log_path().display().to_string(),
        );
        if ui.button("Clear Hotkey Log").clicked() {
            let path = crate::input::hotkey_debug_log_path();
            match std::fs::write(&path, "") {
                Ok(()) => crate::input::append_hotkey_debug_log("hotkey_log_cleared"),
                Err(error) => crate::input::append_hotkey_debug_log(format!(
                    "hotkey_log_clear_failed error={error}"
                )),
            }
        }

        let diagnostics = state.network_diagnostics.load();
        debug_status_row(ui, "Transport", diagnostics.transport.label());
        debug_status_row(ui, "Last Event", diagnostics.last_event.as_str());
        debug_status_row(
            ui,
            "Last Event ms",
            &diagnostics.last_event_unix_ms.to_string(),
        );
        if !diagnostics.last_parse_error.is_empty() {
            debug_status_row(
                ui,
                "Last Parse Error",
                diagnostics.last_parse_error.as_str(),
            );
        }
        if !diagnostics.last_connection_error.is_empty() {
            debug_status_row(
                ui,
                "Last Connection Error",
                diagnostics.last_connection_error.as_str(),
            );
        }

        ui.separator();
        let version_check = state.version_check.load();
        debug_status_row(ui, "Current Version", env!("CARGO_PKG_VERSION"));
        let version_status = if !version_check.checked {
            "Checking...".to_string()
        } else if version_check.update_available {
            format!("Update available ({})", version_check.latest_tag)
        } else if !version_check.error.is_empty() {
            version_check.error.clone()
        } else {
            format!("Up to date ({})", version_check.latest_tag)
        };
        debug_status_row(ui, "Version Check", &version_status);

        let config_status = state.config_status.load();
        debug_status_row(ui, "Config Path", &config_status.path);
        debug_status_row(
            ui,
            "Config Status",
            if config_status.last_error.is_empty() {
                "OK"
            } else {
                config_status.last_error.as_str()
            },
        );

        ui.separator();
        for player in players.values() {
            ui.label(format!(
                "{} | team {} | {} | boost {}",
                player.name, player.team, player.platform, player.boost
            ));
        }
    });

    ui.add_space(10.0);
    ui.group(|ui| {
        ui.heading("Stats API Capture");
        let capture = state.debug_capture_status.load();
        if capture.running {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.label("Capturing 30 seconds of Stats API output...");
            });
        } else if ui.button("Capture 30s Stats API Output").clicked() {
            start_debug_capture(state.clone());
        }

        render_capture_status(ui, &capture);
    });
}

fn render_capture_status(ui: &mut egui::Ui, capture: &DebugCaptureStatus) {
    if !capture.message.is_empty() {
        ui.colored_label(egui::Color32::from_rgb(100, 220, 100), &capture.message);
    }
    if !capture.error.is_empty() {
        ui.colored_label(egui::Color32::from_rgb(230, 80, 80), &capture.error);
    }
    if !capture.last_output_path.is_empty() {
        debug_status_row(ui, "Output", &capture.last_output_path);
    }
}

fn start_debug_capture(state: Arc<AppState>) {
    let output = crate::stats_api::default_capture_path(crate::state::config_dir());
    state
        .debug_capture_status
        .store(Arc::new(DebugCaptureStatus {
            running: true,
            last_output_path: output.display().to_string(),
            message: String::new(),
            error: String::new(),
        }));

    tokio::spawn(async move {
        let result = crate::stats_api::capture_to_file(&output, 30).await;
        let status = match result {
            Ok(()) => DebugCaptureStatus {
                running: false,
                last_output_path: output.display().to_string(),
                message: "Capture complete.".to_string(),
                error: String::new(),
            },
            Err(error) => DebugCaptureStatus {
                running: false,
                last_output_path: output.display().to_string(),
                message: String::new(),
                error: format!("Capture failed: {error}"),
            },
        };
        state.debug_capture_status.store(Arc::new(status));
    });
}
