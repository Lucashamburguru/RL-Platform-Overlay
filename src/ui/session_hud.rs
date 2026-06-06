use crate::session::{SessionOverlayDisplay, SessionState};
use crate::state::AppState;
use eframe::egui;
use std::sync::Arc;

use super::common::debug_status_row;
use super::layout::{
    active_layout_drag_position, normalized_to_pos, persist_dragged_position,
    render_drag_position_handle,
};

pub(super) fn render_session_overlay(ctx: &egui::Context, state: &Arc<AppState>) {
    let config = state.config.load();
    let position = active_layout_drag_position(ctx, "session").or_else(|| {
        config
            .session_manual_position
            .map(|position| normalized_to_pos(ctx, position))
    });
    let area = egui::Area::new("session_overlay_panel".into()).order(egui::Order::Foreground);
    let area = if let Some(position) = position {
        area.fixed_pos(position)
    } else {
        // Fallback default: Top Left
        area.anchor(egui::Align2::LEFT_TOP, egui::vec2(24.0, 150.0) * config.session_overlay_scale)
    };

    let response = area.show(ctx, |ui| {
        draw_session_panel(
            ui,
            &state.session.load(),
            config.session_overlay_scale,
            config.session_overlay_display,
            config.session_overlay_opacity,
        );
        render_drag_position_handle(ui, config.layout_mode, config.session_overlay_scale)
    });

    if let Some(drag_response) = response.inner {
        persist_dragged_position(
            ctx,
            state,
            response.response.rect.min,
            "session",
            &drag_response,
        );
    }
}

pub(super) fn draw_session_panel(
    ui: &mut egui::Ui,
    session: &SessionState,
    scale: f32,
    display: SessionOverlayDisplay,
    opacity: u8,
) {
    let width = match display {
        SessionOverlayDisplay::Compact => 155.0 * scale,
        SessionOverlayDisplay::Expanded => 220.0 * scale,
    };
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgba_unmultiplied(16, 18, 24, opacity))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(24)))
        .corner_radius(6.0 * scale)
        .inner_margin(8.0 * scale);

    frame.show(ui, |ui| {
        ui.set_min_width(width);
        ui.label(
            egui::RichText::new("SESSION")
                .size(9.0 * scale)
                .strong()
                .color(egui::Color32::from_gray(170)),
        );
        ui.add_space(4.0 * scale);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("{}W", session.wins))
                    .size(17.0 * scale)
                    .strong()
                    .color(egui::Color32::from_rgb(90, 230, 150)),
            );
            ui.label(
                egui::RichText::new(format!("{}L", session.losses))
                    .size(17.0 * scale)
                    .strong()
                    .color(egui::Color32::from_rgb(255, 105, 105)),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(streak_label(session.streak))
                        .size(12.0 * scale)
                        .color(egui::Color32::from_rgb(220, 220, 245)),
                );
            });
        });

        if display == SessionOverlayDisplay::Expanded {
            ui.separator();
            debug_status_row(ui, "Matches", &session.matches_played.to_string());
            debug_status_row(ui, "Last", session.last_result.label());
            debug_status_row(
                ui,
                "Score",
                &format!("{} - {}", session.blue_score, session.orange_score),
            );
        }
    });
}

fn streak_label(streak: i32) -> String {
    if streak > 0 {
        format!("+{} streak", streak)
    } else if streak < 0 {
        format!("{} streak", streak)
    } else {
        "no streak".to_string()
    }
}

pub(super) fn session_display_label(display: SessionOverlayDisplay) -> &'static str {
    match display {
        SessionOverlayDisplay::Compact => "Compact",
        SessionOverlayDisplay::Expanded => "Expanded",
    }
}
