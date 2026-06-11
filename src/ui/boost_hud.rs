use crate::state::{AppState, PlayerInfo, TeammateBoostDisplay};
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::layout::{
    active_layout_drag_position, normalized_to_pos, persist_dragged_position,
    render_drag_position_handle,
};

pub(super) fn teammate_boost_display_label(display: TeammateBoostDisplay) -> &'static str {
    match display {
        TeammateBoostDisplay::Bars => "Bars",
        TeammateBoostDisplay::Circles => "Circles",
        TeammateBoostDisplay::Compact => "Compact",
        TeammateBoostDisplay::Numbers => "Numbers",
    }
}

pub(super) fn preview_teammates(state: &Arc<AppState>) -> Vec<PlayerInfo> {
    let players = state.game.players.load();
    let local_name = state.game.local_player_name.load().trim().to_lowercase();
    let local_team = state.game.local_team.load(Ordering::SeqCst);
    let mut teammates: Vec<_> = players
        .values()
        .filter(|p| {
            local_team != crate::state::NO_TEAM
                && p.team == local_team
                && !p.is_local
                && (local_name.is_empty() || p.name.trim().to_lowercase() != local_name)
        })
        .cloned()
        .collect();

    if teammates.is_empty() {
        teammates = vec![
            PlayerInfo {
                name: "C-Block".to_string(),
                team: 0,
                boost: 18,
                is_bot: true,
                platform: "BOT".to_string(),
                ..Default::default()
            },
            PlayerInfo {
                name: "Caveman".to_string(),
                team: 0,
                boost: 72,
                is_bot: true,
                platform: "BOT".to_string(),
                ..Default::default()
            },
        ];
    }

    teammates.sort_by(|a, b| a.boost.cmp(&b.boost).then_with(|| a.name.cmp(&b.name)));
    teammates
}

pub(super) fn render_teammate_boost(ctx: &egui::Context, state: &Arc<AppState>) {
    let players = state.game.players.load();
    let local_name_raw = state.game.local_player_name.load();
    let local_name = local_name_raw.trim().to_lowercase();
    let config = state.system.config.load();

    // Find our team (preferring the stabilized local_team from state)
    // Do not guess if not found, because a bad fallback shows the wrong team.
    let my_team = {
        let stored_team = state.game.local_team.load(Ordering::SeqCst);
        if stored_team != crate::state::NO_TEAM {
            Some(stored_team)
        } else {
            players
                .values()
                .find(|p| {
                    p.is_local
                        || (!local_name.is_empty() && p.name.trim().to_lowercase() == local_name)
                })
                .map(|p| p.team)
        }
    };
    let Some(my_team) = my_team else {
        return;
    };

    // Find all teammates (excluding ourselves)
    let mut teammates: Vec<PlayerInfo> = players
        .values()
        .filter(|p| {
            p.team == my_team
                && !p.is_local
                && (local_name.is_empty() || p.name.trim().to_lowercase() != local_name)
        })
        .cloned()
        .collect();

    if teammates.is_empty() {
        return;
    }

    teammates.sort_by(|a, b| a.boost.cmp(&b.boost).then_with(|| a.name.cmp(&b.name)));

    let screen_rect = ctx.input(|i| i.screen_rect());
    let width = teammate_boost_width(config.teammate_hud_scale, config.teammate_boost_display);
    let height = teammate_boost_panel_height(
        teammates.len(),
        config.teammate_hud_scale,
        config.teammate_boost_display,
    );
    let default_offset_x = 110.0;
    let default_offset_y = 180.0;
    let base_x = screen_rect.max.x - default_offset_x * config.teammate_hud_scale - width;
    let base_y = screen_rect.max.y - default_offset_y * config.teammate_hud_scale - height;

    let position = active_layout_drag_position(ctx, "boost")
        .or_else(|| {
            config
                .teammate_boost_manual_position
                .map(|position| normalized_to_pos(ctx, position))
        })
        .unwrap_or_else(|| egui::pos2(base_x.max(0.0), base_y.max(0.0)));

    let response = egui::Area::new("teammate_boost_panel".into())
        .fixed_pos(position)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            draw_teammate_boost_panel(
                ui,
                &teammates,
                my_team,
                config.teammate_hud_scale,
                config.teammate_boost_display,
            );
            render_drag_position_handle(ui, config.layout_mode, config.teammate_hud_scale)
        });

    if let Some(drag_response) = response.inner {
        persist_dragged_position(
            ctx,
            state,
            response.response.rect.min,
            "boost",
            &drag_response,
        );
    }
}

