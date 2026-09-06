use crate::state::{AppState, Config};
use crate::ui::common::{
    StatusTone, helper_text, overlay_danger_color, overlay_success_color, overlay_text_color,
    overlay_title_color, setting_row, settings_section, status_text,
};
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
                                    .color(overlay_title_color())
                                    .strong(),
                            );
                            ui.label(
                                egui::RichText::new(totals.matches.to_string())
                                    .size(22.0)
                                    .strong()
                                    .color(overlay_text_color()),
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
                                    .color(overlay_title_color())
                                    .strong(),
                            );
                            ui.label(
                                egui::RichText::new(totals.players.to_string())
                                    .size(22.0)
                                    .strong()
                                    .color(overlay_text_color()),
                            );
                        });
                    });
            });
        });
    });

    ui.add_space(10.0);
    ui.collapsing("Maintenance", |ui| {
        ui.label("Delete stored matches and player encounters. New completed matches will still be recorded.");
        if ui.add_enabled(!state.history.clear_running.load(std::sync::atomic::Ordering::SeqCst), egui::Button::new("Clear History…")).clicked() {
            *confirm_modal = Some(crate::ui::app::ConfirmAction::ClearHistory);
        }
        crate::ui::common::maintenance_status(ui, &state.history.clear_status.load());
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
            ui.label(history_muted_text("Search:"));
            let _response = ui.add(
                egui::TextEdit::singleline(search_query)
                    .hint_text("Search by name or platform...")
                    .desired_width((ui.available_width() - 80.0).max(120.0)),
            );
            if !search_query.is_empty() && ui.button("Clear").clicked() {
                search_query.clear();
            }
        });
        ui.add_space(8.0);

        let query = search_query.to_ascii_lowercase().trim().to_string();
        let mut filtered_players: Vec<&crate::history::PlayerHistorySummary> = if query.is_empty() {
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
            let sort_id = ui.id().with("history_sort");
            let (mut sort, mut descending) = ui
                .data(|d| d.get_temp::<(usize, bool)>(sort_id))
                .unwrap_or((7, true));
            ui.horizontal_wrapped(|ui| {
                ui.label("Sort by:");
                egui::ComboBox::from_id_salt("history_sort_column")
                    .selected_text(HISTORY_COLUMNS[sort])
                    .show_ui(ui, |ui| {
                        for (index, label) in HISTORY_COLUMNS.iter().enumerate() {
                            ui.selectable_value(&mut sort, index, *label);
                        }
                    });
                ui.checkbox(&mut descending, "Descending");
            });
            filtered_players.sort_by(|a, b| {
                let order = match sort {
                    0 => a.name_normalized.cmp(&b.name_normalized),
                    1 => a.platform_normalized.cmp(&b.platform_normalized),
                    2 => a.total_games().cmp(&b.total_games()),
                    3 => a.games_with.cmp(&b.games_with),
                    4 => a.games_against.cmp(&b.games_against),
                    5 => rounded_percent_u64(
                        u64::from(a.wins_with),
                        u64::from(a.wins_with) + u64::from(a.losses_with),
                    )
                    .cmp(&rounded_percent_u64(
                        u64::from(b.wins_with),
                        u64::from(b.wins_with) + u64::from(b.losses_with),
                    )),
                    6 => rounded_percent_u64(
                        u64::from(a.wins_against),
                        u64::from(a.wins_against) + u64::from(a.losses_against),
                    )
                    .cmp(&rounded_percent_u64(
                        u64::from(b.wins_against),
                        u64::from(b.wins_against) + u64::from(b.losses_against),
                    )),
                    _ => a.last_seen_unix_ms.cmp(&b.last_seen_unix_ms),
                };
                (if descending { order.reverse() } else { order })
                    .then_with(|| a.player_key.cmp(&b.player_key))
            });
            render_player_table(ui, &filtered_players, &mut sort, &mut descending);
            ui.data_mut(|d| d.insert_temp(sort_id, (sort, descending)));
        }
    });
}

