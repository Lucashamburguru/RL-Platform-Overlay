use crate::state::{AppState, Config};
use crate::ui::common::{StatusTone, debug_status_row, setting_row, settings_section, status_text};
use eframe::egui;
use std::sync::Arc;

pub(crate) fn render_setup_settings_tab(
    ui: &mut egui::Ui,
    _ctx: &egui::Context,
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
                if ui.button("Auto-detect").clicked()
                    && let Some(path) = crate::state::detect_rocket_league_path()
                {
                    config_edit.rocket_league_path = path;
                    *changed = true;
                }
            });
        });

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
                Ok(result) => state.stats_api_setup_result.store(Arc::new(result)),
                Err(error) => state.stats_api_setup_result.store(Arc::new(
                    crate::setup::StatsApiSetupResult {
                        message: error,
                        ..Default::default()
                    },
                )),
            }
        }

        let result = state.stats_api_setup_result.load();
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
    render_positioning_settings_section(ui, config_edit, changed);
}

pub(super) fn render_positioning_settings_section(
    ui: &mut egui::Ui,
    config_edit: &mut Config,
    changed: &mut bool,
) {
    settings_section(ui, "Overlay Positioning", |ui| {
        if ui
            .checkbox(&mut config_edit.layout_mode, "Enable Drag Positioning")
            .changed()
        {
            *changed = true;
        }

        if config_edit.layout_mode {
            status_text(
                ui,
                StatusTone::Warning,
                "Drag the visible panels into place. Drag positioning will automatically turn off when settings are closed to restore game click-through.",
            );
        }

        ui.horizontal_wrapped(|ui| {
            if ui.button("Reset Lobby").clicked() {
                config_edit.lobby_manual_position = None;
                *changed = true;
            }
            if ui.button("Reset Boost").clicked() {
                config_edit.teammate_boost_manual_position = None;
                *changed = true;
            }
            if ui.button("Reset Session").clicked() {
                config_edit.session_manual_position = None;
                *changed = true;
            }
        });
    });
}
