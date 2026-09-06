use eframe::egui;

const SETTINGS_LABEL_WIDTH: f32 = 150.0;
const SETTINGS_SECTION_RADIUS: u8 = 6;
pub(super) const SETTINGS_HELPER_TEXT_SIZE: f32 = 12.0;
pub(super) const SETTINGS_LABEL_TEXT_SIZE: f32 = 14.0;
pub(super) const SETTINGS_SECTION_TITLE_SIZE: f32 = 16.0;
pub(super) const OVERLAY_RADIUS: f32 = 6.0;

pub(super) fn overlay_panel_frame(scale: f32, opacity: u8) -> egui::Frame {
    egui::Frame::default()
        .fill(overlay_panel_fill(opacity))
        .stroke(overlay_panel_stroke())
        .corner_radius(OVERLAY_RADIUS * scale)
        .inner_margin(8.0 * scale)
}

pub(super) fn overlay_panel_fill(opacity: u8) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(16, 18, 24, opacity)
}

pub(super) fn overlay_panel_stroke() -> egui::Stroke {
    egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(24))
}

pub(super) fn overlay_row_fill() -> egui::Color32 {
    egui::Color32::from_black_alpha(92)
}

pub(super) fn overlay_row_stroke() -> egui::Stroke {
    egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(18))
}

pub(super) fn overlay_title_color() -> egui::Color32 {
    egui::Color32::from_gray(180)
}

pub(super) fn overlay_muted_color() -> egui::Color32 {
    egui::Color32::from_gray(150)
}

pub(super) fn overlay_subtle_color() -> egui::Color32 {
    egui::Color32::from_gray(120)
}

pub(super) fn overlay_text_color() -> egui::Color32 {
    egui::Color32::from_rgb(225, 227, 235)
}

pub(super) fn overlay_player_text_color() -> egui::Color32 {
    egui::Color32::from_gray(232)
}

pub(super) fn overlay_local_text_color() -> egui::Color32 {
    egui::Color32::from_rgb(230, 255, 245)
}

pub(super) fn overlay_success_color() -> egui::Color32 {
    egui::Color32::from_rgb(90, 230, 150)
}

pub(super) fn overlay_danger_color() -> egui::Color32 {
    egui::Color32::from_rgb(255, 105, 105)
}

pub(super) fn overlay_disconnected_color() -> egui::Color32 {
    egui::Color32::from_rgb(255, 80, 80)
}

pub(super) fn overlay_team_color(team: u8) -> egui::Color32 {
    match team {
        0 => egui::Color32::from_rgb(0, 212, 255),
        1 => egui::Color32::from_rgb(255, 140, 0),
        _ => egui::Color32::from_gray(165),
    }
}

pub(super) fn overlay_boost_color(boost: u8) -> egui::Color32 {
    match boost {
        0..=20 => egui::Color32::from_rgb(255, 56, 48),
        21..=50 => egui::Color32::from_rgb(255, 157, 28),
        51..=80 => egui::Color32::from_rgb(255, 224, 74),
        _ => egui::Color32::from_rgb(102, 232, 255),
    }
}

pub(super) fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    if haystack.len() < needle.len() {
        return false;
    }

    let needle = needle.as_bytes();
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|candidate| candidate.eq_ignore_ascii_case(needle))
}

pub(super) fn helper_text(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text.into())
        .size(SETTINGS_HELPER_TEXT_SIZE)
        .color(egui::Color32::from_gray(178))
}

pub(super) fn debug_status_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(label)
                .size(SETTINGS_LABEL_TEXT_SIZE)
                .color(egui::Color32::from_gray(178)),
        );
        ui.label(value);
    });
}

pub(super) fn settings_section(
    ui: &mut egui::Ui,
    title: &str,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let frame = egui::Frame::default()
        .fill(egui::Color32::TRANSPARENT)
        .stroke(egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgb(52, 55, 64),
        ))
        .corner_radius(SETTINGS_SECTION_RADIUS)
        .inner_margin(egui::Margin::same(10));

    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(
            egui::RichText::new(title)
                .size(SETTINGS_SECTION_TITLE_SIZE)
                .strong()
                .color(egui::Color32::from_rgb(225, 227, 235)),
        );
        ui.add_space(8.0);
        add_contents(ui);
    });
}

pub(super) fn setting_row(ui: &mut egui::Ui, label: &str, add_control: impl FnOnce(&mut egui::Ui)) {
    if ui.available_width() < 300.0 {
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(label).color(egui::Color32::from_gray(210)));
            ui.horizontal_wrapped(add_control);
        });
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.set_width(ui.available_width());
        let label_width = SETTINGS_LABEL_WIDTH.min((ui.available_width() * 0.42).max(92.0));
        ui.add_sized(
            [label_width, 20.0],
            egui::Label::new(
                egui::RichText::new(label)
                    .size(SETTINGS_LABEL_TEXT_SIZE)
                    .color(egui::Color32::from_gray(188)),
            ),
        );
        add_control(ui);
    });
}

