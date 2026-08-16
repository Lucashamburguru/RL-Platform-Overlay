use crate::state::{AppState, Config};
use crate::ui::common::{
    StatusTone, debug_status_row, helper_text, setting_row, settings_section, status_text,
};
use crate::ui::hotkeys::render_hotkey_settings_section;
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub(crate) fn render_setup_settings_tab(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &Arc<AppState>,
    config_edit: &mut Config,
    changed: &mut bool,
    is_rl_running: bool,
    rl_process_detection_detail: &str,
) {
    settings_section(ui, "Stats API Setup", |ui| {
        setting_row(ui, "Rocket League Folder", |ui| {
            ui.horizontal(|ui| {
                let input_width = (ui.available_width() - 96.0).max(160.0);
                if ui
                    .add_sized(
                        [input_width, 22.0],
                        egui::TextEdit::singleline(&mut config_edit.rocket_league_path),
                    )
                    .changed()
                {
                    *changed = true;
                    request_stats_api_setup_refresh(
                        state,
                        config_edit.rocket_league_path.clone(),
                        false,
                    );
                }
                let auto_detect_btn = ui.button("Auto-detect");
                if auto_detect_btn.clicked() {
                    if let Some(path) = crate::state::detect_rocket_league_path() {
                        config_edit.rocket_league_path = path;
                        *changed = true;
                        request_stats_api_setup_refresh(
                            state,
                            config_edit.rocket_league_path.clone(),
                            true,
                        );
                        ui.data_mut(|d| {
                            d.insert_temp(ui.make_persistent_id("rl_path_autodetect_failed"), false)
                        });
                    } else {
                        ui.data_mut(|d| {
                            d.insert_temp(ui.make_persistent_id("rl_path_autodetect_failed"), true)
                        });
                    }
                }
            });
        });

        if ui.data(|d| {
            d.get_temp::<bool>(ui.make_persistent_id("rl_path_autodetect_failed"))
                .unwrap_or(false)
        }) {
            status_text(
                ui,
                StatusTone::Error,
                "❌ Auto-detection failed. Could not find common installation folders. Please select your Rocket League folder manually.",
            );
        }

        let expected_ini_path = crate::setup::stats_ini_path(&config_edit.rocket_league_path)
            .display()
            .to_string();
        let status = state.system.stats_api_setup_status.load();
        if status.message.is_empty() || status.ini_path != expected_ini_path {
            request_stats_api_setup_refresh(state, config_edit.rocket_league_path.clone(), false);
        }
        ui.add_space(8.0);
        debug_status_row(ui, "Config File", &status.ini_path);
        debug_status_row(
            ui,
            "PacketSendRate",
            &status
                .packet_send_rate
                .map(|rate| rate.to_string())
                .unwrap_or_else(|| "missing".to_string()),
        );
        debug_status_row(
            ui,
            "Port",
            &status
                .port
                .map(|port| port.to_string())
                .unwrap_or_else(|| "49123 default".to_string()),
        );

        if status.configured {
            status_text(ui, StatusTone::Success, &status.message);
        } else if status.exists {
            status_text(ui, StatusTone::Warning, &status.message);
        } else {
            status_text(ui, StatusTone::Error, &status.message);
        }

        if is_rl_running {
            status_text(
                ui,
                StatusTone::Warning,
                "Rocket League is running. Restart the game after changing this config.",
            );
        }

        ui.add_space(8.0);
        ui.label(helper_text(
            "Choose the periodic UpdateState rate. Lower rates reduce overhead; 30 Hz is best for the smoothest teammate boost.",
        ));
        ui.horizontal_wrapped(|ui| {
            for rate in crate::setup::PACKET_SEND_RATE_OPTIONS {
                let label = match rate {
                    0 => "Turn Off",
                    5 => "5 Hz Minimal",
                    15 => "15 Hz Responsive",
                    30 => "30 Hz Smooth",
                    _ => "Custom",
                };
                let selected = status.packet_send_rate == Some(rate);
                if ui
                    .selectable_label(selected, label)
                    .on_hover_text("Writes this PacketSendRate to DefaultStatsAPI.ini.")
                    .clicked()
                {
                    let rocket_league_path = config_edit.rocket_league_path.clone();
                    apply_stats_api_setup_rate(
                        state,
                        &rocket_league_path,
                        rate,
                        &mut config_edit.stats_api_packet_send_rate,
                        changed,
                    );
                }
            }
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Enter own (Hz):");
            let mut custom_rate_str = ui.data_mut(|d| {
                d.get_temp::<String>(ui.make_persistent_id("custom_rate_input"))
                    .unwrap_or_else(|| {
                        if let Some(r) = status.packet_send_rate
                            && r != 0
                            && r != 5
                            && r != 15
                            && r != 30
                        {
                            return r.to_string();
                        }
                        "60".to_string()
                    })
            });

            let response =
                ui.add(egui::TextEdit::singleline(&mut custom_rate_str).desired_width(40.0));
            if response.changed() {
                ui.data_mut(|d| {
                    d.insert_temp(
                        ui.make_persistent_id("custom_rate_input"),
                        custom_rate_str.clone(),
                    )
                });
            }

            let is_custom_active = status
                .packet_send_rate
                .is_some_and(|r| r != 0 && r != 5 && r != 15 && r != 30);

            if ui
                .button("Apply")
                .on_hover_text("Writes your custom PacketSendRate to DefaultStatsAPI.ini.")
                .clicked()
            {
                if let Ok(rate) = custom_rate_str.trim().parse::<u16>() {
                    let rocket_league_path = config_edit.rocket_league_path.clone();
                    apply_stats_api_setup_rate(
                        state,
                        &rocket_league_path,
                        rate,
                        &mut config_edit.stats_api_packet_send_rate,
                        changed,
                    );
                } else {
                    state.system.stats_api_setup_result.store(Arc::new(
                        crate::setup::StatsApiSetupResult {
                            message: "Please enter a valid number between 0 and 120.".to_string(),
                            ..Default::default()
                        },
                    ));
                }
            }

            if is_custom_active {
                ui.label(egui::RichText::new("Active").color(egui::Color32::from_rgb(34, 197, 94)));
            }
        });

        let result = state.system.stats_api_setup_result.load();
        if !result.message.is_empty() {
            ui.add_space(6.0);
            let tone = if result.changed {
                StatusTone::Success
            } else {
                StatusTone::Neutral
            };
            status_text(ui, tone, &result.message);
            if let Some(path) = &result.backup_path {
                debug_status_row(ui, "Backup", path);
            }
            if result.restart_required {
                status_text(
                    ui,
                    StatusTone::Warning,
                    "Restart Rocket League once before expecting the overlay to connect.",
                );
            }
        }
    });

    ui.add_space(10.0);
    render_support_diagnostics_section(
        ui,
        state,
        config_edit,
        changed,
        is_rl_running,
        rl_process_detection_detail,
    );

    ui.add_space(10.0);
    render_hotkey_settings_section(ui, ctx, state, config_edit, changed);
}

