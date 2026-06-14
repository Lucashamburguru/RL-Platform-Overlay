use crate::state::{AppState, Config};
use crate::ui::common::{StatusTone, helper_text, setting_row, settings_section, status_text};
use eframe::egui;
use std::sync::Arc;

pub(crate) fn render_history_settings_tab(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config_edit: &mut Config,
    changed: &mut bool,
    confirm_modal: &mut Option<crate::ui::app::ConfirmAction>,
    history_players: Option<&Result<Vec<crate::history::PlayerHistorySummary>, String>>,
) {
    settings_section(ui, "Player History", |ui| {
        setting_row(ui, "Storage", |ui| {
            if ui
                .checkbox(
                    &mut config_edit.history_enabled,
                    "Enable local SQLite history",
                )
                .changed()
            {
                *changed = true;
            }
        });

        ui.add_space(6.0);
        setting_row(ui, "Lobby", |ui| {
            if ui
                .checkbox(
                    &mut config_edit.lobby_history_indicators_enabled,
                    "Show compact history counts in lobby",
                )
                .changed()
            {
                *changed = true;
            }
        });

        ui.add_space(8.0);
        ui.label(helper_text(
            "History is stored locally in SQLite and records completed matches only.",
        ));

        ui.add_space(10.0);
        let status = state
            .history
            .status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_else(|_| "History status unavailable.".to_string());
        status_text(ui, StatusTone::Neutral, status);
    });

    ui.add_space(10.0);
    settings_section(ui, "Summary", |ui| {
        if !config_edit.history_enabled {
            ui.label(helper_text(
                "Enable history to start storing player match records.",
            ));
            return;
        }

        let totals = state.history.totals.load();
        ui.horizontal(|ui| {
            ui.label(format!("Matches: {}", totals.matches));
            ui.add_space(16.0);
            ui.label(format!("Players: {}", totals.players));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Clear History").clicked() {
                    *confirm_modal = Some(crate::ui::app::ConfirmAction::ClearHistory);
                }
            });
        });
    });

    ui.add_space(10.0);
    settings_section(ui, "Players", |ui| {
        if !config_edit.history_enabled {
            ui.label(helper_text(
                "No player history is shown while history is disabled.",
            ));
            return;
        }

        let Some(history_players) = history_players else {
            ui.label(helper_text("Loading history..."));
            return;
        };

        match history_players {
            Ok(players) if players.is_empty() => {
                ui.label(helper_text("No completed matches have been stored yet."));
            }
            Ok(players) => render_player_table(ui, players),
            Err(error) => {
                status_text(
                    ui,
                    StatusTone::Error,
                    format!("Could not load history: {error}"),
                );
            }
        }
    });
}

fn render_player_table(ui: &mut egui::Ui, players: &[crate::history::PlayerHistorySummary]) {
    egui::Grid::new("history_players_grid")
        .striped(true)
        .min_col_width(72.0)
        .show(ui, |ui| {
            ui.strong("Player");
            ui.strong("Platform");
            ui.strong("Seen");
            ui.strong("With");
            ui.strong("Vs");
            ui.strong("W/L With");
            ui.strong("W/L Vs");
            ui.end_row();

            for player in players {
                ui.label(&player.name);
                ui.label(&player.platform);
                ui.label(player.total_games().to_string());
                ui.label(player.games_with.to_string());
                ui.label(player.games_against.to_string());
                ui.label(format!("{}/{}", player.wins_with, player.losses_with));
                ui.label(format!("{}/{}", player.wins_against, player.losses_against));
                ui.end_row();
            }
        });
}
