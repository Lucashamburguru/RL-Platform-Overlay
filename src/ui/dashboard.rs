use crate::history::{PlayerHistorySummary, player_key};
use crate::mmr::TrackerSnapshot;
use crate::session::SessionMode;
use crate::state::{AppState, Config, DashboardPlayerLayout, LocalPlayerIdentity, PlayerInfo};
use eframe::egui;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::common::contains_ignore_ascii_case;
use super::lobby_overlay::preview_lobby_players;
use super::monitor;

const DASHBOARD_VIEWPORT_ID_SOURCE: &str = "second_monitor_dashboard";

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct DashboardViewportState {
    placement: Option<monitor::MonitorPlacement>,
    pending_fullscreen_restore: Option<monitor::MonitorPlacement>,
}

#[derive(Clone, Debug, PartialEq)]
struct DashboardPlayerRow {
    name: String,
    platform: String,
    team: u8,
    is_local: bool,
    is_bot: bool,
    boost: u8,
    boost_known: bool,
    score: u32,
    goals: u32,
    assists: u32,
    saves: u32,
    shots: u32,
    touches: u32,
    car_touches: u32,
    demos: u32,
    boost_available: bool,
    rank_label: String,
    mmr: Option<i32>,
    matches_played: Option<i32>,
    history_summary: Option<PlayerHistorySummary>,
}

pub(crate) fn render_dashboard_viewport(
    ctx: &egui::Context,
    state: Arc<AppState>,
    config: Config,
    viewport_state: &mut DashboardViewportState,
) {
    let placement = monitor::dashboard_placement(
        ctx,
        config.dashboard_monitor_index,
        config.dashboard_fullscreen,
    );
    let pending_fullscreen_restore = viewport_state.pending_fullscreen_restore == Some(placement);
    if viewport_state.pending_fullscreen_restore.is_some() && !pending_fullscreen_restore {
        viewport_state.pending_fullscreen_restore = None;
    }

    let reset_fullscreen_for_monitor_change = cfg!(target_os = "windows")
        && placement.fullscreen
        && viewport_state.pending_fullscreen_restore.is_none()
        && viewport_state
            .placement
            .is_some_and(|previous| previous.fullscreen && previous != placement);

    let builder_fullscreen = placement.fullscreen && !reset_fullscreen_for_monitor_change;
    let viewport = egui::ViewportBuilder::default()
        .with_title("RL Second Screen Dashboard")
        .with_decorations(true)
        .with_resizable(true)
        .with_transparent(false)
        .with_mouse_passthrough(false)
        .with_fullscreen(builder_fullscreen)
        .with_position(placement.position)
        .with_inner_size(placement.size)
        .with_min_inner_size([960.0, 540.0]);

    if reset_fullscreen_for_monitor_change {
        viewport_state.pending_fullscreen_restore = Some(placement);
    } else if pending_fullscreen_restore {
        viewport_state.pending_fullscreen_restore = None;
    }
    viewport_state.placement = Some(placement);

    ctx.show_viewport_deferred(
        egui::ViewportId::from_hash_of(DASHBOARD_VIEWPORT_ID_SOURCE),
        viewport,
        move |ctx, _class| {
            if ctx.input(|input| input.viewport().close_requested()) {
                state.update_config(|config| config.dashboard_enabled = false);
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                return;
            }

            if reset_fullscreen_for_monitor_change {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(placement.position));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(placement.size.into()));
                ctx.request_repaint();
            } else if pending_fullscreen_restore {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(placement.position));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(placement.size.into()));
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
                ctx.request_repaint();
            }

            egui::CentralPanel::default()
                .frame(
                    egui::Frame::NONE
                        .fill(egui::Color32::from_rgb(10, 12, 16))
                        .inner_margin(egui::Margin::same(18)),
                )
                .show(ctx, |ui| {
                    render_dashboard(ui, &state, &config);
                });
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        },
    );
}

pub(crate) fn render_dashboard(ui: &mut egui::Ui, state: &Arc<AppState>, config: &Config) {
    ui.spacing_mut().item_spacing = egui::vec2(14.0, 12.0);
    ui.set_min_size(ui.available_size());

    let players = state.game.players.load();
    let dashboard_snapshot = state.game.dashboard_match_snapshot.load();
    let session = state.game.session.load();
    let local_identity = state.game.local_player_identity.load();
    let local_player_name = state.game.local_player_name.load();
    let local_mmr = state.mmr.local_mmr.load();
    let history = state.history.player_summaries.load();
    let is_connected = state.flags.is_connected.load(Ordering::SeqCst);
    let snapshot_active =
        !dashboard_snapshot.match_guid.is_empty() && !dashboard_snapshot.players.is_empty();
    let dashboard_session = if snapshot_active {
        dashboard_snapshot.session.clone()
    } else {
        (**session).clone()
    };
    let local_team = if snapshot_active {
        dashboard_snapshot.local_team
    } else {
        dashboard_session.local_team
    }
    .or_else(|| {
        let team = state.game.local_team.load(Ordering::SeqCst);
        (team != crate::state::NO_TEAM).then_some(team)
    });
    let dashboard_players: Vec<PlayerInfo> = if snapshot_active {
        dashboard_snapshot.players.values().cloned().collect()
    } else {
        players.values().cloned().collect()
    };
    let rows = build_dashboard_rows(
        dashboard_players,
        DashboardRowsContext {
            config,
            mode: dashboard_session.active_mode,
            local_team,
            is_replay: dashboard_session.is_watching_replay,
            local_identity: Some(&local_identity),
            local_player_name: Some(local_player_name.as_str()),
            local_mmr: local_mmr.current.as_ref(),
            history_summaries: &history,
        },
    );
    let team_bumps = if config.debounce_touch_counters && config.estimate_teammate_bumps {
        if snapshot_active {
            dashboard_snapshot.team_bumps
        } else {
            state
                .game
                .teammate_bump_estimator
                .lock()
                .map(|estimator| estimator.team_bumps)
                .unwrap_or([0, 0])
        }
    } else {
        [0, 0]
    };

    render_top_band(ui, is_connected, &dashboard_session, rows.len());
    ui.add_space(10.0);

    let available = ui.available_size();
    let optional_side_sections =
        config.dashboard_show_event_feed as u8 + config.dashboard_show_replay_upload as u8;
    let target_side_width = match (available.x >= 1500.0, optional_side_sections) {
        (true, 0) => 310.0,
        (true, 1) => 335.0,
        (true, _) => 360.0,
        (false, 0) => 280.0,
        (false, 1) => 295.0,
        (false, _) => 310.0,
    };
    let side_width = f32::min(target_side_width, available.x * 0.28);
    let gap = 14.0;
    let main_width = (available.x - side_width - gap).max(620.0);
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(main_width, available.y),
            egui::Layout::top_down(egui::Align::Min),
            |ui| render_team_columns(ui, state, config, &rows, team_bumps),
        );
        ui.add_space(gap);
        ui.allocate_ui_with_layout(
            egui::vec2(side_width, available.y),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                render_side_rail(
                    ui,
                    SideRailContext {
                        config,
                        identity: &local_identity,
                        local_mmr: local_mmr.as_ref(),
                        history: &history,
                        state,
                        rows: &rows,
                        session: &dashboard_session,
                    },
                );
            },
        );
    });
}