fn apply_stats_api_setup_rate(
    state: &Arc<AppState>,
    rocket_league_path: &str,
    rate: u16,
    saved_rate: &mut u16,
    changed: &mut bool,
) {
    match crate::setup::ensure_stats_api_setup_with_rate(rocket_league_path, rate) {
        Ok(result) => {
            if *saved_rate != rate {
                *saved_rate = rate;
                *changed = true;
            }
            state.system.stats_api_setup_result.store(Arc::new(result));
            request_stats_api_setup_refresh(state, rocket_league_path.to_string(), true);
        }
        Err(error) => {
            state
                .system
                .stats_api_setup_result
                .store(Arc::new(crate::setup::StatsApiSetupResult {
                    message: error,
                    ..Default::default()
                }))
        }
    }
}

pub(crate) fn request_stats_api_setup_refresh(
    state: &Arc<AppState>,
    rocket_league_path: String,
    force: bool,
) -> bool {
    if !force
        && state
            .system
            .stats_api_setup_refresh_running
            .load(Ordering::SeqCst)
    {
        return false;
    }
    if state
        .system
        .stats_api_setup_refresh_running
        .swap(true, Ordering::SeqCst)
    {
        return false;
    }

    let current = state.system.stats_api_setup_status.load();
    state
        .system
        .stats_api_setup_status
        .store(Arc::new(crate::setup::StatsApiSetupStatus {
            message: if current.message.is_empty() {
                "Checking Stats API config...".to_string()
            } else {
                "Refreshing Stats API config...".to_string()
            },
            ..(**current).clone()
        }));

    let state_clone = state.clone();
    let run = move || {
        let status = crate::setup::inspect_stats_api_setup(&rocket_league_path);
        state_clone
            .system
            .stats_api_setup_status
            .store(Arc::new(status));
        state_clone
            .system
            .stats_api_setup_refresh_running
            .store(false, Ordering::SeqCst);
    };
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn_blocking(run);
    } else {
        std::thread::spawn(run);
    }
    true
}

