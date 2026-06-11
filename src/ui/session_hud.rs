use crate::mmr::TrackerSnapshot;
use crate::session::{
    SessionMode, SessionModeRecord, SessionOverlayDisplay, SessionState, format_win_rate,
};
use crate::state::{AppState, LocalMmrState};
use eframe::egui;
use std::sync::Arc;

use super::layout::{
    active_layout_drag_position, normalized_to_pos, persist_dragged_position,
    render_drag_position_handle,
};

pub(super) fn render_session_overlay(ctx: &egui::Context, state: &Arc<AppState>) {
    let config = state.system.config.load();
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
        area.anchor(
            egui::Align2::LEFT_TOP,
            egui::vec2(24.0, 150.0) * config.session_overlay_scale,
        )
    };

    let response = area.show(ctx, |ui| {
        draw_session_panel(
            ui,
            &state.game.session.load(),
            &state.mmr.local_mmr.load(),
            config.session_overlay_scale,
            config.session_overlay_display,
            config.session_overlay_opacity,
            SessionHudOptions {
                show_streaks: config.session_expanded_show_streaks,
                show_breakdown: config.session_expanded_show_breakdown,
                show_mmr_delta: config.session_expanded_show_mmr_delta,
            },
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
    local_mmr: &LocalMmrState,
    scale: f32,
    display: SessionOverlayDisplay,
    opacity: u8,
    options: SessionHudOptions,
) {
    let width = match display {
        SessionOverlayDisplay::Compact => 155.0 * scale,
        SessionOverlayDisplay::Expanded => 280.0 * scale,
    };
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgba_unmultiplied(16, 18, 24, opacity))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(24)))
        .corner_radius(6.0 * scale)
        .inner_margin(8.0 * scale);

    frame.show(ui, |ui| {
        ui.set_min_width(width);
        match display {
            SessionOverlayDisplay::Compact => draw_compact_session(ui, session, scale),
            SessionOverlayDisplay::Expanded => {
                draw_expanded_session(ui, session, local_mmr, scale, options)
            }
        }
    });
}

#[derive(Clone, Copy, Debug)]
pub(super) struct SessionHudOptions {
    pub show_streaks: bool,
    pub show_breakdown: bool,
    pub show_mmr_delta: bool,
}

fn draw_compact_session(ui: &mut egui::Ui, session: &SessionState, scale: f32) {
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
}

fn draw_expanded_session(
    ui: &mut egui::Ui,
    session: &SessionState,
    local_mmr: &LocalMmrState,
    scale: f32,
    options: SessionHudOptions,
) {
    let overall = SessionModeRecord {
        wins: session.wins,
        losses: session.losses,
    };

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("SESSION")
                .size(10.0 * scale)
                .strong()
                .color(egui::Color32::from_gray(170)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format_win_rate(session.wins, session.losses))
                    .size(12.0 * scale)
                    .strong()
                    .color(egui::Color32::from_rgb(220, 220, 245)),
            );
        });
    });
    ui.add_space(4.0 * scale);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{}W", session.wins))
                .size(21.0 * scale)
                .strong()
                .color(egui::Color32::from_rgb(90, 230, 150)),
        );
        ui.label(
            egui::RichText::new(format!("{}L", session.losses))
                .size(21.0 * scale)
                .strong()
                .color(egui::Color32::from_rgb(255, 105, 105)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(streak_label(session.streak))
                    .size(12.0 * scale)
                    .color(streak_color(session.streak)),
            );
        });
    });

    if options.show_streaks {
        ui.separator();
        section_label(ui, "STREAKS & STATS", scale);
        stat_row(ui, "Current", &streak_label(session.streak), scale);
        stat_row(ui, "Best win", &session.best_win_streak.to_string(), scale);
        stat_row(
            ui,
            "Worst loss",
            &session.worst_loss_streak.to_string(),
            scale,
        );
        stat_row(ui, "Last", session.last_result.label(), scale);
    }

    if options.show_breakdown {
        ui.add_space(4.0 * scale);
        section_label(ui, "BREAKDOWN", scale);
        breakdown_header(ui, scale, options.show_mmr_delta);
        breakdown_row(
            ui,
            "Overall",
            &overall,
            Some(session.active_mode),
            local_mmr,
            scale,
            options.show_mmr_delta,
        );
        for (mode, record) in &session.mode_records {
            if record.matches_played() > 0 {
                breakdown_row(
                    ui,
                    mode.label(),
                    record,
                    Some(*mode),
                    local_mmr,
                    scale,
                    options.show_mmr_delta,
                );
            }
        }
    }
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

fn streak_color(streak: i32) -> egui::Color32 {
    if streak > 0 {
        egui::Color32::from_rgb(90, 230, 150)
    } else if streak < 0 {
        egui::Color32::from_rgb(255, 105, 105)
    } else {
        egui::Color32::from_rgb(220, 220, 245)
    }
}

