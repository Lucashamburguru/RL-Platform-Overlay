use crate::mmr::TrackerSnapshot;
use crate::session::SessionMode;
use crate::state::{AppState, LocalPlayerIdentity, PlayerInfo};
use eframe::egui;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::layout::{
    active_layout_drag_position, normalized_to_pos, persist_dragged_position,
    render_drag_position_handle,
};

pub(super) fn preview_lobby_players(state: &Arc<AppState>) -> Vec<PlayerInfo> {
    let players = state.game.players.load();
    let mut lobby_players: Vec<PlayerInfo> = players.values().cloned().collect();

    if lobby_players.is_empty() {
        let local_identity = state.game.local_player_identity.load();
        let local_mmr = state.mmr.local_mmr.load();
        let session = state.game.session.load();
        let local_team = session
            .local_team
            .or_else(|| {
                let team = state.game.local_team.load(Ordering::SeqCst);
                (team != crate::state::NO_TEAM).then_some(team)
            })
            .unwrap_or(0);
        let opponent_team = if local_team == 0 { 1 } else { 0 };
        let (local_name, local_platform, local_primary_id) = if local_identity.is_known() {
            (
                local_identity.name.clone(),
                local_identity.platform.clone(),
                local_identity.primary_id.clone(),
            )
        } else {
            (
                "You (Local)".to_string(),
                "steam".to_string(),
                "Steam|preview|0".to_string(),
            )
        };

        lobby_players = vec![
            PlayerInfo {
                name: local_name,
                primary_id: local_primary_id,
                platform: local_platform,
                team: local_team,
                is_bot: false,
                is_local: true,
                boost: 100,
                boost_known: true,
                score: 350,
                goals: 1,
                assists: 1,
                saves: 1,
                shots: 3,
                touches: 14,
                car_touches: 3,
                demos: 2,
                mmr: local_mmr
                    .current
                    .clone()
                    .or_else(|| Some(preview_mmr(1150, "Champion I"))),
            },
            PlayerInfo {
                name: "OpponentOne".to_string(),
                primary_id: "Epic|preview|0".to_string(),
                platform: "epic".to_string(),
                team: opponent_team,
                is_bot: false,
                is_local: false,
                boost: 45,
                boost_known: true,
                score: 210,
                goals: 0,
                assists: 0,
                saves: 2,
                shots: 2,
                touches: 8,
                car_touches: 1,
                demos: 0,
                mmr: Some(preview_mmr(1045, "Diamond II")),
            },
        ];
    }

    lobby_players
}

fn preview_mmr(rating: i32, tier_name: &str) -> TrackerSnapshot {
    let mut playlists = HashMap::new();

    // Dynamic mapping for 1v1 tier name based on its rating
    let one_v_one_rating = rating - 150;
    let one_v_one_tier = if one_v_one_rating >= 1400 {
        "Grand Champion I"
    } else if one_v_one_rating >= 1200 {
        "Champion I"
    } else if one_v_one_rating >= 1000 {
        "Diamond II"
    } else if one_v_one_rating >= 800 {
        "Platinum III"
    } else if one_v_one_rating >= 600 {
        "Gold III"
    } else {
        "Silver III"
    };

    playlists.insert(
        10,
        crate::mmr::TrackerPlaylistSnapshot {
            name: "Ranked Duel 1v1".to_string(),
            rating: one_v_one_rating,
            matches: 30,
            tier_name: one_v_one_tier.to_string(),
        },
    );
    playlists.insert(
        11,
        crate::mmr::TrackerPlaylistSnapshot {
            name: "Ranked Doubles 2v2".to_string(),
            rating,
            matches: 120,
            tier_name: tier_name.to_string(),
        },
    );
    playlists.insert(
        13,
        crate::mmr::TrackerPlaylistSnapshot {
            name: "Ranked Standard 3v3".to_string(),
            rating: rating - 80,
            matches: 50,
            tier_name: tier_name.to_string(),
        },
    );
    playlists.insert(
        27,
        crate::mmr::TrackerPlaylistSnapshot {
            name: "Ranked Hoops".to_string(),
            rating: rating - 35,
            matches: 40,
            tier_name: tier_name.to_string(),
        },
    );
    playlists.insert(
        29,
        crate::mmr::TrackerPlaylistSnapshot {
            name: "Ranked Dropshot".to_string(),
            rating: rating - 55,
            matches: 25,
            tier_name: tier_name.to_string(),
        },
    );
    TrackerSnapshot {
        playlists,
        last_updated: None,
        current_season: None,
    }
}

pub(super) fn lobby_theme_label(theme: crate::state::LobbyTheme) -> &'static str {
    match theme {
        crate::state::LobbyTheme::Glass => "Glassmorphism",
        crate::state::LobbyTheme::Solid => "High-Contrast Solid",
        crate::state::LobbyTheme::Modern => "Modern Cyber",
        crate::state::LobbyTheme::Minimalist => "Minimalist Floating",
    }
}

