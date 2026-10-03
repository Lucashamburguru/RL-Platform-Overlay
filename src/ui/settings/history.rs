use super::library_table::{self, Column, cell_text};
use crate::state::{AppState, Config};
use crate::ui::common::{
    StatusTone, overlay_danger_color, overlay_success_color, overlay_text_color,
    overlay_title_color, status_text,
};
use eframe::egui;
use std::sync::Arc;

pub(crate) fn render_history_settings_tab(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config: &mut Config,
    changed: &mut bool,
    confirm_modal: &mut Option<crate::ui::app::ConfirmAction>,
    search: &mut String,
) {
    ui.heading("Player History");
    let totals = state.history.totals.load();
    ui.horizontal_wrapped(|ui| {
        render_total(ui, totals.matches.to_string(), "matches", overlay_title_color());
        ui.separator();
        render_total(ui, totals.players.to_string(), "players met", overlay_title_color());
        ui.separator();
        render_total(ui, totals.wins.to_string(), "wins", overlay_success_color());
        ui.separator();
        render_total(ui, totals.losses.to_string(), "losses", overlay_danger_color());
        ui.separator();
        let (record, color) = record_text_and_color(totals.wins, totals.losses);
        render_total(ui, record.split_whitespace().next().unwrap_or("-"), "win rate", color)
            .on_hover_text("Your wins divided by wins + losses across all stored matches. Each match counts once; searching players does not change these totals.");
    });
    ui.collapsing("History settings & maintenance", |ui| {
        *changed |= ui
            .checkbox(&mut config.history_enabled, "Record completed matches")
            .changed();
        *changed |= ui
            .checkbox(
                &mut config.lobby_history_indicators_enabled,
                "Show encounter counts in lobby",
            )
            .changed();
        ui.weak("History is stored on this device.");
        if ui
            .add_enabled(
                !state
                    .history
                    .clear_running
                    .load(std::sync::atomic::Ordering::SeqCst),
                egui::Button::new("Clear History…"),
            )
            .clicked()
        {
            *confirm_modal = Some(crate::ui::app::ConfirmAction::ClearHistory);
        }
        crate::ui::common::maintenance_status(ui, &state.history.clear_status.load());
    });
    if !config.history_enabled {
        ui.label("Enable history above to record and browse player encounters.");
        return;
    }
    if let Ok(status) = state.history.status.lock() {
        let lower = status.to_lowercase();
        if lower.contains("error") || lower.contains("failed") {
            status_text(ui, StatusTone::Error, &*status);
        }
    }
    let snapshot = state.history.all_players_snapshot.load();
    if !snapshot.error.is_empty() {
        status_text(ui, StatusTone::Error, &snapshot.error);
    }
    if snapshot.refreshing {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.weak("Refreshing history…");
        });
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 78.0).max(40.0);
        ui.add_sized(
            [width, 28.0],
            egui::TextEdit::singleline(search).hint_text("Search players or platforms…"),
        );
        if ui.button("Clear").clicked() {
            search.clear();
        }
    });
    let query = search.trim().to_lowercase();
    let mut players: Vec<_> = snapshot
        .players
        .iter()
        .filter(|p| {
            query.is_empty()
                || p.name.to_lowercase().contains(&query)
                || p.platform.to_lowercase().contains(&query)
        })
        .collect();
    let sort_id = ui.make_persistent_id("history_sort");
    let mut sort = ui
        .data(|d| d.get_temp::<(usize, bool)>(sort_id))
        .unwrap_or((7, true));
    players.sort_by(|a, b| {
        let order = match sort.0 {
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
        (if sort.1 { order.reverse() } else { order }).then_with(|| a.player_key.cmp(&b.player_key))
    });

    ui.weak(format!(
        "{} of {} players · Click a row for details",
        players.len(),
        snapshot.players.len()
    ));
    if players.is_empty() {
        ui.label(if snapshot.players.is_empty() {
            "No completed matches have been stored yet."
        } else {
            "No players match your search."
        });
        return;
    }
    let width = (ui.available_width() - 20.0).max(1.0);
    let compact = width < 700.0;
    let wide = width >= 1000.0;
    let mut columns = vec![Column {
        label: "Player",
        sort: 0,
        width: if compact {
            width - 226.0
        } else if wide {
            width - 740.0
        } else {
            width - 450.0
        },
        numeric: false,
    }];
    if !compact {
        columns.push(Column {
            label: "Platform",
            sort: 1,
            width: 85.0,
            numeric: false,
        });
    }
    columns.push(Column {
        label: "Encounters",
        sort: 2,
        width: 94.0,
        numeric: true,
    });
    if !compact {
        columns.push(Column {
            label: "With",
            sort: 3,
            width: 64.0,
            numeric: true,
        });
        columns.push(Column {
            label: "Against",
            sort: 4,
            width: 75.0,
            numeric: true,
        });
    }
    if wide {
        columns.push(Column {
            label: "Win % together",
            sort: 5,
            width: 145.0,
            numeric: true,
        });
        columns.push(Column {
            label: "Win % against",
            sort: 6,
            width: 145.0,
            numeric: true,
        });
    }
    columns.push(Column {
        label: "Last seen",
        sort: 7,
        width: 132.0,
        numeric: false,
    });
    let keys: Vec<_> = players.iter().map(|p| p.player_key.clone()).collect();
    let row_height = if compact {
        80.0
    } else if wide {
        34.0
    } else {
        48.0
    };
    library_table::show(
        ui,
        "history_table",
        &columns,
        &keys,
        &mut sort,
        row_height,
        |ui, index, column, expanded| {
            let p = players[index];
            match columns[column].sort {
                0 => {
                    ui.vertical(|ui| {
                        cell_text(
                            ui,
                            format!("{} {}", if expanded { "▾" } else { "▸" }, p.name),
                        );
                        if compact {
                            ui.weak(formatted_platform(&p.platform));
                            render_compact_record(ui, "With", p.wins_with, p.losses_with);
                            render_compact_record(ui, "Against", p.wins_against, p.losses_against);
                        }
                    });
                }
                1 => {
                    ui.add(
                        egui::Image::new(crate::ui::lobby_overlay::platform_icon_for(
                            &p.platform,
                            false,
                        ))
                        .fit_to_exact_size(egui::vec2(14.0, 14.0)),
                    );
                    cell_text(ui, formatted_platform(&p.platform));
                }
                2 => cell_text(ui, p.total_games().to_string()),
                3 => render_encounter_record(ui, p.games_with, p.wins_with, p.losses_with, wide),
                4 => render_encounter_record(
                    ui,
                    p.games_against,
                    p.wins_against,
                    p.losses_against,
                    wide,
                ),
                5 => {
                    let (text, color) = record_text_and_color(p.wins_with, p.losses_with);
                    ui.colored_label(color, text);
                }
                6 => {
                    let (text, color) = record_text_and_color(p.wins_against, p.losses_against);
                    ui.colored_label(color, text);
                }
                _ => cell_text(ui, library_table::local_timestamp(p.last_seen_unix_ms)),
            }
        },
        |ui, index| {
            let p = players[index];
            ui.strong(&p.name);
            ui.label(format!(
                "{} · Last seen {} (local time)",
                formatted_platform(&p.platform),
                library_table::local_timestamp_detail(p.last_seen_unix_ms)
            ));
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                render_record_card(ui, "Together", p.games_with, p.wins_with, p.losses_with);
                render_record_card(
                    ui,
                    "Against",
                    p.games_against,
                    p.wins_against,
                    p.losses_against,
                );
            });
            ui.add_space(4.0);
            if ui.button("Copy Name").clicked() {
                ui.ctx().copy_text(p.name.clone());
            }
        },
    );
    ui.data_mut(|d| d.insert_temp(sort_id, sort));
}