fn render_scoreboard_hud(ui: &mut egui::Ui, session: &crate::session::SessionState) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);

        // Blue Team Badge
        let blue_tag_frame = egui::Frame::default()
            .fill(egui::Color32::from_rgb(0, 110, 220))
            .corner_radius(6)
            .inner_margin(egui::Margin::symmetric(16, 9));
        blue_tag_frame.show(ui, |ui| {
            ui.label(
                egui::RichText::new("BLUE")
                    .strong()
                    .size(15.0)
                    .color(egui::Color32::WHITE),
            );
        });

        // Blue Score Badge
        let blue_score_frame = egui::Frame::default()
            .fill(egui::Color32::from_rgb(18, 36, 68))
            .stroke(egui::Stroke::new(
                1.0_f32,
                egui::Color32::from_rgb(0, 110, 220),
            ))
            .corner_radius(6)
            .inner_margin(egui::Margin::symmetric(18, 8));
        blue_score_frame.show(ui, |ui| {
            ui.label(
                egui::RichText::new(session.blue_score.to_string())
                    .strong()
                    .size(20.0)
                    .color(egui::Color32::from_rgb(230, 240, 255)),
            );
        });

        // VS divider
        ui.label(
            egui::RichText::new("VS")
                .strong()
                .size(15.0)
                .color(egui::Color32::from_gray(160)),
        );

        // Orange Score Badge
        let orange_score_frame = egui::Frame::default()
            .fill(egui::Color32::from_rgb(68, 36, 18))
            .stroke(egui::Stroke::new(
                1.0,
                egui::Color32::from_rgb(220, 100, 10),
            ))
            .corner_radius(6)
            .inner_margin(egui::Margin::symmetric(18, 8));
        orange_score_frame.show(ui, |ui| {
            ui.label(
                egui::RichText::new(session.orange_score.to_string())
                    .strong()
                    .size(20.0)
                    .color(egui::Color32::from_rgb(255, 240, 230)),
            );
        });

        // Orange Team Badge
        let orange_tag_frame = egui::Frame::default()
            .fill(egui::Color32::from_rgb(220, 100, 10))
            .corner_radius(6)
            .inner_margin(egui::Margin::symmetric(16, 9));
        orange_tag_frame.show(ui, |ui| {
            ui.label(
                egui::RichText::new("ORANGE")
                    .strong()
                    .size(15.0)
                    .color(egui::Color32::WHITE),
            );
        });
    });
}

fn render_top_band(
    ui: &mut egui::Ui,
    is_connected: bool,
    session: &crate::session::SessionState,
    player_count: usize,
) {
    let target_width = ui.available_width();
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(18, 22, 29))
        .stroke(egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgb(55, 64, 78),
        ))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(18, 14));

    frame.show(ui, |ui| {
        let inner_width = (target_width - 36.0).max(0.0);
        ui.set_min_width(inner_width);
        ui.set_max_width(inner_width);
        ui.horizontal(|ui| {
            let connection = if is_connected {
                ("CONNECTED", egui::Color32::from_rgb(105, 220, 135))
            } else {
                ("WAITING", egui::Color32::from_rgb(225, 190, 90))
            };
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("RL Dashboard")
                        .strong()
                        .size(22.0)
                        .color(egui::Color32::from_rgb(238, 241, 248)),
                );
                ui.label(
                    egui::RichText::new(connection.0)
                        .strong()
                        .size(13.0)
                        .color(connection.1),
                );
            });
            ui.separator();
            ui.label(
                egui::RichText::new(session.active_mode.label())
                    .size(24.0)
                    .color(egui::Color32::from_rgb(230, 232, 238)),
            );
            ui.separator();
            ui.add_space(4.0);
            render_scoreboard_hud(ui, session);
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !session.active_match_id.is_empty() {
                    stat_pill(ui, "MATCH", short_match_id(&session.active_match_id));
                }
                stat_pill(ui, "CLOCK", clock_label(session));
                stat_pill(
                    ui,
                    "SESSION",
                    format!(
                        "{}W {}L | {}% | {}",
                        session.wins,
                        session.losses,
                        win_rate(session.wins, session.losses),
                        streak_label(session.streak)
                    ),
                );
                stat_pill(ui, "PLAYERS", player_count.to_string());
            });
        });
    });
}

fn stat_pill(ui: &mut egui::Ui, label: &str, value: impl Into<String>) {
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(27, 31, 39))
        .stroke(egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgb(55, 62, 76),
        ))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(10, 7));
    frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(label)
                    .size(10.0)
                    .strong()
                    .color(egui::Color32::from_gray(145)),
            );
            ui.label(
                egui::RichText::new(value.into())
                    .size(14.0)
                    .color(egui::Color32::from_rgb(228, 231, 238)),
            );
        });
    });
}

fn render_team_columns(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config: &Config,
    rows: &[DashboardPlayerRow],
    team_bumps: [u32; 2],
) {
    if rows.is_empty() {
        render_empty_state(ui, state, config);
        return;
    }

    ui.set_min_size(ui.available_size());
    let section_width = ui.available_width();
    render_team_panel(ui, "Blue Team", 0, rows, config, section_width);
    ui.add_space(14.0);
    render_team_panel(ui, "Orange Team", 1, rows, config, section_width);

    let unknown: Vec<_> = rows
        .iter()
        .filter(|row| row.team != 0 && row.team != 1)
        .collect();
    if !unknown.is_empty() {
        ui.add_space(14.0);
        render_player_table(ui, "Unknown Team", unknown, config, section_width);
    }

    if config.dashboard_show_team_comparison {
        ui.add_space(14.0);
        render_team_comparison(ui, rows, section_width, team_bumps, config);
    }
}

fn render_team_panel(
    ui: &mut egui::Ui,
    title: &str,
    team: u8,
    rows: &[DashboardPlayerRow],
    config: &Config,
    section_width: f32,
) {
    let team_rows: Vec<_> = rows.iter().filter(|row| row.team == team).collect();
    render_player_table(ui, title, team_rows, config, section_width);
}

fn frame_content_width(frame: &egui::Frame, outer_width: f32) -> f32 {
    let horizontal_margin = frame.inner_margin.sum().x + frame.outer_margin.sum().x;
    let horizontal_stroke = frame.stroke.width * 2.0;
    (outer_width - horizontal_margin - horizontal_stroke).max(0.0)
}

fn fixed_width_frame<R>(
    ui: &mut egui::Ui,
    frame: egui::Frame,
    outer_width: f32,
    add_contents: impl FnOnce(&mut egui::Ui, f32) -> R,
) -> egui::InnerResponse<R> {
    let inner_width = frame_content_width(&frame, outer_width);
    frame.show(ui, |ui| {
        ui.set_min_width(inner_width);
        ui.set_max_width(inner_width);
        add_contents(ui, inner_width)
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TeamComparison {
    players: u32,
    score: u32,
    goals: u32,
    assists: u32,
    saves: u32,
    shots: u32,
    touches: u32,
    car_touches: u32,
    demos: u32,
    mmr_total: i32,
    ranked_players: u32,
}

fn render_team_comparison(
    ui: &mut egui::Ui,
    rows: &[DashboardPlayerRow],
    target_width: f32,
    team_bumps: [u32; 2],
    config: &Config,
) {
    let blue = team_comparison(rows, 0);
    let orange = team_comparison(rows, 1);
    let total_touches = blue.touches + orange.touches;
    let blue_possession = possession_pct(blue.touches, total_touches);
    let orange_possession = possession_pct(orange.touches, total_touches);
    let total_shots = blue.shots + orange.shots;
    let blue_shot_share = possession_pct(blue.shots, total_shots);
    let orange_shot_share = possession_pct(orange.shots, total_shots);

    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(17, 20, 27))
        .stroke(egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgb(48, 57, 70),
        ))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(18, 14));
    fixed_width_frame(ui, frame, target_width, |ui, inner_width| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Team Comparison")
                    .strong()
                    .size(18.0)
                    .color(egui::Color32::from_rgb(225, 228, 235)),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("Estimated possession uses touch share")
                        .size(11.0)
                        .color(egui::Color32::from_gray(145)),
                );
            });
        });
        ui.add_space(10.0);
        comparison_bar(
            ui,
            "Estimated Possession",
            blue_possession,
            format!("{blue_possession}%"),
            format!("{orange_possession}%"),
        );
        ui.add_space(8.0);
        comparison_bar(
            ui,
            "Shot Share",
            blue_shot_share,
            format!("{blue_shot_share}%"),
            format!("{orange_shot_share}%"),
        );
        ui.add_space(10.0);
        let num_cols = if inner_width < 820.0 { 2 } else { 5 };
        let spacing_x = 12.0;
        let tile_width = ((inner_width - (spacing_x * (num_cols - 1) as f32)) / num_cols as f32)
            .clamp(120.0, 420.0);

        egui::Grid::new("dashboard_team_comparison")
            .num_columns(num_cols)
            .spacing(egui::vec2(spacing_x, 10.0))
            .show(ui, |ui| {
                let mut index = 0;
                comparison_tile(ui, "Score", blue.score, orange.score, tile_width);
                end_comparison_cell_row(ui, num_cols, &mut index);
                comparison_tile(ui, "Goals", blue.goals, orange.goals, tile_width);
                end_comparison_cell_row(ui, num_cols, &mut index);
                comparison_tile(ui, "Assists", blue.assists, orange.assists, tile_width);
                end_comparison_cell_row(ui, num_cols, &mut index);
                comparison_tile(ui, "Saves", blue.saves, orange.saves, tile_width);
                end_comparison_cell_row(ui, num_cols, &mut index);
                comparison_tile(ui, "Shots", blue.shots, orange.shots, tile_width);
                end_comparison_cell_row(ui, num_cols, &mut index);
                comparison_tile(ui, "Touches", blue.touches, orange.touches, tile_width);
                end_comparison_cell_row(ui, num_cols, &mut index);
                comparison_tile(
                    ui,
                    "Car Touches",
                    blue.car_touches,
                    orange.car_touches,
                    tile_width,
                );
                end_comparison_cell_row(ui, num_cols, &mut index);
                comparison_tile(ui, "Demos", blue.demos, orange.demos, tile_width);
                end_comparison_cell_row(ui, num_cols, &mut index);
                if config.debounce_touch_counters && config.estimate_teammate_bumps {
                    comparison_tile(
                        ui,
                        "Est. Team Bumps",
                        team_bumps[0],
                        team_bumps[1],
                        tile_width,
                    );
                    end_comparison_cell_row(ui, num_cols, &mut index);
                }
                comparison_text_tile(
                    ui,
                    "Avg MMR",
                    avg_mmr_label(blue),
                    avg_mmr_label(orange),
                    tile_width,
                );
                end_comparison_cell_row(ui, num_cols, &mut index);
                comparison_text_tile(
                    ui,
                    "Ranked Players",
                    format!("{}/{}", blue.ranked_players, blue.players),
                    format!("{}/{}", orange.ranked_players, orange.players),
                    tile_width,
                );
                end_comparison_cell_row(ui, num_cols, &mut index);
            });
    });
}