pub(super) fn lobby_display_mode_label(mode: crate::state::LobbyDisplayMode) -> &'static str {
    match mode {
        crate::state::LobbyDisplayMode::Compact => "Compact",
        crate::state::LobbyDisplayMode::Expanded => "Expanded",
    }
}

pub(super) fn render_overlay(ctx: &egui::Context, state: &Arc<AppState>) {
    let config = state.system.config.load();
    let players = state.game.players.load();

    let area = egui::Area::new("overlay_area".into()).order(egui::Order::Foreground);
    let area = if let Some(position) = active_layout_drag_position(ctx, "lobby") {
        area.fixed_pos(position)
    } else if let Some(position) = config.lobby_manual_position {
        area.fixed_pos(normalized_to_pos(ctx, position))
    } else {
        // Fallback default: Center Right
        area.anchor(
            egui::Align2::RIGHT_CENTER,
            egui::vec2(-20.0, 0.0) * config.ui_scale,
        )
    };

    let area_response = area.show(ctx, |ui| {
        let players_vec: Vec<PlayerInfo> = if config.layout_mode {
            preview_lobby_players(state)
        } else {
            players.values().cloned().collect()
        };
        let local_identity = state.game.local_player_identity.load();
        let local_mmr = state.mmr.local_mmr.load();
        let session = state.game.session.load();
        let history_summaries = state.history.player_summaries.load();
        draw_lobby_panel(
            ui,
            &players_vec,
            &config,
            state.flags.is_connected.load(Ordering::SeqCst),
            Some(&local_identity),
            local_mmr.current.as_ref(),
            session.active_mode,
            Some(&history_summaries),
            None,
        );
        render_drag_position_handle(ui, config.layout_mode, config.ui_scale)
    });

    if let Some(drag_response) = area_response.inner {
        persist_dragged_position(
            ctx,
            state,
            area_response.response.rect.min,
            "lobby",
            &drag_response,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_lobby_panel(
    ui: &mut egui::Ui,
    players: &[PlayerInfo],
    config: &crate::state::Config,
    is_connected: bool,
    local_identity: Option<&LocalPlayerIdentity>,
    local_mmr: Option<&TrackerSnapshot>,
    session_mode: SessionMode,
    history_summaries: Option<&HashMap<String, crate::history::PlayerHistorySummary>>,
    scale_override: Option<f32>,
) {
    let scale = scale_override.unwrap_or(config.ui_scale);
    let (fill, stroke) = match config.lobby_theme {
        crate::state::LobbyTheme::Glass => (
            egui::Color32::from_rgba_unmultiplied(20, 20, 25, config.transparency),
            egui::Stroke::new(
                1.0,
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, 20),
            ),
        ),
        crate::state::LobbyTheme::Solid => (
            egui::Color32::from_rgba_unmultiplied(10, 10, 12, 255),
            egui::Stroke::new(1.0, egui::Color32::from_gray(50)),
        ),
        crate::state::LobbyTheme::Modern => (
            egui::Color32::from_rgba_unmultiplied(12, 14, 18, config.transparency.max(220)),
            egui::Stroke::new(1.2, egui::Color32::from_rgba_unmultiplied(0, 176, 255, 140)),
        ),
        crate::state::LobbyTheme::Minimalist => (egui::Color32::TRANSPARENT, egui::Stroke::NONE),
    };

    let frame = egui::Frame::default()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(6.0 * scale)
        .inner_margin(8.0 * scale);

    frame.show(ui, |ui| {
        let content_width = match config.lobby_display_mode {
            crate::state::LobbyDisplayMode::Compact => 270.0 * scale,
            crate::state::LobbyDisplayMode::Expanded => 315.0 * scale,
        };
        ui.set_min_width(content_width);
        ui.set_max_width(content_width);

        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("LOBBY")
                        .size(10.0 * scale)
                        .color(egui::Color32::from_gray(180))
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let status_color = if is_connected {
                        egui::Color32::from_rgb(0, 255, 150)
                    } else {
                        egui::Color32::from_rgb(255, 80, 80)
                    };
                    ui.label(
                        egui::RichText::new("●")
                            .color(status_color)
                            .size(7.0 * scale),
                    );
                    ui.label(
                        egui::RichText::new(session_mode.label())
                            .size(8.5 * scale)
                            .color(egui::Color32::from_gray(150)),
                    );
                });
            });

            ui.add_space(4.0 * scale);

            let player_count = players.len();
            let mut sorted_players: Vec<_> = players
                .iter()
                .filter(|p| config.show_bots || !p.is_bot)
                .collect();
            sorted_players.sort_by(|a, b| {
                let a_local = is_local_lobby_player(a, local_identity, player_count);
                let b_local = is_local_lobby_player(b, local_identity, player_count);
                a.team
                    .cmp(&b.team)
                    .then_with(|| b_local.cmp(&a_local))
                    // ⚡ Bolt: Use zero-allocation iterator for case-insensitive sort to prevent heap allocations in render loop
                    .then_with(|| {
                        a.name
                            .bytes()
                            .map(|b| b.to_ascii_lowercase())
                            .cmp(b.name.bytes().map(|b| b.to_ascii_lowercase()))
                    })
            });

            if sorted_players.is_empty() {
                ui.label(
                    egui::RichText::new("Waiting...")
                        .size(11.0 * scale)
                        .italics()
                        .color(egui::Color32::from_gray(120)),
                );
            } else {
                let mut previous_team = None;
                for p in sorted_players {
                    if previous_team != Some(p.team) {
                        if previous_team.is_some() {
                            ui.add_space(4.0 * scale);
                        }
                        render_team_header(ui, p.team, scale);
                        ui.add_space(2.0 * scale);
                        previous_team = Some(p.team);
                    }

                    render_lobby_player_row(
                        ui,
                        p,
                        config,
                        scale,
                        content_width,
                        local_identity,
                        local_mmr,
                        player_count,
                        session_mode,
                        history_summaries,
                    );
                    ui.add_space(2.0 * scale);
                }
            }
        });
    });
}