fn render_total(
    ui: &mut egui::Ui,
    value: impl Into<String>,
    label: &str,
    color: egui::Color32,
) -> egui::Response {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(value.into()).strong().color(color));
        ui.weak(label);
    })
    .response
}

fn render_compact_record(ui: &mut egui::Ui, label: &str, wins: u32, losses: u32) {
    let (record, color) = record_text_and_color(wins, losses);
    let percentage = record.split_whitespace().next().unwrap_or("-");
    ui.add(
        egui::Label::new(
            egui::RichText::new(format!("{label} {percentage}"))
                .size(11.0)
                .color(color),
        )
        .truncate(),
    )
    .on_hover_text(format!(
        "{label}: {record} · Your wins and losses in completed matches"
    ));
}

fn render_encounter_record(ui: &mut egui::Ui, games: u32, wins: u32, losses: u32, wide: bool) {
    if wide {
        cell_text(ui, games.to_string());
    } else {
        ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
            cell_text(ui, games.to_string());
            let (record, color) = record_text_and_color(wins, losses);
            let percentage = record.split_whitespace().next().unwrap_or("-");
            ui.label(
                egui::RichText::new(percentage)
                    .size(11.0)
                    .strong()
                    .color(color),
            )
            .on_hover_text(format!(
                "{record} · Your wins and losses in completed matches"
            ));
        });
    }
}