fn end_comparison_cell_row(ui: &mut egui::Ui, num_cols: usize, index: &mut usize) {
    *index += 1;
    if *index == num_cols {
        ui.end_row();
        *index = 0;
    }
}

fn team_comparison(rows: &[DashboardPlayerRow], team: u8) -> TeamComparison {
    rows.iter().filter(|row| row.team == team).fold(
        TeamComparison::default(),
        |mut summary, row| {
            summary.players += 1;
            summary.score += row.score;
            summary.goals += row.goals;
            summary.assists += row.assists;
            summary.saves += row.saves;
            summary.shots += row.shots;
            summary.touches += row.touches;
            summary.car_touches += row.car_touches;
            summary.demos += row.demos;
            if let Some(mmr) = row.mmr
                && !row.is_bot
            {
                summary.mmr_total += mmr;
                summary.ranked_players += 1;
            }
            summary
        },
    )
}

fn avg_mmr_label(team: TeamComparison) -> String {
    let Some(ranked_players) = i32::try_from(team.ranked_players)
        .ok()
        .filter(|count| *count > 0)
    else {
        return "--".to_string();
    };

    (team.mmr_total / ranked_players).to_string()
}

fn comparison_bar(
    ui: &mut egui::Ui,
    label: &str,
    blue_pct: u32,
    blue_label: String,
    orange_label: String,
) {
    ui.label(
        egui::RichText::new(label)
            .size(14.0)
            .strong()
            .color(egui::Color32::from_gray(165)),
    );
    let width = ui.available_width().max(160.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 24.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 6.0, egui::Color32::from_rgb(28, 32, 40));
    let blue_width = rect.width() * (blue_pct as f32 / 100.0).clamp(0.0, 1.0);
    let blue_rect = egui::Rect::from_min_size(rect.min, egui::vec2(blue_width, rect.height()));
    let orange_rect =
        egui::Rect::from_min_max(egui::pos2(rect.min.x + blue_width, rect.min.y), rect.max);
    painter.rect_filled(blue_rect, 6.0, egui::Color32::from_rgb(75, 150, 230));
    painter.rect_filled(orange_rect, 6.0, egui::Color32::from_rgb(230, 130, 65));
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(blue_label)
                .strong()
                .size(16.0)
                .color(egui::Color32::from_rgb(110, 185, 245)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(orange_label)
                    .strong()
                    .size(16.0)
                    .color(egui::Color32::from_rgb(245, 160, 95)),
            );
        });
    });
}

fn comparison_tile(ui: &mut egui::Ui, label: &str, blue: u32, orange: u32, width: f32) {
    let (edge_text, edge_color) = comparison_edge_label(blue, orange);
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(21, 25, 33))
        .stroke(egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgb(38, 45, 56),
        ))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(10, 8));
    fixed_width_frame(ui, frame, width, |ui, inner_width| {
        ui.set_width(inner_width);
        ui.label(
            egui::RichText::new(label)
                .size(11.0)
                .strong()
                .color(egui::Color32::from_gray(155)),
        );
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
            ui.label(
                egui::RichText::new(blue.to_string())
                    .size(18.0)
                    .strong()
                    .color(egui::Color32::from_rgb(110, 185, 245)),
            );
            ui.label(
                egui::RichText::new(orange.to_string())
                    .size(18.0)
                    .strong()
                    .color(egui::Color32::from_rgb(245, 160, 95)),
            );
        });
        ui.label(egui::RichText::new(edge_text).size(11.0).color(edge_color));
    });
}

fn comparison_edge_label(blue: u32, orange: u32) -> (String, egui::Color32) {
    if blue > orange {
        (
            format!("Blue +{}", blue.abs_diff(orange)),
            egui::Color32::from_rgb(110, 185, 245),
        )
    } else if orange > blue {
        (
            format!("Orange +{}", orange.abs_diff(blue)),
            egui::Color32::from_rgb(245, 160, 95),
        )
    } else {
        ("Even".to_string(), egui::Color32::from_gray(145))
    }
}

fn comparison_text_tile(ui: &mut egui::Ui, label: &str, blue: String, orange: String, width: f32) {
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(21, 25, 33))
        .stroke(egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgb(38, 45, 56),
        ))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(10, 8));
    fixed_width_frame(ui, frame, width, |ui, inner_width| {
        ui.set_width(inner_width);
        ui.label(
            egui::RichText::new(label)
                .size(11.0)
                .strong()
                .color(egui::Color32::from_gray(155)),
        );
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
            ui.label(
                egui::RichText::new(blue)
                    .size(18.0)
                    .strong()
                    .color(egui::Color32::from_rgb(110, 185, 245)),
            );
            ui.label(
                egui::RichText::new(orange)
                    .size(18.0)
                    .strong()
                    .color(egui::Color32::from_rgb(245, 160, 95)),
            );
        });
        ui.label(
            egui::RichText::new("—")
                .size(11.0)
                .color(egui::Color32::from_gray(50)),
        );
    });
}

fn possession_pct(touches: u32, total_touches: u32) -> u32 {
    rounded_percent(touches, total_touches)
}