fn render_team_header(ui: &mut egui::Ui, team: u8, scale: f32) {
    let color = team_color(team);
    let label = team_label(team);
    ui.horizontal(|ui| {
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(18.0 * scale, 2.0 * scale), egui::Sense::hover());
        ui.painter().rect_filled(rect, 1.0 * scale, color);
        ui.label(
            egui::RichText::new(label)
                .size(8.5 * scale)
                .color(color)
                .strong(),
        );
    });
}

#[allow(clippy::too_many_arguments)]
fn render_lobby_player_row(
    ui: &mut egui::Ui,
    player: &PlayerInfo,
    config: &crate::state::Config,
    scale: f32,
    content_width: f32,
    local_identity: Option<&LocalPlayerIdentity>,
    local_mmr: Option<&TrackerSnapshot>,
    player_count: usize,
    session_mode: SessionMode,
    history_summaries: Option<&HashMap<String, crate::history::PlayerHistorySummary>>,
) {
    let team_color = team_color(player.team);
    let is_local = is_local_lobby_player(player, local_identity, player_count);
    let mmr = player
        .mmr
        .as_ref()
        .or_else(|| is_local.then_some(local_mmr).flatten());
    let name_color = if is_local {
        egui::Color32::from_rgb(230, 255, 245)
    } else if player.is_bot {
        egui::Color32::from_gray(140)
    } else {
        egui::Color32::WHITE
    };

    if config.lobby_display_mode == crate::state::LobbyDisplayMode::Compact {
        render_compact_row(
            ui,
            player,
            mmr,
            is_local,
            name_color,
            config,
            scale,
            content_width,
            team_color,
            player_count,
            session_mode,
            history_summaries,
        );
    } else {
        render_expanded_row_v2(
            ui,
            player,
            mmr,
            is_local,
            name_color,
            config,
            scale,
            content_width,
            team_color,
            player_count,
            session_mode,
            history_summaries,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_compact_row(
    ui: &mut egui::Ui,
    player: &PlayerInfo,
    mmr: Option<&TrackerSnapshot>,
    is_local: bool,
    name_color: egui::Color32,
    config: &crate::state::Config,
    scale: f32,
    content_width: f32,
    team_color: egui::Color32,
    player_count: usize,
    session_mode: SessionMode,
    history_summaries: Option<&HashMap<String, crate::history::PlayerHistorySummary>>,
) {
    let accent_width = 3.0 * scale;
    let row_height = 18.0 * scale;
    let gap = 8.0 * scale;

    ui.horizontal(|ui| {
        ui.set_width(content_width);

        // 1. Accent Bar
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(accent_width, row_height), egui::Sense::hover());
        ui.painter().rect_filled(rect, 1.5 * scale, team_color);
        ui.add_space(gap);

        // 2. Left group: Name, YOU, MMR Badge
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0 * scale;

            // Player Name
            ui.label(
                egui::RichText::new(&player.name)
                    .color(name_color)
                    .size(11.0 * scale)
                    .strong(),
            );

            if is_local {
                render_you_badge(ui, scale);
            }

            if let (true, Some(playlist)) = (
                config.show_lobby_ranks,
                select_lobby_playlist(mmr, session_mode, player_count),
            ) {
                render_mmr_badge(ui, &playlist.tier_name, playlist.rating, false, scale);
            }

            if let Some(summary) = history_summary_for_player(player, history_summaries)
                && !is_local
            {
                render_history_badge(ui, summary, scale);
            }
        });

        // 3. Right group
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0 * scale;

            // Boost percentage
            if config.show_stats {
                let boost = egui::RichText::new(format!("{}%", player.boost))
                    .size(10.0 * scale)
                    .color(boost_color(player.boost))
                    .strong();
                ui.label(boost);
            }

            // Platform Icon
            ui.add(
                egui::Image::new(platform_icon(player))
                    .max_width(9.0 * scale)
                    .maintain_aspect_ratio(true),
            );

            // Platform Name
            render_platform_name(ui, &player.platform, 8.5 * scale);
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn render_expanded_row_v2(
    ui: &mut egui::Ui,
    player: &PlayerInfo,
    mmr: Option<&TrackerSnapshot>,
    is_local: bool,
    name_color: egui::Color32,
    config: &crate::state::Config,
    scale: f32,
    content_width: f32,
    team_color: egui::Color32,
    player_count: usize,
    session_mode: SessionMode,
    history_summaries: Option<&HashMap<String, crate::history::PlayerHistorySummary>>,
) {
    let accent_width = 3.0 * scale;
    let accent_height = 32.0 * scale;
    let gap = 8.0 * scale;

    let platform_width = 82.0 * scale;
    let stats_width = 76.0 * scale;
    let total_fixed_width = accent_width + (gap * 3.0) + platform_width + stats_width;
    let identity_width = (content_width - total_fixed_width).max(130.0 * scale);

    ui.horizontal(|ui| {
        ui.set_width(content_width);

        // 1. Accent Bar
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(accent_width, accent_height),
            egui::Sense::hover(),
        );
        ui.painter().rect_filled(rect, 1.5 * scale, team_color);
        ui.add_space(gap);

        // 2. Identity Column (Name on line 1, Rank & MMR Badge on line 2)
        ui.allocate_ui_with_layout(
            egui::vec2(identity_width, accent_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(identity_width);
                ui.spacing_mut().item_spacing.y = 2.0 * scale;
                // Line 1: Player Name + YOU badge
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0 * scale;
                    ui.label(
                        egui::RichText::new(&player.name)
                            .color(name_color)
                            .size(12.0 * scale)
                            .strong(),
                    );
                    if is_local {
                        render_you_badge(ui, scale);
                    }
                    if let Some(summary) = history_summary_for_player(player, history_summaries)
                        && !is_local
                    {
                        render_history_badge(ui, summary, scale);
                    }
                });

                // Line 2: MMR Badge + Optional Matches Played
                if config.show_lobby_ranks {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0 * scale;
                        let playlist = select_lobby_playlist(mmr, session_mode, player_count);
                        let show_matches =
                            config.show_lobby_matches && playlist.is_some_and(|p| p.matches > 0);

                        if let Some(pl) = playlist {
                            render_mmr_badge(ui, &pl.tier_name, pl.rating, true, scale);
                            if show_matches {
                                ui.label(
                                    egui::RichText::new(format!("{} Games", pl.matches))
                                        .size(7.0 * scale)
                                        .color(egui::Color32::from_gray(120)),
                                );
                            }
                        } else if should_fetch_rank(player) {
                            ui.label(
                                egui::RichText::new("Fetching rank...")
                                    .color(egui::Color32::from_gray(120))
                                    .size(8.5 * scale),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("-")
                                    .color(egui::Color32::from_gray(120))
                                    .size(8.5 * scale),
                            );
                        }
                    });
                }
            },
        );

        ui.add_space(gap);

        // 3. Platform Column (Centered Column)
        ui.allocate_ui_with_layout(
            egui::vec2(platform_width, accent_height),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                ui.set_width(platform_width);
                ui.add_space(10.0 * scale);

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0 * scale;
                    ui.add(
                        egui::Image::new(platform_icon(player))
                            .max_width(9.0 * scale)
                            .maintain_aspect_ratio(true),
                    );
                    render_platform_name(ui, &player.platform, 8.5 * scale);
                });
            },
        );

        ui.add_space(gap);

        // 4. Stats Column (Right side, right-aligned)
        ui.allocate_ui_with_layout(
            egui::vec2(stats_width, accent_height),
            egui::Layout::top_down(egui::Align::Max),
            |ui| {
                ui.set_width(stats_width);
                ui.spacing_mut().item_spacing.y = 2.0 * scale;
                // Line 1: Boost %
                let boost = egui::RichText::new(format!("{}%", player.boost))
                    .size(10.0 * scale)
                    .color(boost_color(player.boost))
                    .strong();
                ui.label(boost);

                // Line 2: Touches | Bumps | Demos
                if config.show_stats {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} TCH | {} BMP | {} DEM",
                            player.touches, player.car_touches, player.demos
                        ))
                        .size(7.0 * scale)
                        .color(egui::Color32::from_gray(120)),
                    );
                }
            },
        );
    });
}