pub(super) fn render_teammate_boost_position_preview(
    ctx: &egui::Context,
    state: &Arc<AppState>,
    draggable: bool,
) {
    let config = state.system.config.load();
    let teammates = preview_teammates(state);
    let screen_rect = ctx.input(|i| i.screen_rect());
    let scale = config.teammate_hud_scale;
    let width = teammate_boost_width(scale, config.teammate_boost_display);
    let height = teammate_boost_panel_height(teammates.len(), scale, config.teammate_boost_display);
    let default_offset_x = 110.0;
    let default_offset_y = 180.0;
    let base_x = screen_rect.max.x - default_offset_x * scale - width;
    let base_y = screen_rect.max.y - default_offset_y * scale - height;
    let position = active_layout_drag_position(ctx, "boost")
        .or_else(|| {
            config
                .teammate_boost_manual_position
                .map(|position| normalized_to_pos(ctx, position))
        })
        .unwrap_or_else(|| egui::pos2(base_x.max(0.0), base_y.max(0.0)));

    let response = egui::Area::new("teammate_boost_position_preview".into())
        .fixed_pos(position)
        .order(if draggable {
            egui::Order::Foreground
        } else {
            egui::Order::Background
        })
        .show(ctx, |ui| {
            ui.set_opacity(0.72);
            draw_teammate_boost_panel(ui, &teammates, 0, scale, config.teammate_boost_display);
            render_drag_position_handle(ui, draggable, scale)
        });

    if let Some(drag_response) = response.inner {
        persist_dragged_position(
            ctx,
            state,
            response.response.rect.min,
            "boost",
            &drag_response,
        );
    }
}

pub(super) fn draw_teammate_boost_panel(
    ui: &mut egui::Ui,
    teammates: &[PlayerInfo],
    my_team: u8,
    scale: f32,
    display: TeammateBoostDisplay,
) {
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_black_alpha(96))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(18)))
        .corner_radius(6.0 * scale)
        .inner_margin(5.0 * scale);

    frame.show(ui, |ui| {
        ui.set_min_width(teammate_boost_width(scale, display) - 10.0 * scale);
        for (index, player) in teammates.iter().enumerate() {
            draw_teammate_boost_row(ui, player, my_team, scale, display);
            if index + 1 < teammates.len() {
                ui.add_space(3.0 * scale);
            }
        }
    });
}