pub(super) fn settings_two_column(
    ui: &mut egui::Ui,
    mut add_contents: impl FnMut(&mut egui::Ui, bool),
) {
    if ui.available_width() < 700.0 {
        ui.push_id("first_column", |ui| add_contents(ui, false));
        ui.add_space(12.0);
        ui.push_id("second_column", |ui| add_contents(ui, true));
        return;
    }
    ui.columns(2, |columns| {
        columns[0].set_width(columns[0].available_width());
        columns[1].set_width(columns[1].available_width());
        columns[0].push_id("first_column", |ui| add_contents(ui, false));
        columns[1].push_id("second_column", |ui| add_contents(ui, true));
    });
}

pub(super) fn settings_style(ui: &mut egui::Ui) {
    let style = ui.style_mut();
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
    style.spacing.interact_size.y = 26.0;
    style.spacing.item_spacing = egui::vec2(7.0, 6.0);
    style.spacing.button_padding = egui::vec2(9.0, 4.0);
    style.visuals.widgets.inactive.bg_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_gray(100));
    style.visuals.widgets.inactive.fg_stroke.color = egui::Color32::from_gray(225);
    style.visuals.widgets.hovered.bg_stroke =
        egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(0, 190, 230));
    style.visuals.widgets.active.bg_stroke =
        egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(0, 210, 255));
}

pub(super) fn scale_slider(
    ui: &mut egui::Ui,
    scale: &mut f32,
    min: f32,
    max: f32,
) -> egui::Response {
    let mut percent = *scale * 100.0;
    let response = ui.add(
        egui::Slider::new(&mut percent, min * 100.0..=max * 100.0)
            .clamping(egui::SliderClamping::Edits)
            .suffix("%")
            .fixed_decimals(0),
    );
    if response.changed() {
        *scale = percent / 100.0;
    }
    response
}

pub(super) fn opacity_slider(ui: &mut egui::Ui, opacity: &mut u8, min: u8) -> egui::Response {
    let mut percent = f32::from(*opacity) * 100.0 / 255.0;
    let response = ui.add(
        egui::Slider::new(&mut percent, f32::from(min) * 100.0 / 255.0..=100.0)
            .clamping(egui::SliderClamping::Edits)
            .suffix("%")
            .fixed_decimals(0),
    );
    if response.changed() {
        *opacity = (percent * 255.0 / 100.0)
            .round()
            .clamp(f32::from(min), 255.0) as u8;
    }
    response
}

pub(super) fn maintenance_status(ui: &mut egui::Ui, status: &crate::state::MaintenanceStatus) {
    match status {
        crate::state::MaintenanceStatus::Idle => {}
        crate::state::MaintenanceStatus::Running => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Clearing…");
            });
        }
        crate::state::MaintenanceStatus::Success(message) => {
            status_text(ui, StatusTone::Success, message)
        }
        crate::state::MaintenanceStatus::Failed(message) => {
            status_text(ui, StatusTone::Error, message)
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum StatusTone {
    Success,
    Warning,
    Error,
    Neutral,
}

pub(super) fn status_color(tone: StatusTone) -> egui::Color32 {
    match tone {
        StatusTone::Success => egui::Color32::from_rgb(100, 220, 120),
        StatusTone::Warning => egui::Color32::from_rgb(225, 190, 90),
        StatusTone::Error => egui::Color32::from_rgb(230, 95, 85),
        StatusTone::Neutral => egui::Color32::from_gray(165),
    }
}

pub(super) fn status_text(ui: &mut egui::Ui, tone: StatusTone, text: impl Into<String>) {
    ui.label(
        egui::RichText::new(text.into())
            .size(SETTINGS_LABEL_TEXT_SIZE)
            .color(status_color(tone)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawing_percent_controls_does_not_round_saved_values() {
        let ctx = egui::Context::default();
        let mut opacity = 173;
        let mut scale = 1.234;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                opacity_slider(ui, &mut opacity, 40);
                scale_slider(ui, &mut scale, 0.5, 2.5);
            });
        });
        assert_eq!(opacity, 173);
        assert_eq!(scale, 1.234);
    }

    #[test]
    fn contains_ignore_ascii_case_matches_without_allocating_lowercase_strings() {
        assert!(contains_ignore_ascii_case("Ranked Doubles 2v2", "doubles"));
        assert!(contains_ignore_ascii_case("Grand Champion III", "CHAMPION"));
        assert!(contains_ignore_ascii_case("Epic Games", "epic"));
        assert!(contains_ignore_ascii_case("abc", ""));
        assert!(!contains_ignore_ascii_case("abc", "abcd"));
        assert!(!contains_ignore_ascii_case("Steam", "epic"));
    }
}