fn render_you_badge(ui: &mut egui::Ui, scale: f32) {
    ui.label(
        egui::RichText::new("YOU")
            .size(7.0 * scale)
            .color(egui::Color32::from_rgb(0, 255, 150))
            .strong(),
    );
}

fn history_summary_for_player<'a>(
    player: &PlayerInfo,
    history_summaries: Option<&'a HashMap<String, crate::history::PlayerHistorySummary>>,
) -> Option<&'a crate::history::PlayerHistorySummary> {
    let key = crate::history::player_key(player)?;
    history_summaries?.get(key.as_str())
}

fn render_history_badge(
    ui: &mut egui::Ui,
    summary: &crate::history::PlayerHistorySummary,
    scale: f32,
) {
    if summary.total_games() == 0 {
        return;
    }

    ui.label(
        egui::RichText::new(format!(
            "Seen {} With {} Vs {}",
            summary.total_games(),
            summary.games_with,
            summary.games_against
        ))
        .size(7.0 * scale)
        .color(egui::Color32::from_gray(145)),
    );
}

pub(super) fn render_platform_name(ui: &mut egui::Ui, platform: &str, size: f32) {
    let normalized = crate::stats_api_parser::format_platform(platform);
    let plat_lower = normalized.to_lowercase();
    if plat_lower.contains("epic") {
        let mut job = egui::text::LayoutJob::default();
        let colors = [
            egui::Color32::from_rgb(255, 90, 90),   // Red
            egui::Color32::from_rgb(255, 170, 70),  // Orange
            egui::Color32::from_rgb(240, 220, 50),  // Yellow
            egui::Color32::from_rgb(70, 230, 90),   // Green
            egui::Color32::from_rgb(70, 180, 255),  // Blue
            egui::Color32::from_rgb(180, 100, 255), // Purple
        ];
        for (i, c) in normalized.chars().enumerate() {
            let color = colors[i % colors.len()];
            job.append(
                &c.to_string(),
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::proportional(size),
                    color,
                    ..Default::default()
                },
            );
        }
        ui.label(job);
    } else {
        let color = if plat_lower.contains("xbox") || plat_lower.contains("xbl") {
            egui::Color32::from_rgb(30, 200, 80) // Xbox Green
        } else if plat_lower.contains("playstation") || plat_lower.contains("ps") {
            egui::Color32::from_rgb(41, 140, 255) // PSN Blue
        } else if plat_lower.contains("switch") || plat_lower.contains("nintendo") {
            egui::Color32::from_rgb(255, 65, 80) // Switch Red
        } else {
            egui::Color32::from_gray(160) // Steam / Default stays gray
        };

        ui.label(egui::RichText::new(normalized).size(size).color(color));
    }
}

