use eframe::egui;

pub(super) fn debug_status_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(egui::Color32::from_gray(150)));
        ui.label(value);
    });
}