fn render_player_table(
    ui: &mut egui::Ui,
    title: &str,
    rows: Vec<&DashboardPlayerRow>,
    config: &Config,
    target_width: f32,
) {
    let accent = if contains_ignore_ascii_case(title, "Blue") {
        egui::Color32::from_rgb(85, 170, 245)
    } else if contains_ignore_ascii_case(title, "Orange") {
        egui::Color32::from_rgb(245, 150, 80)
    } else {
        egui::Color32::from_rgb(170, 180, 195)
    };
    let stroke_color = if contains_ignore_ascii_case(title, "Blue") {
        egui::Color32::from_rgb(30, 65, 105)
    } else if contains_ignore_ascii_case(title, "Orange") {
        egui::Color32::from_rgb(105, 55, 25)
    } else {
        egui::Color32::from_rgb(48, 57, 70)
    };
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(17, 20, 27))
        .stroke(egui::Stroke::new(1.2_f32, stroke_color))
        .corner_radius(8)
        .inner_margin(egui::Margin::same(14));

    fixed_width_frame(ui, frame, target_width, |ui, _inner_width| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(title).strong().size(20.0).color(accent));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(format!("{} players", rows.len()));
            });
        });
        ui.add_space(12.0);

        match config.dashboard_player_layout {
            DashboardPlayerLayout::Table => render_player_grid(ui, title, &rows, config),
            DashboardPlayerLayout::Cards => {
                let dense = rows.len() >= 3 || target_width < 980.0;
                let row_height = if dense { 74.0 } else { 88.0 };
                for (index, row) in rows.into_iter().enumerate() {
                    if index > 0 {
                        ui.add_space(if dense { 6.0 } else { 8.0 });
                    }
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), row_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| render_player_row(ui, row, config, dense),
                    );
                }
            }
        }
    });
}

fn render_player_grid(
    ui: &mut egui::Ui,
    title: &str,
    rows: &[&DashboardPlayerRow],
    config: &Config,
) {
    let mut columns = 9;
    if config.dashboard_show_boost {
        columns += 1;
    }
    if config.dashboard_show_ranks {
        columns += 1;
    }

    let target_width = ui.available_width();
    let spacing_x = 18.0;
    let total_spacing = spacing_x * (columns - 1) as f32;

    let col_score = 55.0;
    let col_goals = 50.0;
    let col_assists = 50.0;
    let col_saves = 50.0;
    let col_shots = 50.0;
    let col_touches = 60.0;
    let col_car_touches = 80.0;
    let col_demos = 50.0;
    let col_boost = if config.dashboard_show_boost {
        110.0
    } else {
        0.0
    };
    let col_rank = if config.dashboard_show_ranks {
        200.0
    } else {
        0.0
    };

    let fixed_width_sum = col_score
        + col_goals
        + col_assists
        + col_saves
        + col_shots
        + col_touches
        + col_car_touches
        + col_demos
        + col_boost
        + col_rank;

    let player_width = (target_width - total_spacing - fixed_width_sum).max(180.0);

    egui::Grid::new(format!("dashboard_table_{title}"))
        .num_columns(columns)
        .spacing(egui::vec2(spacing_x, 14.0))
        .striped(true)
        .show(ui, |ui| {
            table_header(ui, "Player", player_width);
            table_header(ui, "Score", col_score);
            table_header(ui, "Goals", col_goals);
            table_header(ui, "Assists", col_assists);
            table_header(ui, "Saves", col_saves);
            table_header(ui, "Shots", col_shots);
            table_header(ui, "Touches", col_touches);
            table_header(ui, "Car Touches", col_car_touches);
            table_header(ui, "Demos", col_demos);
            if config.dashboard_show_boost {
                table_header(ui, "Boost", col_boost);
            }
            if config.dashboard_show_ranks {
                table_header(ui, "Rank", col_rank);
            }
            ui.end_row();

            for row in rows {
                render_player_name(ui, row, player_width);
                number_cell(ui, row.score, col_score);
                number_cell(ui, row.goals, col_goals);
                number_cell(ui, row.assists, col_assists);
                number_cell(ui, row.saves, col_saves);
                number_cell(ui, row.shots, col_shots);
                number_cell(ui, row.touches, col_touches);
                number_cell(ui, row.car_touches, col_car_touches);
                number_cell(ui, row.demos, col_demos);
                if config.dashboard_show_boost {
                    ui.allocate_ui_with_layout(
                        egui::vec2(col_boost, 24.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| render_boost(ui, row.boost_available.then_some(row.boost)),
                    );
                }
                if config.dashboard_show_ranks {
                    ui.allocate_ui_with_layout(
                        egui::vec2(col_rank, 48.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| render_rank(ui, row),
                    );
                }
                ui.end_row();
            }
        });
}

fn table_header(ui: &mut egui::Ui, text: &str, width: f32) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, 20.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(
                egui::RichText::new(text)
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_gray(170)),
            );
        },
    );
}

fn number_cell(ui: &mut egui::Ui, value: u32, width: f32) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, 24.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(
                egui::RichText::new(value.to_string())
                    .size(17.0)
                    .color(egui::Color32::from_rgb(222, 225, 232)),
            );
        },
    );
}

fn render_player_row(ui: &mut egui::Ui, row: &DashboardPlayerRow, config: &Config, dense: bool) {
    let row_height = if dense { 74.0 } else { 88.0 };
    let fill = if row.is_local {
        egui::Color32::from_rgb(19, 35, 28)
    } else {
        egui::Color32::from_rgb(14, 17, 23)
    };
    egui::Frame::default()
        .fill(fill)
        .stroke(egui::Stroke::new(
            if row.is_local { 1.2_f32 } else { 0.8_f32 },
            if row.is_local {
                egui::Color32::from_rgb(80, 155, 105)
            } else {
                egui::Color32::from_rgb(35, 42, 52)
            },
        ))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_min_height(row_height);
            ui.set_max_height(row_height);
            ui.horizontal_top(|ui| {
                let row_width = ui.available_width();
                let name_width = (row_width * 0.22).clamp(210.0, 330.0);
                let rank_width = if config.dashboard_show_ranks {
                    250.0
                } else {
                    0.0
                };
                let boost_width = if config.dashboard_show_boost {
                    118.0
                } else {
                    0.0
                };
                let divider_width = if config.dashboard_show_boost && config.dashboard_show_ranks {
                    3.0
                } else if config.dashboard_show_boost || config.dashboard_show_ranks {
                    2.0
                } else {
                    1.0
                };
                let stat_width =
                    (row_width - name_width - rank_width - boost_width - divider_width - 42.0)
                        .max(560.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(name_width, row_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| render_player_name(ui, row, ui.available_width()),
                );
                vertical_divider(ui, row_height - 18.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(stat_width, row_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| render_stat_pack(ui, row, dense),
                );
                if config.dashboard_show_boost {
                    vertical_divider(ui, row_height - 18.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(boost_width, row_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            mini_label(ui, "Boost");
                            render_boost(ui, row.boost_available.then_some(row.boost));
                        },
                    );
                }
                if config.dashboard_show_ranks {
                    vertical_divider(ui, row_height - 18.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(rank_width, row_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| render_rank(ui, row),
                    );
                }
            });
        });
}

fn vertical_divider(ui: &mut egui::Ui, height: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0_f32, height), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.5, egui::Color32::from_rgb(42, 49, 60));
}

fn render_player_name(ui: &mut egui::Ui, row: &DashboardPlayerRow, max_width: f32) {
    ui.allocate_ui_with_layout(
        egui::vec2(max_width.clamp(200.0, 520.0), 48.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
                ui.label(
                    egui::RichText::new(&row.name)
                        .strong()
                        .color(if row.is_local {
                            egui::Color32::from_rgb(120, 220, 155)
                        } else {
                            egui::Color32::from_rgb(230, 232, 238)
                        })
                        .size(19.0),
                );

                if row.is_local {
                    let badge_frame = egui::Frame::default()
                        .fill(egui::Color32::from_rgb(20, 75, 45))
                        .stroke(egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgb(120, 220, 155),
                        ))
                        .corner_radius(4)
                        .inner_margin(egui::Margin::symmetric(6, 2));
                    badge_frame.show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("YOU")
                                .strong()
                                .size(10.5)
                                .color(egui::Color32::from_rgb(120, 220, 155)),
                        );
                    });
                } else if row.is_bot {
                    let badge_frame = egui::Frame::default()
                        .fill(egui::Color32::from_rgb(45, 50, 60))
                        .corner_radius(4)
                        .inner_margin(egui::Margin::symmetric(6, 2));
                    badge_frame.show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("BOT")
                                .strong()
                                .size(10.5)
                                .color(egui::Color32::from_gray(180)),
                        );
                    });
                }
            });

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                ui.add(
                    egui::Image::new(super::lobby_overlay::platform_icon_for(
                        &row.platform,
                        row.is_bot,
                    ))
                    .max_width(12.0)
                    .max_height(12.0)
                    .maintain_aspect_ratio(true),
                );
                super::lobby_overlay::render_platform_name(ui, &row.platform, 10.0);

                if let Some(history) = &row.history_summary {
                    ui.label(
                        egui::RichText::new(" • ")
                            .size(10.0)
                            .color(egui::Color32::from_gray(120)),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "{} with / {} against",
                            history.games_with, history.games_against
                        ))
                        .size(10.0)
                        .color(egui::Color32::from_gray(150)),
                    );
                }
            });
        },
    );
}