fn render_rank_icon(ui: &mut egui::Ui, icon: egui::ImageSource<'static>, size: f32) {
    ui.add(
        egui::Image::new(icon)
            .max_size(egui::vec2(size, size))
            .maintain_aspect_ratio(true),
    );
}

fn render_mmr_badge(ui: &mut egui::Ui, rank: &str, rating: i32, show_rank_name: bool, scale: f32) {
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgba_unmultiplied(8, 10, 14, 180))
        .stroke(egui::Stroke::new(
            0.7 * scale,
            egui::Color32::from_rgba_unmultiplied(180, 200, 255, 80),
        ))
        .corner_radius(4.0 * scale)
        .inner_margin(egui::Margin::symmetric(
            (4.0 * scale).round() as i8,
            (2.0 * scale).round() as i8,
        ));

    frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0 * scale;
            if let Some(icon) = rank_icon(rank) {
                render_rank_icon(ui, icon, 12.0 * scale);
            }
            if show_rank_name {
                ui.label(
                    egui::RichText::new(rank)
                        .size(7.5 * scale)
                        .color(egui::Color32::from_rgb(180, 200, 255)),
                );
            }
            ui.label(
                egui::RichText::new(mmr_badge_rating_text(rank, rating))
                    .size(8.5 * scale)
                    .color(egui::Color32::WHITE)
                    .strong(),
            );
        });
    });
}

fn mmr_badge_rating_text(rank: &str, rating: i32) -> String {
    if rank.trim().eq_ignore_ascii_case("unranked") {
        format!("MMR {rating}")
    } else {
        rating.to_string()
    }
}

#[allow(dead_code)]
fn compact_player_text(player: &PlayerInfo) -> String {
    if should_fetch_rank(player) {
        format!("{} (Fetching...)", player.name)
    } else {
        player.name.clone()
    }
}

pub(super) fn select_lobby_playlist(
    mmr: Option<&TrackerSnapshot>,
    session_mode: SessionMode,
    player_count: usize,
) -> Option<&crate::mmr::TrackerPlaylistSnapshot> {
    let snapshot = mmr?;

    if let Some(target_id) =
        lobby_playlist_id(session_mode).or_else(|| playlist_id_from_player_count(player_count))
        && let Some(playlist) = snapshot
            .playlists
            .get(&target_id)
            .filter(|p| !p.tier_name.is_empty())
    {
        return Some(playlist);
    }

    // Fallback: highest rating playlist
    best_playlist(mmr)
}

