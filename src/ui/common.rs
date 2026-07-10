use eframe::egui;

const SETTINGS_LABEL_WIDTH: f32 = 150.0;
const SETTINGS_SECTION_RADIUS: u8 = 6;
pub(super) const SETTINGS_HELPER_TEXT_SIZE: f32 = 11.5;
pub(super) const SETTINGS_LABEL_TEXT_SIZE: f32 = 12.5;
pub(super) const SETTINGS_SECTION_TITLE_SIZE: f32 = 14.0;

pub(super) fn helper_text(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text.into())
        .size(SETTINGS_HELPER_TEXT_SIZE)
        .color(egui::Color32::from_gray(178))
}

pub(super) fn debug_status_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
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
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(52, 55, 64)))
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
    ui.horizontal(|ui| {
        ui.set_width(ui.available_width());
        ui.add_sized(
            [SETTINGS_LABEL_WIDTH, 20.0],
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
    add_contents: impl FnOnce(&mut egui::Ui, &mut egui::Ui),
) {
    ui.columns(2, |columns| {
        columns[0].set_width(columns[0].available_width());
        columns[1].set_width(columns[1].available_width());
        let (left_columns, right_columns) = columns.split_at_mut(1);
        add_contents(&mut left_columns[0], &mut right_columns[0]);
    });
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

pub(crate) fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
}
