use crate::state::{AppState, Config};
use eframe::egui;
use std::sync::Arc;

pub(crate) fn render_support_settings_tab(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config_edit: &mut Config,
    changed: &mut bool,
    is_rl_running: bool,
    rl_process_detection_detail: &str,
) {
    super::setup::render_support_diagnostics_section(
        ui,
        state,
        config_edit,
        changed,
        is_rl_running,
        rl_process_detection_detail,
    );
}