fn lobby_playlist_id(session_mode: SessionMode) -> Option<i32> {
    match session_mode {
        SessionMode::Ones => Some(10),
        SessionMode::Twos => Some(11),
        SessionMode::Threes => Some(13),
        SessionMode::Hoops => Some(27),
        SessionMode::Dropshot => Some(29),
        SessionMode::Snowday => Some(30),
        SessionMode::Knockout | SessionMode::Freeplay | SessionMode::Unknown => None,
    }
}

fn playlist_id_from_player_count(player_count: usize) -> Option<i32> {
    match player_count {
        0 | 1 => None,
        2 => Some(10),
        3 | 4 => Some(11),
        _ => Some(13),
    }
}

fn best_playlist(mmr: Option<&TrackerSnapshot>) -> Option<&crate::mmr::TrackerPlaylistSnapshot> {
    mmr?.playlists
        .iter()
        .filter(|entry| *entry.0 != 0 && !entry.1.tier_name.is_empty())
        .map(|entry| entry.1)
        .max_by_key(|playlist| playlist.rating)
}

#[allow(dead_code)]
fn best_rank(mmr: Option<&TrackerSnapshot>) -> Option<(String, i32)> {
    let playlist = best_playlist(mmr)?;
    Some((playlist.tier_name.clone(), playlist.rating))
}

pub(super) fn rank_icon(rank: &str) -> Option<egui::ImageSource<'static>> {
    match rank.trim().to_lowercase().as_str() {
        "bronze i" => Some(egui::include_image!("../../assets/ranks/bronze_1.png")),
        "bronze ii" => Some(egui::include_image!("../../assets/ranks/bronze_2.png")),
        "bronze iii" => Some(egui::include_image!("../../assets/ranks/bronze_3.png")),
        "silver i" => Some(egui::include_image!("../../assets/ranks/silver_1.png")),
        "silver ii" => Some(egui::include_image!("../../assets/ranks/silver_2.png")),
        "silver iii" => Some(egui::include_image!("../../assets/ranks/silver_3.png")),
        "gold i" => Some(egui::include_image!("../../assets/ranks/gold_1.png")),
        "gold ii" => Some(egui::include_image!("../../assets/ranks/gold_2.png")),
        "gold iii" => Some(egui::include_image!("../../assets/ranks/gold_3.png")),
        "platinum i" => Some(egui::include_image!("../../assets/ranks/platinum_1.png")),
        "platinum ii" => Some(egui::include_image!("../../assets/ranks/platinum_2.png")),
        "platinum iii" => Some(egui::include_image!("../../assets/ranks/platinum_3.png")),
        "diamond i" => Some(egui::include_image!("../../assets/ranks/diamond_1.png")),
        "diamond ii" => Some(egui::include_image!("../../assets/ranks/diamond_2.png")),
        "diamond iii" => Some(egui::include_image!("../../assets/ranks/diamond_3.png")),
        "champion i" => Some(egui::include_image!("../../assets/ranks/champion_1.png")),
        "champion ii" => Some(egui::include_image!("../../assets/ranks/champion_2.png")),
        "champion iii" => Some(egui::include_image!("../../assets/ranks/champion_3.png")),
        "grand champion i" => Some(egui::include_image!(
            "../../assets/ranks/grand_champion_1.png"
        )),
        "grand champion ii" => Some(egui::include_image!(
            "../../assets/ranks/grand_champion_2.png"
        )),
        "grand champion iii" => Some(egui::include_image!(
            "../../assets/ranks/grand_champion_3.png"
        )),
        "supersonic legend" => Some(egui::include_image!(
            "../../assets/ranks/supersonic_legend.png"
        )),
        "unranked" => Some(egui::include_image!("../../assets/ranks/unranked.png")),
        _ => None,
    }
}

fn should_fetch_rank(player: &PlayerInfo) -> bool {
    !player.is_local
        && !player.is_bot
        && !player.platform.eq_ignore_ascii_case("bot")
        && !player.platform.eq_ignore_ascii_case("unknown")
}

fn is_local_lobby_player(
    player: &PlayerInfo,
    local_identity: Option<&LocalPlayerIdentity>,
    player_count: usize,
) -> bool {
    if player.is_local || (player_count == 1 && !player.is_bot) {
        return true;
    }
    let Some(identity) = local_identity else {
        return false;
    };
    if !identity.is_known() {
        return false;
    }
    let same_account = !player.primary_id.trim().is_empty()
        && !player.platform.trim().is_empty()
        && identity.primary_id.eq_ignore_ascii_case(&player.primary_id)
        && identity.platform.eq_ignore_ascii_case(&player.platform);
    same_account || player.name.eq_ignore_ascii_case(&identity.name)
}

fn team_color(team: u8) -> egui::Color32 {
    if team == 0 {
        egui::Color32::from_rgb(0, 212, 255)
    } else {
        egui::Color32::from_rgb(255, 140, 0)
    }
}