fn render_record_card(ui: &mut egui::Ui, label: &str, games: u32, wins: u32, losses: u32) {
    let (record, color) = record_text_and_color(wins, losses);
    let width = ui.available_width().clamp(1.0, 190.0);
    egui::Frame::NONE
        .fill(egui::Color32::from_gray(27))
        .corner_radius(5)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width((width - 20.0).max(1.0));
                ui.strong(label);
                ui.label(
                    egui::RichText::new(&record)
                        .size(18.0)
                        .strong()
                        .color(color),
                )
                .on_hover_text("Your win percentage and win–loss record in completed matches");
                ui.weak(format!("{games} encounters"));
                let total = u64::from(wins) + u64::from(losses);
                if total > 0 {
                    ui.add(
                        egui::ProgressBar::new((f64::from(wins) / total as f64) as f32)
                            .desired_width(ui.available_width())
                            .desired_height(5.0)
                            .fill(color),
                    );
                } else {
                    ui.weak("No results recorded");
                }
            });
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn history_library_layout() {
        let state = AppState::new();
        state
            .history
            .totals
            .store(Arc::new(crate::history::HistoryTotals {
                matches: 66,
                players: 100,
                wins: 40,
                losses: 26,
            }));
        let players = (0..100)
            .map(|i| crate::history::PlayerHistorySummary {
                player_key: format!("player-{i:03}"),
                name: if i == 0 {
                    "A very long player name 漢字".into()
                } else {
                    format!("Player {i}")
                },
                platform: if i % 2 == 0 {
                    "Steam".into()
                } else {
                    "Epic".into()
                },
                games_with: 12,
                games_against: 23,
                wins_with: [7, 4, 6][i as usize % 3],
                losses_with: [5, 8, 6][i as usize % 3],
                wins_against: [8, 15, 0][i as usize % 3],
                losses_against: [15, 8, 0][i as usize % 3],
                last_seen_unix_ms: 1_783_800_000_000 - i,
                ..Default::default()
            })
            .collect();
        state.history.all_players_snapshot.store(Arc::new(
            crate::history::HistoryPlayersSnapshot {
                loaded: true,
                players,
                ..Default::default()
            },
        ));
        let mut config = Config {
            history_enabled: true,
            ..Default::default()
        };
        let mut changed = false;
        let mut confirm = None;
        let mut search = String::new();
        library_table::assert_page_layout("history", |ui, expanded| {
            if expanded {
                let id = ui.make_persistent_id("history_table").with("selected");
                ui.data_mut(|d| d.insert_temp(id, "player-000".to_owned()));
            }
            render_history_settings_tab(
                ui,
                &state,
                &mut config,
                &mut changed,
                &mut confirm,
                &mut search,
            );
        });
    }

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