fn render_stat_pack(ui: &mut egui::Ui, row: &DashboardPlayerRow, dense: bool) {
    let available = ui.available_width();
    let gap = 8.0;
    let cell_width = ((available - gap * 3.0) / 4.0).clamp(62.0, 120.0);
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            stat_cell(ui, "Score", row.score, dense, true, cell_width);
            stat_cell(ui, "Goals", row.goals, dense, false, cell_width);
            stat_cell(ui, "Assists", row.assists, dense, false, cell_width);
            stat_cell(ui, "Saves", row.saves, dense, false, cell_width);
        });
        ui.add_space(if dense { 4.0 } else { 6.0 });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            stat_cell(ui, "Shots", row.shots, dense, false, cell_width);
            stat_cell(ui, "Touches", row.touches, dense, false, cell_width);
            stat_cell(ui, "Car Touches", row.car_touches, dense, false, cell_width);
            stat_cell(ui, "Demos", row.demos, dense, false, cell_width);
        });
    });
}

fn stat_cell(ui: &mut egui::Ui, label: &str, value: u32, dense: bool, primary: bool, width: f32) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, if dense { 28.0 } else { 32.0 }),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.label(
                egui::RichText::new(value.to_string())
                    .size(if primary { 18.0 } else { 16.0 })
                    .strong()
                    .color(if primary {
                        egui::Color32::from_rgb(238, 241, 248)
                    } else {
                        egui::Color32::from_rgb(205, 211, 222)
                    }),
            );
            ui.label(
                egui::RichText::new(label)
                    .size(9.5)
                    .color(egui::Color32::from_gray(135)),
            );
        },
    );
}

fn mini_label(ui: &mut egui::Ui, label: &str) {
    ui.label(
        egui::RichText::new(label)
            .size(10.0)
            .strong()
            .color(egui::Color32::from_gray(145)),
    );
}

fn render_boost(ui: &mut egui::Ui, boost: Option<u8>) {
    let Some(boost) = boost else {
        ui.label(
            egui::RichText::new("--")
                .size(12.0)
                .color(egui::Color32::from_gray(120)),
        );
        return;
    };

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
        ui.add_sized(
            [70.0, 10.0],
            egui::ProgressBar::new(boost as f32 / 100.0).fill(boost_color(boost)),
        );
        ui.label(
            egui::RichText::new(format!("{boost}"))
                .size(14.0)
                .strong()
                .color(egui::Color32::from_rgb(222, 225, 232)),
        );
    });
}

fn render_rank(ui: &mut egui::Ui, row: &DashboardPlayerRow) {
    if row.is_bot {
        ui.label(
            egui::RichText::new("Bot player")
                .size(13.0)
                .color(egui::Color32::from_gray(145)),
        );
        return;
    }

    let rating = rank_rating_label(&row.rank_label, row.mmr);
    ui.horizontal(|ui| {
        if let Some(icon) = super::lobby_overlay::rank_icon(&row.rank_label) {
            ui.add(
                egui::Image::new(icon)
                    .max_width(30.0)
                    .max_height(30.0)
                    .maintain_aspect_ratio(true),
            );
        }
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
                ui.label(
                    egui::RichText::new(row.rank_label.as_str())
                        .size(14.0)
                        .strong()
                        .color(egui::Color32::from_rgb(224, 227, 234)),
                );
                ui.label(
                    egui::RichText::new(rating.as_str())
                        .size(14.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                );
            });
            if let Some(matches) = row.matches_played {
                ui.label(
                    egui::RichText::new(format!("{matches} matches"))
                        .size(11.0)
                        .color(egui::Color32::from_gray(140)),
                );
            }
        });
    });
}

fn rank_rating_label(rank: &str, mmr: Option<i32>) -> String {
    let Some(mmr) = mmr else {
        return "-".to_string();
    };
    if rank.trim().eq_ignore_ascii_case("unranked") {
        format!("MMR {mmr}")
    } else {
        format!("({mmr})")
    }
}

fn render_empty_state(ui: &mut egui::Ui, state: &Arc<AppState>, config: &Config) {
    let preview = preview_lobby_players(state);
    let history = state.history.player_summaries.load();
    let session = state.game.session.load();
    let local_identity = state.game.local_player_identity.load();
    let local_player_name = state.game.local_player_name.load();
    let rows = build_dashboard_rows(
        preview,
        DashboardRowsContext {
            config,
            mode: session.active_mode,
            local_team: session.local_team,
            is_replay: session.is_watching_replay,
            local_identity: Some(&local_identity),
            local_player_name: Some(local_player_name.as_str()),
            local_mmr: None,
            history_summaries: &history,
        },
    );
    ui.set_min_size(ui.available_size());
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(15, 18, 24))
        .stroke(egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgb(48, 57, 70),
        ))
        .corner_radius(8)
        .inner_margin(egui::Margin::same(18));
    frame.show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("Waiting for live match")
                        .strong()
                        .size(28.0)
                        .color(egui::Color32::from_rgb(235, 238, 245)),
                );
                ui.label(
                    egui::RichText::new(
                        "The dashboard will fill these panels with live Stats API data.",
                    )
                    .size(15.0)
                    .color(egui::Color32::from_gray(178)),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                stat_pill(ui, "PREVIEW", format!("{} players", rows.len()));
            });
        });
        ui.add_space(18.0);
        render_team_columns(ui, state, config, &rows, [0, 0]);
    });
}

struct SideRailContext<'a> {
    config: &'a Config,
    identity: &'a crate::state::LocalPlayerIdentity,
    local_mmr: &'a crate::state::LocalMmrState,
    history: &'a HashMap<String, PlayerHistorySummary>,
    state: &'a Arc<AppState>,
    rows: &'a [DashboardPlayerRow],
    session: &'a crate::session::SessionState,
}