fn team_label(team: u8) -> &'static str {
    if team == 0 { "BLUE" } else { "ORANGE" }
}

fn boost_color(boost: u8) -> egui::Color32 {
    if boost > 50 {
        egui::Color32::from_rgb(255, 255, 100)
    } else {
        egui::Color32::from_rgb(255, 150, 50)
    }
}

pub(super) fn platform_icon(player: &PlayerInfo) -> egui::ImageSource<'static> {
    platform_icon_for(&player.platform, player.is_bot)
}

pub(super) fn platform_icon_for(platform: &str, is_bot: bool) -> egui::ImageSource<'static> {
    if is_bot {
        return egui::include_image!("../../assets/bot.png");
    }
    let plat = platform.to_lowercase();
    if plat.contains("steam") {
        egui::include_image!("../../assets/steam.png")
    } else if plat.contains("epic") {
        egui::include_image!("../../assets/epic.png")
    } else if plat.contains("xbox") || plat.contains("xbl") {
        egui::include_image!("../../assets/xbox.png")
    } else if plat.contains("playstation") || plat.contains("ps") {
        egui::include_image!("../../assets/ps.png")
    } else if plat.contains("switch") || plat.contains("nintendo") {
        egui::include_image!("../../assets/switch.png")
    } else {
        egui::include_image!("../../assets/bot.png")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> LocalPlayerIdentity {
        LocalPlayerIdentity {
            name: "CachedName".to_string(),
            primary_id: "Steam|123|0".to_string(),
            platform: "Steam".to_string(),
        }
    }

    fn player(name: &str, primary_id: &str, platform: &str) -> PlayerInfo {
        PlayerInfo {
            name: name.to_string(),
            primary_id: primary_id.to_string(),
            platform: platform.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn local_flag_marks_lobby_player_local() {
        let player = PlayerInfo {
            is_local: true,
            ..player("Someone", "Epic|2|0", "Epic")
        };

        assert!(is_local_lobby_player(&player, None, 2));
    }

    #[test]
    fn cached_identity_marks_matching_account_local() {
        let identity = identity();
        let player = player("Renamed", "steam|123|0", "steam");

        assert!(is_local_lobby_player(&player, Some(&identity), 2));
    }

    #[test]
    fn cached_identity_falls_back_to_matching_name() {
        let identity = identity();
        let player = player("cachedname", "Unknown|0|0", "Unknown");

        assert!(is_local_lobby_player(&player, Some(&identity), 2));
    }

    #[test]
    fn cached_identity_does_not_mark_different_account_local() {
        let identity = identity();
        let player = player("Opponent", "Steam|999|0", "Steam");

        assert!(!is_local_lobby_player(&player, Some(&identity), 2));
    }

    #[test]
    fn single_player_lobby_marks_player_local() {
        let p = player("Player", "Unknown|0|0", "Unknown");
        assert!(is_local_lobby_player(&p, None, 1));

        let bot = PlayerInfo {
            is_bot: true,
            ..player("Bot", "Unknown|0|0", "Unknown")
        };
        assert!(!is_local_lobby_player(&bot, None, 1));
    }

    #[test]
    fn best_rank_extracts_local_cached_mmr_for_badge() {
        let mut playlists = HashMap::new();
        playlists.insert(
            11,
            crate::mmr::TrackerPlaylistSnapshot {
                name: "Ranked Doubles 2v2".to_string(),
                rating: 1234,
                matches: 20,
                tier_name: "Champion II".to_string(),
            },
        );
        let mmr = TrackerSnapshot {
            playlists,
            last_updated: None,
            current_season: None,
        };
        let player = PlayerInfo {
            name: "CachedName".to_string(),
            is_local: true,
            ..Default::default()
        };

        assert_eq!(compact_player_text(&player), "CachedName");
        assert_eq!(
            best_rank(Some(&mmr)),
            Some(("Champion II".to_string(), 1234))
        );
    }

    #[test]
    fn preview_uses_cached_local_identity_and_mmr() {
        let state = AppState::new();
        state.update_local_player_identity(identity());
        let mut playlists = HashMap::new();
        playlists.insert(
            11,
            crate::mmr::TrackerPlaylistSnapshot {
                name: "Ranked Doubles 2v2".to_string(),
                rating: 1420,
                matches: 40,
                tier_name: "Grand Champion I".to_string(),
            },
        );
        state
            .mmr
            .local_mmr
            .store(Arc::new(crate::state::LocalMmrState {
                current: Some(TrackerSnapshot {
                    playlists,
                    last_updated: None,
                    current_season: None,
                }),
                ..Default::default()
            }));

        let preview = preview_lobby_players(&state);
        let local = preview.iter().find(|player| player.is_local).unwrap();

        assert_eq!(local.name, "CachedName");
        assert_eq!(local.primary_id, "Steam|123|0");
        assert_eq!(local.platform, "Steam");
        assert_eq!(
            best_rank(local.mmr.as_ref()),
            Some(("Grand Champion I".to_string(), 1420))
        );
    }

    #[test]
    fn preview_uses_session_local_team() {
        let state = AppState::new();
        let mut session = (**state.game.session.load()).clone();
        session.local_team = Some(1);
        state.game.session.store(Arc::new(session));

        let preview = preview_lobby_players(&state);
        let local = preview.iter().find(|player| player.is_local).unwrap();
        let opponent = preview.iter().find(|player| !player.is_local).unwrap();

        assert_eq!(local.team, 1);
        assert_eq!(opponent.team, 0);
    }

    #[test]
    fn rank_icon_maps_current_rank_names() {
        assert!(rank_icon("Champion II").is_some());
        assert!(rank_icon("Grand Champion III").is_some());
        assert!(rank_icon("Supersonic Legend").is_some());
        assert!(rank_icon("Prospect I").is_none());
    }

    #[test]
    fn unranked_mmr_badge_labels_rating_as_mmr() {
        assert_eq!(mmr_badge_rating_text("Unranked", 944), "MMR 944");
        assert_eq!(mmr_badge_rating_text("Diamond II", 944), "944");
    }

    #[test]
    fn test_select_lobby_playlist_modes_and_fallbacks() {
        let mut playlists = HashMap::new();
        // 1v1
        playlists.insert(
            10,
            crate::mmr::TrackerPlaylistSnapshot {
                name: "Ranked Duel 1v1".to_string(),
                rating: 800,
                matches: 10,
                tier_name: "Gold III".to_string(),
            },
        );
        // 2v2
        playlists.insert(
            11,
            crate::mmr::TrackerPlaylistSnapshot {
                name: "Ranked Doubles 2v2".to_string(),
                rating: 1200,
                matches: 15,
                tier_name: "Diamond II".to_string(),
            },
        );
        // Hoops
        playlists.insert(
            27,
            crate::mmr::TrackerPlaylistSnapshot {
                name: "Ranked Hoops".to_string(),
                rating: 950,
                matches: 20,
                tier_name: "Platinum III".to_string(),
            },
        );
        // Dropshot
        playlists.insert(
            29,
            crate::mmr::TrackerPlaylistSnapshot {
                name: "Ranked Dropshot".to_string(),
                rating: 875,
                matches: 8,
                tier_name: "Platinum I".to_string(),
            },
        );
        // Casual (ID 0) with a higher rating
        playlists.insert(
            0,
            crate::mmr::TrackerPlaylistSnapshot {
                name: "Un-Ranked".to_string(),
                rating: 2000,
                matches: 50,
                tier_name: "Unranked".to_string(),
            },
        );

        let mmr = TrackerSnapshot {
            playlists,
            last_updated: None,
            current_season: None,
        };

        // 1. Should select 1v1 for 2 total players.
        let pl_1v1 = select_lobby_playlist(Some(&mmr), SessionMode::Unknown, 2).unwrap();
        assert_eq!(pl_1v1.name, "Ranked Duel 1v1");
        assert_eq!(pl_1v1.rating, 800);

        // 2. Should select 2v2 for 4 total players, including bot exhibitions.
        let pl_2v2 = select_lobby_playlist(Some(&mmr), SessionMode::Unknown, 4).unwrap();
        assert_eq!(pl_2v2.name, "Ranked Doubles 2v2");
        assert_eq!(pl_2v2.rating, 1200);

        // 3. Should select Hoops from inferred map mode even though 4 humans would normally mean 2v2.
        let pl_hoops = select_lobby_playlist(Some(&mmr), SessionMode::Hoops, 4).unwrap();
        assert_eq!(pl_hoops.name, "Ranked Hoops");
        assert_eq!(pl_hoops.rating, 950);

        // 4. Should select Dropshot from inferred map mode.
        let pl_dropshot = select_lobby_playlist(Some(&mmr), SessionMode::Dropshot, 6).unwrap();
        assert_eq!(pl_dropshot.name, "Ranked Dropshot");
        assert_eq!(pl_dropshot.rating, 875);

        // 5. Should fallback to best playlist (2v2 with 1200 rating) for 3v3 (player_count >= 5) because 3v3 is missing
        // (Even though Casual playlist 0 has a higher rating of 2000, it must be ignored as a fallback for lobby badges)
        let pl_fallback = select_lobby_playlist(Some(&mmr), SessionMode::Unknown, 6).unwrap();
        assert_eq!(pl_fallback.name, "Ranked Doubles 2v2");
        assert_eq!(pl_fallback.rating, 1200);

        // 6. One total player is not enough to infer a current match playlist.
        let pl_single_player = select_lobby_playlist(Some(&mmr), SessionMode::Unknown, 1).unwrap();
        assert_eq!(pl_single_player.name, "Ranked Doubles 2v2");
        assert_eq!(pl_single_player.rating, 1200);
    }
}