fn draw_teammate_boost_row(
    ui: &mut egui::Ui,
    player: &PlayerInfo,
    my_team: u8,
    scale: f32,
    display: TeammateBoostDisplay,
) {
    let row_size = egui::vec2(
        teammate_boost_width(scale, display) - 10.0 * scale,
        teammate_boost_row_height(scale, display),
    );
    let (rect, _) = ui.allocate_exact_size(row_size, egui::Sense::hover());
    let painter = ui.painter();
    let rounding = 4.0 * scale;
    let team_color = if my_team == 0 {
        egui::Color32::from_rgb(0, 176, 255)
    } else {
        egui::Color32::from_rgb(255, 132, 36)
    };

    let low_boost_alpha = if player.boost <= 20 {
        let pulse = (ui.input(|i| i.time) * 5.0).sin() as f32;
        (24.0 + 20.0 * ((pulse + 1.0) * 0.5)) as u8
    } else {
        0
    };

    painter.rect_filled(rect, rounding, egui::Color32::from_black_alpha(92));
    if low_boost_alpha > 0 {
        painter.rect_filled(
            rect,
            rounding,
            egui::Color32::from_rgba_unmultiplied(255, 40, 24, low_boost_alpha),
        );
    }

    let accent_rect = egui::Rect::from_min_max(
        rect.left_top(),
        egui::pos2(rect.left() + 3.0 * scale, rect.bottom()),
    );
    painter.rect_filled(accent_rect, rounding, team_color);

    match display {
        TeammateBoostDisplay::Bars => draw_teammate_boost_bar_content(ui, rect, player, scale),
        TeammateBoostDisplay::Circles => {
            draw_teammate_boost_circle_content(ui, rect, player, scale)
        }
        TeammateBoostDisplay::Compact => {
            draw_teammate_boost_compact_content(ui, rect, player, scale)
        }
        TeammateBoostDisplay::Numbers => {
            draw_teammate_boost_number_content(ui, rect, player, scale)
        }
    }
}

fn draw_teammate_boost_bar_content(
    ui: &egui::Ui,
    rect: egui::Rect,
    player: &PlayerInfo,
    scale: f32,
) {
    let painter = ui.painter();
    let boost_color = teammate_boost_color(player.boost);
    let inner = rect.shrink2(egui::vec2(8.0 * scale, 4.0 * scale));
    let value_width = 34.0 * scale;
    let bar_height = 5.0 * scale;
    let bar_rect = egui::Rect::from_min_max(
        egui::pos2(inner.left(), inner.bottom() - bar_height),
        egui::pos2(inner.right() - value_width - 8.0 * scale, inner.bottom()),
    );
    let fill_width = bar_rect.width() * (player.boost as f32 / 100.0).clamp(0.0, 1.0);
    let fill_rect =
        egui::Rect::from_min_size(bar_rect.left_top(), egui::vec2(fill_width, bar_height));

    painter.text(
        egui::pos2(inner.left(), inner.top() - 1.0 * scale),
        egui::Align2::LEFT_TOP,
        &player.name,
        egui::FontId::proportional(10.5 * scale),
        egui::Color32::from_gray(232),
    );

    painter.text(
        egui::pos2(inner.right(), inner.center().y - 1.0 * scale),
        egui::Align2::RIGHT_CENTER,
        format!("{:>3}", player.boost),
        egui::FontId::monospace(16.0 * scale),
        boost_color,
    );

    painter.rect_filled(bar_rect, 2.0 * scale, egui::Color32::from_white_alpha(32));
    painter.rect_filled(fill_rect, 2.0 * scale, boost_color);
}

fn draw_teammate_boost_circle_content(
    ui: &egui::Ui,
    rect: egui::Rect,
    player: &PlayerInfo,
    scale: f32,
) {
    let painter = ui.painter();
    let boost_color = teammate_boost_color(player.boost);
    let inner = rect.shrink2(egui::vec2(8.0 * scale, 4.0 * scale));
    let radius = 11.0 * scale;
    let center = egui::pos2(inner.right() - radius, inner.center().y);

    painter.text(
        egui::pos2(inner.left(), inner.center().y),
        egui::Align2::LEFT_CENTER,
        &player.name,
        egui::FontId::proportional(10.0 * scale),
        egui::Color32::from_gray(232),
    );

    painter.circle_filled(center, radius, egui::Color32::from_black_alpha(130));
    painter.circle_stroke(
        center,
        radius,
        egui::Stroke::new(2.0 * scale, egui::Color32::from_white_alpha(34)),
    );

    let start_angle = -std::f32::consts::PI * 0.5;
    let end_angle = start_angle + std::f32::consts::TAU * (player.boost as f32 / 100.0);
    if player.boost > 0 {
        let segments = 28;
        let mut points = Vec::with_capacity(segments + 1);
        for i in 0..=segments {
            let t = i as f32 / segments as f32;
            let angle = start_angle + (end_angle - start_angle) * t;
            points.push(center + egui::vec2(angle.cos(), angle.sin()) * radius);
        }
        painter.add(egui::Shape::Path(egui::epaint::PathShape {
            points,
            closed: false,
            fill: egui::Color32::TRANSPARENT,
            stroke: egui::Stroke::new(3.0 * scale, boost_color).into(),
        }));
    }

    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        player.boost.to_string(),
        egui::FontId::monospace(9.5 * scale),
        egui::Color32::WHITE,
    );
}