fn render_side_rail(ui: &mut egui::Ui, context: SideRailContext<'_>) {
    ui.set_min_width(ui.available_width());
    render_status_panel(ui, "Local Player", |ui| {
        if context.identity.is_known() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
                let mock_bot = false;
                ui.add(
                    egui::Image::new(super::lobby_overlay::platform_icon_for(
                        &context.identity.platform,
                        mock_bot,
                    ))
                    .max_width(14.0)
                    .max_height(14.0)
                    .maintain_aspect_ratio(true),
                );
                ui.label(
                    egui::RichText::new(context.identity.name.as_str())
                        .strong()
                        .size(16.0)
                        .color(egui::Color32::WHITE),
                );
            });
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(crate::stats_api_parser::format_platform(
                    &context.identity.platform,
                ))
                .size(12.0)
                .color(egui::Color32::from_gray(160)),
            );
        } else {
            ui.label(
                egui::RichText::new("Waiting for identity")
                    .color(egui::Color32::from_rgb(225, 190, 90)),
            );
        }
        if context.local_mmr.fetching {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.label("Refreshing MMR");
            });
        }
        if !context.local_mmr.error.is_empty() {
            ui.colored_label(
                egui::Color32::from_rgb(230, 120, 90),
                &context.local_mmr.error,
            );
        }
    });

    ui.add_space(8.0);
    render_status_panel(ui, "Local MMR", |ui| {
        if let Some(snapshot) = &context.local_mmr.current {
            render_local_mmr_list(ui, snapshot);
        } else {
            ui.label(egui::RichText::new("No snapshot yet").color(egui::Color32::GRAY));
        }
    });

    if context.config.dashboard_show_event_feed {
        ui.add_space(8.0);
        render_status_panel(ui, "Event Feed", |ui| {
            render_event_feed(ui, context.rows, context.session);
        });
    }

    ui.add_space(8.0);
    render_status_panel(ui, "History", |ui| {
        if context.config.history_enabled {
            ui.label(format!(
                "{} current summaries loaded",
                context.history.len()
            ));
        } else {
            ui.label(egui::RichText::new("History off").color(egui::Color32::GRAY));
        }
    });

    if context.config.dashboard_show_replay_upload {
        ui.add_space(8.0);
        render_status_panel(ui, "Replay Upload", |ui| {
            let status = context
                .state
                .replays
                .ballchasing_status
                .lock()
                .map(|status| status.clone())
                .unwrap_or_else(|_| "Status unavailable".to_string());
            let progress = context.state.replays.upload_progress.load();
            ui.label(status);
            if context.state.replays.upload_running.load(Ordering::SeqCst) || progress.running {
                ui.label(format!(
                    "{} / {} processed",
                    progress.processed, progress.total
                ));
            }
        });
    }
}

fn render_event_feed(
    ui: &mut egui::Ui,
    rows: &[DashboardPlayerRow],
    session: &crate::session::SessionState,
) {
    let mut events = dashboard_events(rows);
    if session.is_watching_replay {
        feed_row(
            ui,
            "Replay",
            "Reviewing replay data",
            egui::Color32::from_rgb(225, 190, 90),
        );
    } else if rows.is_empty() {
        feed_row(
            ui,
            "Standby",
            "Waiting for live match",
            egui::Color32::from_gray(150),
        );
    }

    if events.is_empty() && !rows.is_empty() {
        feed_row(
            ui,
            "Live",
            "Stats stream active",
            egui::Color32::from_rgb(105, 220, 135),
        );
        return;
    }

    events.truncate(8);
    for event in events {
        feed_row(ui, event.label, event.detail, event.color);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DashboardEvent {
    weight: u32,
    label: &'static str,
    detail: String,
    color: egui::Color32,
}

fn dashboard_events(rows: &[DashboardPlayerRow]) -> Vec<DashboardEvent> {
    let mut events = Vec::new();
    for row in rows {
        let team = team_label(row.team);
        if row.goals > 0 {
            let suffix = if row.goals == 1 { "goal" } else { "goals" };
            events.push(DashboardEvent {
                weight: 100 + row.goals,
                label: "Goal",
                detail: format!("{}: {} {suffix}", row.name, row.goals),
                color: team_color(row.team),
            });
        }
        if row.assists > 0 {
            let suffix = if row.assists == 1 {
                "assist"
            } else {
                "assists"
            };
            events.push(DashboardEvent {
                weight: 80 + row.assists,
                label: "Assist",
                detail: format!("{}: {} {suffix}", row.name, row.assists),
                color: team_color(row.team),
            });
        }
        if row.saves > 0 {
            let suffix = if row.saves == 1 { "save" } else { "saves" };
            events.push(DashboardEvent {
                weight: 70 + row.saves,
                label: "Save",
                detail: format!("{}: {} {suffix}", row.name, row.saves),
                color: team_color(row.team),
            });
        }
        if row.demos > 0 {
            let suffix = if row.demos == 1 { "demo" } else { "demos" };
            events.push(DashboardEvent {
                weight: 60 + row.demos,
                label: "Demo",
                detail: format!("{}: {} {suffix} for {team}", row.name, row.demos),
                color: team_color(row.team),
            });
        }
        if row.shots >= 3 {
            let suffix = if row.shots == 1 { "shot" } else { "shots" };
            events.push(DashboardEvent {
                weight: 45 + row.shots,
                label: "Pressure",
                detail: format!("{} has {} {suffix}", row.name, row.shots),
                color: team_color(row.team),
            });
        }
        if row.touches >= 10 {
            let suffix = if row.touches == 1 { "touch" } else { "touches" };
            events.push(DashboardEvent {
                weight: 35 + row.touches,
                label: "Control",
                detail: format!("{} has {} {suffix}", row.name, row.touches),
                color: team_color(row.team),
            });
        }
    }
    events.sort_by(|a, b| {
        b.weight
            .cmp(&a.weight)
            .then_with(|| a.detail.cmp(&b.detail))
    });
    events
}

fn feed_row(ui: &mut egui::Ui, label: &str, detail: impl Into<String>, color: egui::Color32) {
    ui.horizontal_top(|ui| {
        let marker = egui::Frame::default()
            .fill(color)
            .corner_radius(3)
            .inner_margin(egui::Margin::symmetric(3, 8));
        marker.show(ui, |_| {});
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(label)
                    .size(11.0)
                    .strong()
                    .color(egui::Color32::from_gray(160)),
            );
            ui.label(
                egui::RichText::new(detail.into())
                    .size(13.0)
                    .color(egui::Color32::from_rgb(220, 224, 232)),
            );
        });
    });
    ui.add_space(6.0);
}

fn team_label(team: u8) -> &'static str {
    match team {
        0 => "Blue",
        1 => "Orange",
        _ => "Unknown",
    }
}

fn team_color(team: u8) -> egui::Color32 {
    match team {
        0 => egui::Color32::from_rgb(85, 170, 245),
        1 => egui::Color32::from_rgb(245, 150, 80),
        _ => egui::Color32::from_gray(165),
    }
}

fn render_status_panel(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    let target_width = ui.available_width();
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(17, 20, 27))
        .stroke(egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgb(48, 57, 70),
        ))
        .corner_radius(8)
        .inner_margin(egui::Margin::same(12));
    frame.show(ui, |ui| {
        ui.set_min_width(target_width - 24.0);
        ui.label(
            egui::RichText::new(title)
                .strong()
                .size(15.0)
                .color(egui::Color32::from_rgb(225, 228, 235)),
        );
        ui.add_space(6.0);
        add_contents(ui);
    });
}

fn render_local_mmr_list(ui: &mut egui::Ui, snapshot: &TrackerSnapshot) {
    let mut playlists: Vec<_> = snapshot.playlists.iter().collect();
    playlists.sort_by_key(|(playlist_id, playlist)| {
        (
            playlist_sort_priority(**playlist_id, playlist.name.as_str()),
            **playlist_id,
        )
    });

    egui::Grid::new("dashboard_local_mmr")
        .num_columns(3)
        .spacing(egui::vec2(10.0, 6.0))
        .striped(true)
        .show(ui, |ui| {
            for (_playlist_id, playlist) in playlists.into_iter().take(7) {
                // Column 1: Rank Icon
                if let Some(icon) = super::lobby_overlay::rank_icon(&playlist.tier_name) {
                    ui.add(
                        egui::Image::new(icon)
                            .max_width(20.0)
                            .max_height(20.0)
                            .maintain_aspect_ratio(true),
                    );
                } else {
                    ui.label("");
                }

                // Column 2: Playlist Name
                ui.label(
                    egui::RichText::new(compact_playlist_name(&playlist.name))
                        .strong()
                        .color(egui::Color32::from_rgb(200, 205, 215)),
                );

                // Column 3: Tier Name & MMR
                ui.label(
                    egui::RichText::new(format!("{} ({})", playlist.tier_name, playlist.rating))
                        .color(egui::Color32::from_rgb(160, 165, 175)),
                );
                ui.end_row();
            }
        });
}

struct DashboardRowsContext<'a> {
    config: &'a Config,
    mode: SessionMode,
    local_team: Option<u8>,
    is_replay: bool,
    local_identity: Option<&'a LocalPlayerIdentity>,
    local_player_name: Option<&'a str>,
    local_mmr: Option<&'a TrackerSnapshot>,
    history_summaries: &'a HashMap<String, PlayerHistorySummary>,
}

