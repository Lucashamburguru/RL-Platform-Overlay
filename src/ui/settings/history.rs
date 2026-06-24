use crate::state::{AppState, Config};
use crate::ui::common::{StatusTone, helper_text, setting_row, settings_section, status_text};
use eframe::egui;
use egui_extras::{Column, TableBuilder};
use std::sync::Arc;

pub(crate) fn render_history_settings_tab(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config_edit: &mut Config,
    changed: &mut bool,
    confirm_modal: &mut Option<crate::ui::app::ConfirmAction>,
    search_query: &mut String,
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

        let tone = if status.contains("error") || status.contains("failed") {
            StatusTone::Error
        } else if status.contains("ready") || status.contains("saved") || status.contains("cleared")
        {
            StatusTone::Success
        } else {
            StatusTone::Neutral
        };
        status_text(ui, tone, status);
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
            // Dashboard card for Matches
            ui.vertical(|ui| {
                egui::Frame::NONE
                    .fill(egui::Color32::from_gray(30))
                    .inner_margin(8.0)
                    .corner_radius(4)
                    .show(ui, |ui| {
                        ui.set_width(120.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new("MATCHES")
                                    .size(9.0)
                                    .color(egui::Color32::from_gray(160))
                                    .strong(),
                            );
                            ui.label(
                                egui::RichText::new(totals.matches.to_string())
                                    .size(22.0)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            );
                        });
                    });
            });
            ui.add_space(8.0);
            // Dashboard card for Players
            ui.vertical(|ui| {
                egui::Frame::NONE
                    .fill(egui::Color32::from_gray(30))
                    .inner_margin(8.0)
                    .corner_radius(4)
                    .show(ui, |ui| {
                        ui.set_width(120.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new("PLAYERS MET")
                                    .size(9.0)
                                    .color(egui::Color32::from_gray(160))
                                    .strong(),
                            );
                            ui.label(
                                egui::RichText::new(totals.players.to_string())
                                    .size(22.0)
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            );
                        });
                    });
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("🗑 Clear History").clicked() {
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

        let snapshot = state.history.all_players_snapshot.load();
        if !snapshot.loaded && snapshot.refreshing {
            ui.label(helper_text("Loading history..."));
            return;
        }
        if !snapshot.error.is_empty() {
            status_text(
                ui,
                StatusTone::Error,
                format!("Could not load history: {}", snapshot.error),
            );
            if snapshot.players.is_empty() {
                return;
            }
        } else if snapshot.refreshing {
            ui.label(helper_text("Refreshing history..."));
        }

        if snapshot.players.is_empty() {
            ui.label(helper_text("No completed matches have been stored yet."));
            return;
        }

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Search:").color(egui::Color32::from_gray(160)));
            let _response = ui.add(
                egui::TextEdit::singleline(search_query)
                    .hint_text("Search by name or platform...")
                    .desired_width(220.0),
            );
            if !search_query.is_empty() && ui.button("Clear").clicked() {
                search_query.clear();
            }
        });
        ui.add_space(8.0);

        let query = search_query.to_ascii_lowercase().trim().to_string();
        let filtered_players: Vec<&crate::history::PlayerHistorySummary> = if query.is_empty() {
            snapshot.players.iter().collect()
        } else {
            snapshot
                .players
                .iter()
                .filter(|p| {
                    p.name_normalized.contains(&query) || p.platform_normalized.contains(&query)
                })
                .collect()
        };

        if filtered_players.is_empty() {
            ui.label(helper_text("No players match your search."));
        } else {
            render_player_table(ui, &filtered_players);
        }
    });
}

fn render_record(ui: &mut egui::Ui, wins: u32, losses: u32) {
    let (text, color) = record_text_and_color(wins, losses);
    ui.label(egui::RichText::new(text).color(color));
}

fn record_text_and_color(wins: u32, losses: u32) -> (String, egui::Color32) {
    let total = wins + losses;
    if total == 0 {
        return ("-".to_string(), egui::Color32::from_gray(180));
    }
    let win_rate = (wins as f32 / total as f32) * 100.0;
    let text = format!("{win_rate:.0}% ({wins}-{losses})");
    let color = if wins > losses {
        egui::Color32::from_rgb(100, 220, 140) // Green
    } else if losses > wins {
        egui::Color32::from_rgb(230, 120, 120) // Red
    } else {
        egui::Color32::from_gray(180) // Gray
    };
    (text, color)
}