fn render_support_diagnostics_section(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config_edit: &mut Config,
    changed: &mut bool,
    is_rl_running: bool,
    rl_process_detection_detail: &str,
) {
    settings_section(ui, "Support Diagnostics", |ui| {
        ui.horizontal_wrapped(|ui| {
            if ui
                .checkbox(&mut config_edit.debug_logging_enabled, "Enable debug logging")
                .on_hover_text("Records extra hotkey and overlay state events to the local diagnostics log.")
                .changed()
            {
                *changed = true;
                state
                    .debug_logging_enabled
                    .store(config_edit.debug_logging_enabled, Ordering::SeqCst);
                crate::input::append_hotkey_debug_log(
                    config_edit.debug_logging_enabled,
                    format!(
                        "debug_logging_enabled value={}",
                        config_edit.debug_logging_enabled
                    ),
                );
            }

            ui.add_space(8.0);
            if ui
                .add_sized([178.0, 24.0], egui::Button::new("Copy Diagnostics Bundle"))
                .on_hover_text("Copies connection, config, session, player, and recent debug log details. API keys are not included.")
                .clicked()
            {
                let bundle = crate::diagnostics::support_diagnostics_bundle(
                    state,
                    state.flags.is_launched.load(Ordering::SeqCst),
                    is_rl_running,
                    rl_process_detection_detail,
                );
                ui.ctx().copy_text(bundle);
                ui.data_mut(|d| {
                    d.insert_temp(ui.make_persistent_id("support_diagnostics_copied"), true)
                });
                crate::input::append_hotkey_debug_log(
                    state.debug_logging_enabled.load(Ordering::SeqCst),
                    "support_diagnostics_copied",
                );
            }
        });

        ui.add_space(4.0);
        ui.label(helper_text(
            "Use this when reporting connection, hotkey, player parsing, or session detection issues.",
        ));
        debug_status_row(
            ui,
            "Hotkey Log",
            &crate::input::hotkey_debug_log_path().display().to_string(),
        );

        if ui.data(|d| {
            d.get_temp::<bool>(ui.make_persistent_id("support_diagnostics_copied"))
                .unwrap_or(false)
        }) {
            status_text(ui, StatusTone::Success, "Diagnostics copied to clipboard.");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_refresh_request_dedupes_while_running() {
        let state = AppState::new();
        state
            .system
            .stats_api_setup_refresh_running
            .store(true, Ordering::SeqCst);

        assert!(!request_stats_api_setup_refresh(
            &state,
            "/tmp/rocket-league".to_string(),
            false
        ));
    }
}