fn draw_teammate_boost_compact_content(
    ui: &egui::Ui,
    rect: egui::Rect,
    player: &PlayerInfo,
    scale: f32,
) {
    let painter = ui.painter();
    let boost_color = teammate_boost_color(player.boost);
    let inner = rect.shrink2(egui::vec2(8.0 * scale, 3.0 * scale));
    painter.text(
        egui::pos2(inner.left(), inner.center().y),
        egui::Align2::LEFT_CENTER,
        &player.name,
        egui::FontId::proportional(10.0 * scale),
        egui::Color32::from_gray(232),
    );
    painter.text(
        egui::pos2(inner.right(), inner.center().y),
        egui::Align2::RIGHT_CENTER,
        format!("{:>3}", player.boost),
        egui::FontId::monospace(15.0 * scale),
        boost_color,
    );
}

fn draw_teammate_boost_number_content(
    ui: &egui::Ui,
    rect: egui::Rect,
    player: &PlayerInfo,
    scale: f32,
) {
    let painter = ui.painter();
    let boost_color = teammate_boost_color(player.boost);
    let inner = rect.shrink2(egui::vec2(8.0 * scale, 2.0 * scale));
    painter.text(
        egui::pos2(inner.left(), inner.center().y),
        egui::Align2::LEFT_CENTER,
        player.name.chars().take(10).collect::<String>(),
        egui::FontId::proportional(9.0 * scale),
        egui::Color32::from_gray(210),
    );
    painter.text(
        egui::pos2(inner.right(), inner.center().y),
        egui::Align2::RIGHT_CENTER,
        player.boost.to_string(),
        egui::FontId::monospace(18.0 * scale),
        boost_color,
    );
}

fn teammate_boost_color(boost: u8) -> egui::Color32 {
    match boost {
        0..=20 => egui::Color32::from_rgb(255, 56, 48),
        21..=50 => egui::Color32::from_rgb(255, 157, 28),
        51..=80 => egui::Color32::from_rgb(255, 224, 74),
        _ => egui::Color32::from_rgb(102, 232, 255),
    }
}

fn teammate_boost_width(scale: f32, display: TeammateBoostDisplay) -> f32 {
    match display {
        TeammateBoostDisplay::Bars => 178.0 * scale,
        TeammateBoostDisplay::Circles => 142.0 * scale,
        TeammateBoostDisplay::Compact => 142.0 * scale,
        TeammateBoostDisplay::Numbers => 96.0 * scale,
    }
}

fn teammate_boost_panel_height(count: usize, scale: f32, display: TeammateBoostDisplay) -> f32 {
    let rows = count as f32 * teammate_boost_row_height(scale, display);
    let gaps = count.saturating_sub(1) as f32 * 3.0 * scale;
    rows + gaps + 10.0 * scale
}

fn teammate_boost_row_height(scale: f32, display: TeammateBoostDisplay) -> f32 {
    match display {
        TeammateBoostDisplay::Bars => 27.0 * scale,
        TeammateBoostDisplay::Circles => 30.0 * scale,
        TeammateBoostDisplay::Compact => 21.0 * scale,
        TeammateBoostDisplay::Numbers => 20.0 * scale,
    }
}