fn formatted_platform(platform: &str) -> String {
    crate::stats_api_parser::format_platform(platform).to_string()
}

fn render_platform_label(ui: &mut egui::Ui, platform: &str) {
    if crate::ui::common::contains_ignore_ascii_case(platform, "epic") {
        let mut job = egui::text::LayoutJob::default();
        let colors = [
            egui::Color32::from_rgb(255, 90, 90),   // Red
            egui::Color32::from_rgb(255, 170, 70),  // Orange
            egui::Color32::from_rgb(240, 220, 50),  // Yellow
            egui::Color32::from_rgb(70, 230, 90),   // Green
            egui::Color32::from_rgb(70, 180, 255),  // Blue
            egui::Color32::from_rgb(180, 100, 255), // Purple
        ];
        for (i, c) in platform.chars().enumerate() {
            let color = colors[i % colors.len()];
            job.append(
                &c.to_string(),
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::proportional(12.5),
                    color,
                    ..Default::default()
                },
            );
        }
        ui.label(job);
    } else {
        let color = if crate::ui::common::contains_ignore_ascii_case(platform, "xbox")
            || crate::ui::common::contains_ignore_ascii_case(platform, "xbl")
        {
            egui::Color32::from_rgb(30, 200, 80) // Xbox Green
        } else if crate::ui::common::contains_ignore_ascii_case(platform, "playstation")
            || crate::ui::common::contains_ignore_ascii_case(platform, "ps")
        {
            egui::Color32::from_rgb(41, 140, 255) // PSN Blue
        } else if crate::ui::common::contains_ignore_ascii_case(platform, "switch")
            || crate::ui::common::contains_ignore_ascii_case(platform, "nintendo")
        {
            egui::Color32::from_rgb(255, 65, 80) // Switch Red
        } else {
            egui::Color32::from_gray(160) // Steam / Default stays gray
        };
        ui.label(egui::RichText::new(platform).color(color));
    }
}

fn render_player_table(ui: &mut egui::Ui, players: &[&crate::history::PlayerHistorySummary]) {
    let table_height = (players.len() as f32 * 24.0 + 28.0).min(420.0);
    TableBuilder::new(ui)
        .striped(true)
        .resizable(false)
        .min_scrolled_height(table_height)
        .max_scroll_height(table_height)
        .column(Column::remainder().at_least(110.0))
        .column(Column::auto().at_least(78.0))
        .column(Column::auto().at_least(42.0))
        .column(Column::auto().at_least(42.0))
        .column(Column::auto().at_least(42.0))
        .column(Column::auto().at_least(76.0))
        .column(Column::auto().at_least(76.0))
        .header(22.0, |mut header| {
            header.col(|ui| {
                ui.strong("Player");
            });
            header.col(|ui| {
                ui.strong("Platform");
            });
            header.col(|ui| {
                ui.strong("Seen");
            });
            header.col(|ui| {
                ui.strong("With");
            });
            header.col(|ui| {
                ui.strong("Vs");
            });
            header.col(|ui| {
                ui.strong("W/L With");
            });
            header.col(|ui| {
                ui.strong("W/L Vs");
            });
        })
        .body(|body| {
            body.rows(24.0, players.len(), |mut row| {
                let player = players[row.index()];
                row.col(|ui| {
                    ui.label(&player.name);
                });
                row.col(|ui| {
                    let platform = formatted_platform(&player.platform);
                    render_platform_label(ui, &platform);
                });
                row.col(|ui| {
                    ui.label(player.total_games().to_string());
                });
                row.col(|ui| {
                    ui.label(player.games_with.to_string());
                });
                row.col(|ui| {
                    ui.label(player.games_against.to_string());
                });
                row.col(|ui| {
                    render_record(ui, player.wins_with, player.losses_with);
                });
                row.col(|ui| {
                    render_record(ui, player.wins_against, player.losses_against);
                });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_text_formats_wins_losses() {
        assert_eq!(record_text_and_color(3, 1).0, "75% (3-1)");
        assert_eq!(record_text_and_color(0, 0).0, "-");
    }

    #[test]
    fn platform_formatter_keeps_display_name() {
        assert_eq!(formatted_platform("steam"), "Steam");
    }
}