fn render_record(ui: &mut egui::Ui, wins: u32, losses: u32) {
    let (text, color) = record_text_and_color(wins, losses);
    ui.label(history_value_text(text).color(color));
}

fn record_text_and_color(wins: u32, losses: u32) -> (String, egui::Color32) {
    let total = u64::from(wins) + u64::from(losses);
    if total == 0 {
        return ("-".to_string(), overlay_text_color());
    }
    let win_rate = rounded_percent_u64(u64::from(wins), total);
    let text = format!("{win_rate:.0}% ({wins}-{losses})");
    let color = if wins > losses {
        overlay_success_color()
    } else if losses > wins {
        overlay_danger_color()
    } else {
        overlay_text_color()
    };
    (text, color)
}

fn rounded_percent_u64(part: u64, total: u64) -> u32 {
    if total == 0 {
        return 0;
    }

    let percent = (part.saturating_mul(100).saturating_add(total / 2)) / total;
    u32::try_from(percent).unwrap_or(u32::MAX)
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
                    font_id: egui::FontId::proportional(11.0),
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
            overlay_text_color()
        };
        ui.label(history_value_text(platform).color(color));
    }
}

fn history_muted_text(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text.into())
        .size(12.0)
        .color(egui::Color32::from_gray(190))
}

fn history_value_text(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text.into())
        .size(13.0)
        .color(overlay_text_color())
}

const HISTORY_COLUMNS: [&str; 8] = [
    "Player",
    "Platform",
    "Encounters",
    "Teammate",
    "Opponent",
    "Win % together",
    "Win % against",
    "Last seen",
];

fn render_player_table(
    ui: &mut egui::Ui,
    players: &[&crate::history::PlayerHistorySummary],
    sort: &mut usize,
    descending: &mut bool,
) {
    if ui.available_width() < 900.0 {
        for player in players {
            ui.push_id(&player.player_key, |ui| {
                ui.collapsing(
                    format!(
                        "{} · {} · {} encounters",
                        player.name,
                        formatted_platform(&player.platform),
                        player.total_games()
                    ),
                    |ui| {
                        ui.label(format!("As teammate: {} games", player.games_with));
                        ui.label(format!("As opponent: {} games", player.games_against));
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Record together:");
                            render_record(ui, player.wins_with, player.losses_with);
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Record against:");
                            render_record(ui, player.wins_against, player.losses_against);
                        });
                        if ui.button("Copy player name").clicked() {
                            ui.ctx().copy_text(player.name.clone());
                        }
                    },
                );
            });
        }
        return;
    }
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
            for (index, label) in HISTORY_COLUMNS.iter().take(7).enumerate() {
                header.col(|ui| {
                    if ui.selectable_label(*sort == index, *label).clicked() {
                        if *sort == index {
                            *descending = !*descending;
                        } else {
                            *sort = index;
                            *descending = false;
                        }
                    }
                });
            }
        })
        .body(|body| {
            body.rows(24.0, players.len(), |mut row| {
                let player = players[row.index()];
                row.col(|ui| {
                    ui.label(history_value_text(&player.name));
                });
                row.col(|ui| {
                    let platform = formatted_platform(&player.platform);
                    render_platform_label(ui, &platform);
                });
                row.col(|ui| {
                    ui.label(history_value_text(player.total_games().to_string()));
                });
                row.col(|ui| {
                    ui.label(history_value_text(player.games_with.to_string()));
                });
                row.col(|ui| {
                    ui.label(history_value_text(player.games_against.to_string()));
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
        assert_eq!(record_text_and_color(1, 2).0, "33% (1-2)");
        assert_eq!(
            record_text_and_color(u32::MAX, u32::MAX).0,
            format!("50% ({}-{})", u32::MAX, u32::MAX)
        );
        assert_eq!(record_text_and_color(0, 0).0, "-");
    }

    #[test]
    fn platform_formatter_keeps_display_name() {
        assert_eq!(formatted_platform("steam"), "Steam");
    }
}
