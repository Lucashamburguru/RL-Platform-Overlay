use crate::state::{AnchorPos, AppState};
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::layout::{
    active_layout_drag_position, normalized_to_pos, persist_dragged_position,
    render_drag_position_handle,
};

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

    let offset = base_offset * config.ui_scale;

    let area = egui::Area::new("overlay_area".into()).order(egui::Order::Foreground);
    let area = if let Some(position) = active_layout_drag_position(ctx, "lobby") {
        area.fixed_pos(position)
    } else if let Some(position) = config.lobby_manual_position {
        area.fixed_pos(normalized_to_pos(ctx, position))
    } else {
        area.anchor(anchor, offset)
    };

    let area_response = area.show(ctx, |ui| {
        let frame = egui::Frame::default()
            .fill(egui::Color32::from_rgba_unmultiplied(
                20,
                20,
                25,
                config.transparency,
            ))
            .stroke(egui::Stroke::new(
                1.0,
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, 20),
            ))
            .corner_radius(6.0 * config.ui_scale)
            .inner_margin(8.0 * config.ui_scale);

        frame
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("LOBBY")
                                .size(10.0 * config.ui_scale)
                                .color(egui::Color32::from_gray(180))
                                .strong(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let status_color = if state.is_connected.load(Ordering::SeqCst) {
                                egui::Color32::from_rgb(0, 255, 150)
                            } else {
                                egui::Color32::from_rgb(255, 80, 80)
                            };
                            ui.label(
                                egui::RichText::new("●")
                                    .color(status_color)
                                    .size(7.0 * config.ui_scale),
                            );
                        });
                    });
                    let drag_response =
                        render_drag_position_handle(ui, config.layout_mode, config.ui_scale);

                    ui.add_space(4.0 * config.ui_scale);

                    let mut sorted_players: Vec<_> = players
                        .values()
                        .filter(|p| config.show_bots || !p.is_bot)
                        .collect();
                    sorted_players
                        .sort_by(|a, b| a.team.cmp(&b.team).then_with(|| a.name.cmp(&b.name)));

                    if sorted_players.is_empty() {
                        ui.label(
                            egui::RichText::new("Waiting...")
                                .size(11.0 * config.ui_scale)
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
                                let (rect, _) = ui.allocate_at_least(
                                    egui::vec2(2.5 * config.ui_scale, 14.0 * config.ui_scale),
                                    egui::Sense::hover(),
                                );
                                ui.painter()
                                    .rect_filled(rect, 1.5 * config.ui_scale, team_color);

                                ui.add_space(4.0 * config.ui_scale);

                                // Player Name and MMR
                                ui.vertical(|ui| {
                                    let name_color = if p.is_bot {
                                        egui::Color32::from_gray(140)
                                    } else {
                                        egui::Color32::WHITE
                                    };
                                    ui.label(
                                        egui::RichText::new(&p.name)
                                            .color(name_color)
                                            .size(12.0 * config.ui_scale)
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
                                            .size(8.5 * config.ui_scale),
                                        );
                                    } else if !p.is_local
                                        && !p.is_bot
                                        && p.platform.to_lowercase() != "bot"
                                        && p.platform.to_lowercase() != "unknown"
                                    {
                                        ui.label(
                                            egui::RichText::new("Fetching rank...")
                                                .color(egui::Color32::from_gray(120))
                                                .size(8.5 * config.ui_scale),
                                        );
                                    }
                                });

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
                                                .max_width(10.0 * config.ui_scale)
                                                .maintain_aspect_ratio(true),
                                        );

                                        ui.add_space(4.0 * config.ui_scale);
                                        ui.label(
                                            egui::RichText::new(&p.platform)
                                                .size(8.5 * config.ui_scale)
                                                .color(egui::Color32::from_gray(160)),
                                        );

                                        if config.show_stats {
                                            ui.add_space(6.0 * config.ui_scale);
                                            ui.vertical(|ui| {
                                                ui.label(
                                                    egui::RichText::new(format!("{}%", p.boost))
                                                        .size(10.0 * config.ui_scale)
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
                                                    .size(7.0 * config.ui_scale)
                                                    .color(egui::Color32::from_gray(120)),
                                                );
                                            });
                                        }
                                    },
                                );
                            });
                            ui.add_space(1.0 * config.ui_scale);
                        }
                    }
                    drag_response
                })
                .inner
            })
            .inner
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
