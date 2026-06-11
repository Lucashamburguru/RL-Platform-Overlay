use crate::state::{AppState, Config};
use crate::ui::common::{StatusTone, debug_status_row, setting_row, settings_section, status_text};
use crate::ui::hotkeys::render_hotkey_settings_section;
use eframe::egui;
use std::sync::Arc;

pub(crate) fn render_setup_settings_tab(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &Arc<AppState>,
    config_edit: &mut Config,
    changed: &mut bool,
    is_rl_running: bool,
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
                }
                let auto_detect_btn = ui.button("Auto-detect");
                if auto_detect_btn.clicked() {
                    if let Some(path) = crate::state::detect_rocket_league_path() {
                        config_edit.rocket_league_path = path;
                        *changed = true;
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

        let status = crate::setup::inspect_stats_api_setup(&config_edit.rocket_league_path);
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
            status_text(ui, StatusTone::Success, status.message);
        } else if status.exists {
            status_text(ui, StatusTone::Warning, status.message);
        } else {
            status_text(ui, StatusTone::Error, status.message);
        }

        if is_rl_running {
            status_text(
                ui,
                StatusTone::Warning,
                "Rocket League is running. Restart the game after changing this config.",
            );
        }

        ui.add_space(8.0);
        if ui
            .add_sized([140.0, 24.0], egui::Button::new("Enable Stats API"))
            .clicked()
        {
            match crate::setup::ensure_stats_api_setup(&config_edit.rocket_league_path) {
                Ok(result) => state.system.stats_api_setup_result.store(Arc::new(result)),
                Err(error) => state.system.stats_api_setup_result.store(Arc::new(
                    crate::setup::StatsApiSetupResult {
                        message: error,
                        ..Default::default()
                    },
                )),
            }
        }

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
    render_hotkey_settings_section(ui, ctx, state, config_edit, changed);
}
