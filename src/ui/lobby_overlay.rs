use crate::state::{AnchorPos, AppState, PlayerInfo};
use eframe::egui;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::layout::{
    active_layout_drag_position, normalized_to_pos, persist_dragged_position,
    render_drag_position_handle,
};

pub(super) fn preview_lobby_players(state: &Arc<AppState>) -> Vec<PlayerInfo> {
    let players = state.players.load();
    let mut lobby_players: Vec<PlayerInfo> = players.values().cloned().collect();

    if lobby_players.is_empty() {
        let mut playlists_local = HashMap::new();
        playlists_local.insert(11, crate::mmr::TrackerPlaylistSnapshot {
            name: "Ranked Doubles 2v2".to_string(),
            rating: 1150,
            matches: 120,
            tier_name: "Champion I".to_string(),
        });

        let mut playlists_opp = HashMap::new();
        playlists_opp.insert(11, crate::mmr::TrackerPlaylistSnapshot {
            name: "Ranked Doubles 2v2".to_string(),
            rating: 1045,
            matches: 85,
            tier_name: "Diamond II".to_string(),
        });

        lobby_players = vec![
            PlayerInfo {
                name: "You (Local)".to_string(),
                team: 0,
                is_bot: false,
                is_local: true,
                platform: "steam".to_string(),
                boost: 100,
                score: 350,
                goals: 1,
                saves: 1,
                mmr: Some(crate::mmr::TrackerSnapshot {
                    playlists: playlists_local,
                    last_updated: None,
                    current_season: None,
                }),
                ..Default::default()
            },
            PlayerInfo {
                name: "OpponentOne".to_string(),
                team: 1,
                is_bot: false,
                is_local: false,
                platform: "epic".to_string(),
                boost: 45,
                score: 210,
                goals: 0,
                saves: 2,
                mmr: Some(crate::mmr::TrackerSnapshot {
                    playlists: playlists_opp,
                    last_updated: None,
                    current_season: None,
                }),
                ..Default::default()
            },
        ];
    }

    lobby_players
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
    let config = state.config.load();
    let players = state.players.load();

    let (anchor, base_offset) = match config.anchor {
        AnchorPos::TopLeft => (egui::Align2::LEFT_TOP, egui::vec2(20.0, 20.0)),
        AnchorPos::TopRight => (egui::Align2::RIGHT_TOP, egui::vec2(-20.0, 20.0)),
        AnchorPos::BottomLeft => (egui::Align2::LEFT_BOTTOM, egui::vec2(20.0, -20.0)),
        AnchorPos::BottomRight => (egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0, -20.0)),
        AnchorPos::CenterRight => (egui::Align2::RIGHT_CENTER, egui::vec2(-20.0, 0.0)),
    };

    let offset = (base_offset + egui::vec2(config.lobby_offset[0], config.lobby_offset[1])) * config.ui_scale;

    let area = egui::Area::new("overlay_area".into()).order(egui::Order::Foreground);
    let area = if let Some(position) = active_layout_drag_position(ctx, "lobby") {
        area.fixed_pos(position)
    } else if let Some(position) = config.lobby_manual_position {
        area.fixed_pos(normalized_to_pos(ctx, position))
    } else {
        area.anchor(anchor, offset)
    };

    let area_response = area.show(ctx, |ui| {
        let players_vec: Vec<PlayerInfo> = players.values().cloned().collect();
        draw_lobby_panel(
            ui,
            &players_vec,
            &config,
            state.is_connected.load(Ordering::SeqCst),
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

pub(super) fn draw_lobby_panel(
    ui: &mut egui::Ui,
    players: &[PlayerInfo],
    config: &crate::state::Config,
    is_connected: bool,
    scale_override: Option<f32>,
) {
    let scale = scale_override.unwrap_or(config.ui_scale);
    let (fill, stroke) = match config.lobby_theme {
        crate::state::LobbyTheme::Glass => (
            egui::Color32::from_rgba_unmultiplied(20, 20, 25, config.transparency),
            egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 20)),
        ),
        crate::state::LobbyTheme::Solid => (
            egui::Color32::from_rgba_unmultiplied(10, 10, 12, 255),
            egui::Stroke::new(1.0, egui::Color32::from_gray(50)),
        ),
        crate::state::LobbyTheme::Modern => (
            egui::Color32::from_rgba_unmultiplied(12, 14, 18, config.transparency.max(220)),
            egui::Stroke::new(1.2, egui::Color32::from_rgba_unmultiplied(0, 176, 255, 140)),
        ),
        crate::state::LobbyTheme::Minimalist => (
            egui::Color32::TRANSPARENT,
            egui::Stroke::NONE,
        ),
    };

    let frame = egui::Frame::default()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(6.0 * scale)
        .inner_margin(8.0 * scale);

    frame.show(ui, |ui| {
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
                });
            });

            ui.add_space(4.0 * scale);

            let mut sorted_players: Vec<_> = players
                .iter()
                .filter(|p| config.show_bots || !p.is_bot)
                .collect();
            sorted_players
                .sort_by(|a, b| a.team.cmp(&b.team).then_with(|| a.name.cmp(&b.name)));

            if sorted_players.is_empty() {
                ui.label(
                    egui::RichText::new("Waiting...")
                        .size(11.0 * scale)
                        .italics()
                        .color(egui::Color32::from_gray(120)),
                );
            } else {
                for p in sorted_players {
                    let team_color = if p.team == 0 {
                        egui::Color32::from_rgb(0, 212, 255)
                      } else {
                        egui::Color32::from_rgb(255, 140, 0)
                      };

                    ui.horizontal(|ui| {
                        // Vertical Team Accent
                        let accent_height = match config.lobby_display_mode {
                            crate::state::LobbyDisplayMode::Compact => 11.0 * scale,
                            crate::state::LobbyDisplayMode::Expanded => 14.0 * scale,
                        };
                        let (rect, _) = ui.allocate_at_least(
                            egui::vec2(2.5 * scale, accent_height),
                            egui::Sense::hover(),
                        );
                        ui.painter()
                            .rect_filled(rect, 1.5 * scale, team_color);

                        ui.add_space(4.0 * scale);

                        // Player Name and MMR
                        let name_color = if p.is_bot {
                            egui::Color32::from_gray(140)
                        } else {
                            egui::Color32::WHITE
                        };

                        if config.lobby_display_mode == crate::state::LobbyDisplayMode::Compact {
                            let mut text = p.name.clone();
                            if let Some(snapshot) = &p.mmr {
                                let mut mmr_val = 0;
                                if let Some(playlist) = snapshot
                                    .playlists
                                    .values()
                                    .filter(|p| !p.tier_name.is_empty())
                                    .max_by_key(|p| p.rating)
                                {
                                    mmr_val = playlist.rating;
                                }
                                text = format!("{} ({} MMR)", p.name, mmr_val);
                            } else if !p.is_local
                                && !p.is_bot
                                && p.platform.to_lowercase() != "bot"
                                && p.platform.to_lowercase() != "unknown"
                            {
                                text = format!("{} (Fetching...)", p.name);
                            }

                            ui.label(
                                egui::RichText::new(text)
                                    .color(name_color)
                                    .size(11.0 * scale)
                                    .strong(),
                            );
                        } else {
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new(&p.name)
                                        .color(name_color)
                                        .size(12.0 * scale)
                                        .strong(),
                                );

                                // Render MMR if available
                                if let Some(snapshot) = &p.mmr {
                                    let mut display_rank = "Unranked".to_string();
                                    let mut mmr_val = 0;

                                    // Find highest ranked playlist
                                    if let Some(playlist) = snapshot
                                        .playlists
                                        .values()
                                        .filter(|p| !p.tier_name.is_empty())
                                        .max_by_key(|p| p.rating)
                                    {
                                        display_rank = playlist.tier_name.clone();
                                        mmr_val = playlist.rating;
                                    }

                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{} ({} MMR)",
                                            display_rank, mmr_val
                                        ))
                                        .color(egui::Color32::from_rgb(180, 200, 255))
                                        .size(8.5 * scale),
                                    );
                                } else if !p.is_local
                                    && !p.is_bot
                                    && p.platform.to_lowercase() != "bot"
                                    && p.platform.to_lowercase() != "unknown"
                                {
                                    ui.label(
                                        egui::RichText::new("Fetching rank...")
                                            .color(egui::Color32::from_gray(120))
                                            .size(8.5 * scale),
                                    );
                                }
                            });
                        }

                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                // Platform Icon on the right
                                let icon_source = if p.is_bot {
                                    egui::include_image!("../../assets/bot.png")
                                } else {
                                    let plat = p.platform.to_lowercase();
                                    if plat.contains("steam") {
                                        egui::include_image!("../../assets/steam.png")
                                    } else if plat.contains("epic") {
                                        egui::include_image!("../../assets/epic.png")
                                    } else if plat.contains("xbox") || plat.contains("xbl")
                                    {
                                        egui::include_image!("../../assets/xbox.png")
                                    } else if plat.contains("playstation")
                                        || plat.contains("ps")
                                    {
                                        egui::include_image!("../../assets/ps.png")
                                    } else if plat.contains("switch")
                                        || plat.contains("nintendo")
                                    {
                                        egui::include_image!("../../assets/switch.png")
                                    } else {
                                        egui::include_image!("../../assets/bot.png")
                                    }
                                };

                                ui.add(
                                    egui::Image::new(icon_source)
                                        .max_width(10.0 * scale)
                                        .maintain_aspect_ratio(true),
                                );

                                if config.lobby_display_mode == crate::state::LobbyDisplayMode::Expanded {
                                    ui.add_space(4.0 * scale);
                                    ui.label(
                                        egui::RichText::new(&p.platform)
                                            .size(8.5 * scale)
                                            .color(egui::Color32::from_gray(160)),
                                    );
                                }

                                if config.show_stats {
                                    ui.add_space(6.0 * scale);
                                    if config.lobby_display_mode == crate::state::LobbyDisplayMode::Compact {
                                        ui.label(
                                            egui::RichText::new(format!("{}%", p.boost))
                                                .size(10.0 * scale)
                                                .color(if p.boost > 50 {
                                                    egui::Color32::from_rgb(255, 255, 100)
                                                } else {
                                                    egui::Color32::from_rgb(255, 150, 50)
                                                })
                                                .strong(),
                                        );
                                    } else {
                                        ui.vertical(|ui| {
                                            ui.label(
                                                egui::RichText::new(format!("{}%", p.boost))
                                                    .size(10.0 * scale)
                                                    .color(if p.boost > 50 {
                                                        egui::Color32::from_rgb(255, 255, 100)
                                                    } else {
                                                        egui::Color32::from_rgb(255, 150, 50)
                                                    })
                                                    .strong(),
                                            );
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "S:{} G:{} Sv:{}",
                                                    p.score, p.goals, p.saves
                                                ))
                                                .size(7.0 * scale)
                                                .color(egui::Color32::from_gray(120)),
                                            );
                                        });
                                    }
                                }
                            },
                        );
                    });
                    ui.add_space(1.0 * scale);
                }
            }
        });
    });
}