fn build_dashboard_rows(
    players: Vec<PlayerInfo>,
    context: DashboardRowsContext<'_>,
) -> Vec<DashboardPlayerRow> {
    let playlist_player_count = players.len();
    let inferred_local_team = context.local_team.or_else(|| {
        players
            .iter()
            .find_map(|player| player.is_local.then_some(player.team))
    });
    let mut rows: Vec<_> = players
        .into_iter()
        .filter(|player| context.config.show_bots || !player.is_bot)
        .map(|player| {
            let is_local = super::lobby_overlay::is_local_lobby_player(
                &player,
                context.local_identity,
                context.local_player_name,
                playlist_player_count,
            );
            let history_summary = (!is_local)
                .then(|| player_key(&player))
                .flatten()
                .and_then(|key| context.history_summaries.get(key.as_str()).cloned());
            let mmr_snapshot = player
                .mmr
                .as_ref()
                .or_else(|| is_local.then_some(context.local_mmr).flatten());
            let playlist = super::lobby_overlay::select_lobby_playlist(
                mmr_snapshot,
                context.mode,
                playlist_player_count,
            );
            let (rank_label, mmr, matches_played) = if let Some(playlist) = playlist {
                (
                    clean_rank_label(&playlist.tier_name),
                    Some(playlist.rating),
                    Some(playlist.matches),
                )
            } else {
                ("Unranked".to_string(), None, None)
            };
            DashboardPlayerRow {
                name: if player.name.trim().is_empty() {
                    "Unknown".to_string()
                } else {
                    player.name
                },
                platform: player.platform,
                team: player.team,
                is_local,
                is_bot: player.is_bot,
                boost: player.boost,
                boost_known: player.boost_known,
                score: player.score,
                goals: player.goals,
                assists: player.assists,
                saves: player.saves,
                shots: player.shots,
                touches: player.touches,
                car_touches: player.car_touches,
                demos: player.demos,
                boost_available: player.boost_known
                    && (context.is_replay || inferred_local_team == Some(player.team)),
                rank_label,
                mmr,
                matches_played,
                history_summary,
            }
        })
        .collect();

    rows.sort_by(|a, b| {
        team_sort_key(a.team)
            .cmp(&team_sort_key(b.team))
            .then_with(|| b.is_local.cmp(&a.is_local))
            .then_with(|| b.score.cmp(&a.score))
            .then_with(|| {
                a.name
                    .bytes()
                    .map(|b| b.to_ascii_lowercase())
                    .cmp(b.name.bytes().map(|b| b.to_ascii_lowercase()))
            })
    });
    rows
}

fn team_sort_key(team: u8) -> u8 {
    match team {
        0 => 0,
        1 => 1,
        _ => 2,
    }
}

fn playlist_sort_priority(playlist_id: i32, playlist_name: &str) -> i32 {
    if playlist_id == 10
        || contains_ignore_ascii_case(playlist_name, "duel")
        || contains_ignore_ascii_case(playlist_name, "1v1")
    {
        0
    } else if playlist_id == 11
        || contains_ignore_ascii_case(playlist_name, "doubles")
        || contains_ignore_ascii_case(playlist_name, "2v2")
    {
        1
    } else if playlist_id == 13
        || contains_ignore_ascii_case(playlist_name, "standard")
        || contains_ignore_ascii_case(playlist_name, "3v3")
    {
        2
    } else if playlist_id == 27 || contains_ignore_ascii_case(playlist_name, "hoops") {
        3
    } else {
        10
    }
}

fn compact_playlist_name(playlist_name: &str) -> String {
    if contains_ignore_ascii_case(playlist_name, "duel")
        || contains_ignore_ascii_case(playlist_name, "1v1")
    {
        "1v1".to_string()
    } else if contains_ignore_ascii_case(playlist_name, "doubles")
        || contains_ignore_ascii_case(playlist_name, "2v2")
    {
        "2v2".to_string()
    } else if contains_ignore_ascii_case(playlist_name, "standard")
        || contains_ignore_ascii_case(playlist_name, "3v3")
    {
        "3v3".to_string()
    } else {
        playlist_name.trim_start_matches("Ranked ").to_string()
    }
}

fn clean_rank_label(rank: &str) -> String {
    if rank.trim().is_empty() {
        "Unranked".to_string()
    } else {
        rank.trim().to_string()
    }
}

fn boost_color(boost: u8) -> egui::Color32 {
    if boost >= 70 {
        egui::Color32::from_rgb(105, 220, 135)
    } else if boost >= 35 {
        egui::Color32::from_rgb(225, 190, 90)
    } else {
        egui::Color32::from_rgb(230, 105, 90)
    }
}

fn win_rate(wins: u32, losses: u32) -> u32 {
    rounded_percent_u64(u64::from(wins), u64::from(wins) + u64::from(losses))
}

fn rounded_percent(part: u32, total: u32) -> u32 {
    rounded_percent_u64(u64::from(part), u64::from(total))
}

fn rounded_percent_u64(part: u64, total: u64) -> u32 {
    if total == 0 {
        return 0;
    }

    let percent = (part.saturating_mul(100).saturating_add(total / 2)) / total;
    u32::try_from(percent).unwrap_or(u32::MAX)
}