fn section_label(ui: &mut egui::Ui, label: &str, scale: f32) {
    ui.label(
        egui::RichText::new(label)
            .size(9.0 * scale)
            .strong()
            .color(egui::Color32::from_gray(170)),
    );
}

fn stat_row(ui: &mut egui::Ui, label: &str, value: &str, scale: f32) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .size(11.0 * scale)
                .color(egui::Color32::from_gray(178)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(value)
                    .size(11.0 * scale)
                    .color(egui::Color32::from_rgb(225, 227, 235)),
            );
        });
    });
}

fn session_mode_playlist_id(session_mode: SessionMode) -> Option<i32> {
    match session_mode {
        SessionMode::Ones => Some(10),
        SessionMode::Twos => Some(11),
        SessionMode::Threes => Some(13),
        SessionMode::Hoops => Some(27),
        SessionMode::Dropshot => Some(29),
        SessionMode::Knockout | SessionMode::Freeplay | SessionMode::Unknown => None,
    }
}

fn playlist_rating(snapshot: Option<&TrackerSnapshot>, playlist_id: i32) -> Option<i32> {
    snapshot?
        .playlists
        .get(&playlist_id)
        .map(|playlist| playlist.rating)
}

fn mmr_delta_label(delta: i32) -> String {
    if delta > 0 {
        format!("+{delta}")
    } else {
        delta.to_string()
    }
}

fn mmr_delta_color(delta: i32) -> egui::Color32 {
    if delta > 0 {
        egui::Color32::from_rgb(90, 230, 150)
    } else if delta < 0 {
        egui::Color32::from_rgb(255, 105, 105)
    } else {
        egui::Color32::from_rgb(225, 227, 235)
    }
}

fn mmr_delta_for_mode(session_mode: Option<SessionMode>, local_mmr: &LocalMmrState) -> Option<i32> {
    let playlist_id = session_mode.and_then(session_mode_playlist_id)?;
    let current = playlist_rating(local_mmr.current.as_ref(), playlist_id)?;
    let previous = playlist_rating(local_mmr.previous.as_ref(), playlist_id)?;
    Some(current - previous)
}

fn breakdown_header(ui: &mut egui::Ui, scale: f32, show_mmr_delta: bool) {
    ui.horizontal(|ui| {
        ui.add_sized([66.0 * scale, 16.0 * scale], muted_label("Mode", scale));
        ui.add_sized([52.0 * scale, 16.0 * scale], muted_label("Record", scale));
        ui.add_sized([42.0 * scale, 16.0 * scale], muted_label("Games", scale));
        if show_mmr_delta {
            ui.add_sized([44.0 * scale, 16.0 * scale], muted_label("MMR", scale));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(muted_text("WR", scale));
        });
    });
}

fn breakdown_row(
    ui: &mut egui::Ui,
    label: &str,
    record: &SessionModeRecord,
    mode: Option<SessionMode>,
    local_mmr: &LocalMmrState,
    scale: f32,
    show_mmr_delta: bool,
) {
    ui.horizontal(|ui| {
        ui.add_sized([66.0 * scale, 18.0 * scale], value_label(label, scale));
        ui.add_sized(
            [52.0 * scale, 18.0 * scale],
            value_label(format!("{}-{}", record.wins, record.losses), scale),
        );
        ui.add_sized(
            [42.0 * scale, 18.0 * scale],
            value_label(record.matches_played().to_string(), scale),
        );
        if show_mmr_delta {
            let delta = mmr_delta_for_mode(mode, local_mmr);
            let text = delta
                .map(mmr_delta_label)
                .unwrap_or_else(|| "-".to_string());
            let color = delta
                .map(mmr_delta_color)
                .unwrap_or_else(|| egui::Color32::from_rgb(225, 227, 235));
            ui.add_sized(
                [44.0 * scale, 18.0 * scale],
                egui::Label::new(value_text(text, scale).color(color)),
            );
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(value_text(
                format_win_rate(record.wins, record.losses),
                scale,
            ));
        });
    });
}

fn muted_text(text: impl Into<String>, scale: f32) -> egui::RichText {
    egui::RichText::new(text.into())
        .size(10.0 * scale)
        .color(egui::Color32::from_gray(150))
}

fn value_text(text: impl Into<String>, scale: f32) -> egui::RichText {
    egui::RichText::new(text.into())
        .size(11.0 * scale)
        .color(egui::Color32::from_rgb(225, 227, 235))
}

fn muted_label(text: impl Into<String>, scale: f32) -> egui::Label {
    egui::Label::new(muted_text(text, scale))
}

fn value_label(text: impl Into<String>, scale: f32) -> egui::Label {
    egui::Label::new(value_text(text, scale))
}

pub(super) fn session_display_label(display: SessionOverlayDisplay) -> &'static str {
    match display {
        SessionOverlayDisplay::Compact => "Compact",
        SessionOverlayDisplay::Expanded => "Expanded",
    }
}