fn clock_label(session: &crate::session::SessionState) -> String {
    if session.overtime {
        return "OT".to_string();
    }

    let Some(seconds) = session.time_seconds else {
        return "--:--".to_string();
    };
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn streak_label(streak: i32) -> String {
    if streak > 0 {
        format!("W{streak}")
    } else if streak < 0 {
        format!("L{}", streak.abs())
    } else {
        "Even".to_string()
    }
}

fn short_match_id(match_id: &str) -> String {
    match_id.chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmr::TrackerPlaylistSnapshot;

    fn player(name: &str, team: u8, score: u32, is_local: bool) -> PlayerInfo {
        PlayerInfo {
            name: name.to_string(),
            primary_id: format!("Steam|{name}|0"),
            platform: "steam".to_string(),
            team,
            score,
            is_local,
            boost_known: true,
            ..Default::default()
        }
    }

    fn rows_context<'a>(
        config: &'a Config,
        mode: SessionMode,
        local_team: Option<u8>,
        is_replay: bool,
        local_mmr: Option<&'a TrackerSnapshot>,
        history_summaries: &'a HashMap<String, PlayerHistorySummary>,
    ) -> DashboardRowsContext<'a> {
        DashboardRowsContext {
            config,
            mode,
            local_team,
            is_replay,
            local_identity: None,
            local_player_name: None,
            local_mmr,
            history_summaries,
        }
    }

    #[test]
    fn dashboard_rows_sort_by_team_local_score_and_name() {
        let config = Config::default();
        let rows = build_dashboard_rows(
            vec![
                player("Orange", 1, 900, false),
                player("Beta", 0, 100, false),
                player("Local", 0, 10, true),
                player("Alpha", 0, 100, false),
            ],
            rows_context(
                &config,
                SessionMode::Twos,
                Some(0),
                false,
                None,
                &HashMap::new(),
            ),
        );

        let names: Vec<_> = rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, vec!["Local", "Alpha", "Beta", "Orange"]);
    }

    #[test]
    fn dashboard_rows_filter_bots_when_config_hides_bots() {
        let config = Config {
            show_bots: false,
            ..Default::default()
        };
        let mut bot = player("Bot", 0, 0, false);
        bot.is_bot = true;

        let rows = build_dashboard_rows(
            vec![player("Human", 0, 0, false), bot],
            rows_context(
                &config,
                SessionMode::Twos,
                Some(0),
                false,
                None,
                &HashMap::new(),
            ),
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "Human");
    }

    #[test]
    fn dashboard_rank_playlist_uses_unfiltered_player_count_like_lobby_overlay() {
        let config = Config {
            show_bots: false,
            ..Default::default()
        };
        let mut snapshot = TrackerSnapshot::default();
        snapshot.playlists.insert(
            11,
            TrackerPlaylistSnapshot {
                name: "Ranked Doubles 2v2".to_string(),
                rating: 1100,
                matches: 20,
                tier_name: "Diamond III".to_string(),
            },
        );
        snapshot.playlists.insert(
            13,
            TrackerPlaylistSnapshot {
                name: "Ranked Standard 3v3".to_string(),
                rating: 900,
                matches: 15,
                tier_name: "Platinum III".to_string(),
            },
        );

        let mut local = player("Local", 0, 10, true);
        local.mmr = Some(snapshot);
        let mut bot_one = player("BotOne", 1, 0, false);
        bot_one.is_bot = true;
        let mut bot_two = player("BotTwo", 1, 0, false);
        bot_two.is_bot = true;

        let rows = build_dashboard_rows(
            vec![
                local,
                player("Mate", 0, 100, false),
                player("Opponent", 1, 100, false),
                bot_one,
                bot_two,
            ],
            rows_context(
                &config,
                SessionMode::Unknown,
                Some(0),
                false,
                None,
                &HashMap::new(),
            ),
        );

        let local = rows.iter().find(|row| row.name == "Local").unwrap();
        assert_eq!(local.rank_label, "Platinum III");
        assert_eq!(local.mmr, Some(900));
    }

    #[test]
    fn dashboard_rows_hide_live_enemy_boost_but_show_replay_boost() {
        let config = Config::default();
        let live_rows = build_dashboard_rows(
            vec![player("Local", 0, 0, true), player("Opponent", 1, 0, false)],
            rows_context(
                &config,
                SessionMode::Twos,
                Some(0),
                false,
                None,
                &HashMap::new(),
            ),
        );
        assert!(
            live_rows
                .iter()
                .find(|row| row.name == "Local")
                .unwrap()
                .boost_available
        );
        assert!(
            !live_rows
                .iter()
                .find(|row| row.name == "Opponent")
                .unwrap()
                .boost_available
        );

        let replay_rows = build_dashboard_rows(
            vec![player("Local", 0, 0, true), player("Opponent", 1, 0, false)],
            rows_context(
                &config,
                SessionMode::Twos,
                Some(0),
                true,
                None,
                &HashMap::new(),
            ),
        );
        assert!(
            replay_rows
                .iter()
                .find(|row| row.name == "Opponent")
                .unwrap()
                .boost_available
        );
    }

    #[test]
    fn dashboard_rows_hide_replay_boost_when_api_omits_field() {
        let config = Config::default();
        let mut opponent = player("Opponent", 1, 0, false);
        opponent.boost = 0;
        opponent.boost_known = false;

        let rows = build_dashboard_rows(
            vec![player("Local", 0, 0, true), opponent],
            rows_context(
                &config,
                SessionMode::Twos,
                Some(0),
                true,
                None,
                &HashMap::new(),
            ),
        );

        assert!(
            !rows
                .iter()
                .find(|row| row.name == "Opponent")
                .unwrap()
                .boost_available
        );
    }

    #[test]
    fn dashboard_rows_use_local_mmr_snapshot_for_local_player_rank() {
        let config = Config::default();
        let mut snapshot = TrackerSnapshot::default();
        snapshot.playlists.insert(
            27,
            TrackerPlaylistSnapshot {
                name: "Ranked Hoops".to_string(),
                rating: 989,
                matches: 42,
                tier_name: "Champion II".to_string(),
            },
        );

        let rows = build_dashboard_rows(
            vec![player("Local", 0, 0, true)],
            rows_context(
                &config,
                SessionMode::Hoops,
                Some(0),
                false,
                Some(&snapshot),
                &HashMap::new(),
            ),
        );

        let local = rows.iter().find(|row| row.name == "Local").unwrap();
        assert_eq!(local.rank_label, "Champion II");
        assert_eq!(local.mmr, Some(989));
    }

    #[test]
    fn dashboard_rows_use_lobby_local_identity_for_local_rank() {
        let config = Config::default();
        let identity = LocalPlayerIdentity {
            name: "CachedName".to_string(),
            primary_id: "Steam|123|0".to_string(),
            platform: "Steam".to_string(),
        };
        let mut local_mmr = TrackerSnapshot::default();
        local_mmr.playlists.insert(
            11,
            TrackerPlaylistSnapshot {
                name: "Ranked Doubles 2v2".to_string(),
                rating: 1234,
                matches: 20,
                tier_name: "Champion I".to_string(),
            },
        );
        let mut local_row = player("Renamed", 0, 10, false);
        local_row.primary_id = "steam|123|0".to_string();
        local_row.platform = "steam".to_string();

        let rows = build_dashboard_rows(
            vec![local_row, player("Opponent", 1, 100, false)],
            DashboardRowsContext {
                local_identity: Some(&identity),
                ..rows_context(
                    &config,
                    SessionMode::Twos,
                    Some(0),
                    false,
                    Some(&local_mmr),
                    &HashMap::new(),
                )
            },
        );

        let local = rows.iter().find(|row| row.name == "Renamed").unwrap();
        assert!(local.is_local);
        assert_eq!(local.rank_label, "Champion I");
        assert_eq!(local.mmr, Some(1234));
    }

    #[test]
    fn dashboard_rows_do_not_attach_history_to_local_player() {
        let config = Config::default();
        let local = player("Local", 0, 10, true);
        let local_key = player_key(&local).unwrap().as_str().to_string();
        let mut history = HashMap::new();
        history.insert(
            local_key,
            PlayerHistorySummary {
                games_with: 12,
                games_against: 3,
                ..Default::default()
            },
        );

        let rows = build_dashboard_rows(
            vec![local],
            rows_context(&config, SessionMode::Twos, Some(0), false, None, &history),
        );

        let local = rows.iter().find(|row| row.name == "Local").unwrap();
        assert!(local.is_local);
        assert!(local.history_summary.is_none());
    }

    #[test]
    fn unranked_rank_rating_label_keeps_mmr_visible() {
        assert_eq!(rank_rating_label("Unranked", Some(944)), "MMR 944");
        assert_eq!(rank_rating_label("Diamond II", Some(944)), "(944)");
        assert_eq!(rank_rating_label("Unranked", None), "-");
    }

    #[test]
    fn avg_mmr_label_uses_checked_ranked_player_count() {
        assert_eq!(
            avg_mmr_label(TeamComparison {
                mmr_total: 2400,
                ranked_players: 2,
                ..Default::default()
            }),
            "1200"
        );
        assert_eq!(
            avg_mmr_label(TeamComparison {
                mmr_total: 2400,
                ranked_players: 0,
                ..Default::default()
            }),
            "--"
        );
        assert_eq!(
            avg_mmr_label(TeamComparison {
                mmr_total: 2400,
                ranked_players: i32::MAX as u32 + 1,
                ..Default::default()
            }),
            "--"
        );
    }

    #[test]
    fn comparison_edge_label_handles_full_u32_range() {
        assert_eq!(
            comparison_edge_label(u32::MAX, 0),
            (
                format!("Blue +{}", u32::MAX),
                egui::Color32::from_rgb(110, 185, 245)
            )
        );
        assert_eq!(
            comparison_edge_label(0, u32::MAX),
            (
                format!("Orange +{}", u32::MAX),
                egui::Color32::from_rgb(245, 160, 95)
            )
        );
        assert_eq!(
            comparison_edge_label(7, 7),
            ("Even".to_string(), egui::Color32::from_gray(145))
        );
    }

    #[test]
    fn rounded_percent_uses_integer_rounding_and_handles_large_counts() {
        assert_eq!(rounded_percent(0, 0), 0);
        assert_eq!(rounded_percent(1, 3), 33);
        assert_eq!(rounded_percent(2, 3), 67);
        assert_eq!(rounded_percent(1, 2), 50);
        assert_eq!(rounded_percent(u32::MAX, u32::MAX), 100);
        assert_eq!(win_rate(u32::MAX, u32::MAX), 50);
    }

    #[test]
    fn short_match_id_keeps_first_eight_chars() {
        assert_eq!(short_match_id("abcdefghi"), "abcdefgh");
    }
}
