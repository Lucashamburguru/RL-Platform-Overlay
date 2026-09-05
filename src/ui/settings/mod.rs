use crate::state::AppState;
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::app::SettingsTab;
use crate::ui::common::{StatusTone, status_color};

mod boost;
mod dashboard;
mod history;
mod overlay;
mod replays;
mod session;
mod setup;
mod support;

pub(super) use boost::render_boost_settings_tab;
pub(super) use dashboard::render_dashboard_settings_tab;
pub(super) use history::render_history_settings_tab;
pub(super) use overlay::render_overlay_settings_tab;
pub(super) use replays::render_replays_settings_tab;
pub(super) use session::render_session_settings_tab;
pub(super) use setup::render_setup_settings_tab;
pub(super) use support::render_support_settings_tab;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ArrangeHudAction {
    Start,
    Done,
    Cancel,
}

pub(super) fn render_settings_tabs(
    ui: &mut egui::Ui,
    selected: &mut SettingsTab,
    debug_enabled: bool,
) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.selectable_value(selected, SettingsTab::Setup, "Setup");
        ui.selectable_value(selected, SettingsTab::Overlay, "Lobby");
        ui.selectable_value(selected, SettingsTab::Dashboard, "Dashboard");
        ui.selectable_value(selected, SettingsTab::Session, "Session");
        ui.selectable_value(selected, SettingsTab::Boost, "Boost & Alpha");
        ui.selectable_value(selected, SettingsTab::Replays, "Replays");
        ui.selectable_value(selected, SettingsTab::History, "History");
        ui.selectable_value(selected, SettingsTab::Support, "Support");
        if debug_enabled {
            ui.selectable_value(selected, SettingsTab::Debug, "Debug");
        }
    });
    ui.add_space(8.0);
}

#[cfg(feature = "microsoft-store")]
pub(super) fn render_update_notice(ui: &mut egui::Ui, state: &Arc<AppState>) {
    let _ = (ui, state);
}

#[cfg(not(feature = "microsoft-store"))]
pub(super) fn render_update_notice(ui: &mut egui::Ui, state: &Arc<AppState>) {
    let version_check = state.system.version_check.load();
    if !version_check.update_available {
        return;
    }
    let auto_update_status = state.system.auto_update_status.load();

    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(55, 46, 18))
        .stroke(egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgb(255, 188, 72),
        ))
        .corner_radius(5.0)
        .inner_margin(8.0);

    frame.show(ui, |ui| {
        ui.label(
            egui::RichText::new(format!(
                "Update available: {}. Download the newest release from GitHub.",
                version_check.latest_tag
            ))
            .strong()
            .color(egui::Color32::from_rgb(255, 226, 150)),
        );
        egui::CollapsingHeader::new("What's new")
            .id_salt("update_release_notes")
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("update_release_notes_scroll")
                    .max_height(180.0)
                    .show(ui, |ui| {
                        if version_check.release_notes.is_empty() {
                            ui.weak("No release notes were provided for this version.");
                        } else {
                            ui.label(version_check.release_notes.as_str());
                        }
                    });
            });
        ui.horizontal(|ui| {
            #[cfg(target_os = "windows")]
            {
                let can_auto_update = !auto_update_status.running
                    && !version_check.windows_download_url.is_empty()
                    && !version_check.windows_checksum_url.is_empty()
                    && !version_check.windows_signature_url.is_empty();
                if ui
                    .add_enabled(can_auto_update, egui::Button::new("Update and restart"))
                    .clicked()
                {
                    crate::update::start_auto_update(state.clone());
                }
            }
            ui.hyperlink_to("Download release", &version_check.release_url);
        });
        if auto_update_status.running && !auto_update_status.message.is_empty() {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.label(auto_update_status.message.as_str());
            });
        } else if !auto_update_status.error.is_empty() {
            ui.colored_label(
                egui::Color32::from_rgb(230, 100, 80),
                auto_update_status.error.as_str(),
            );
        } else if cfg!(target_os = "windows")
            && (version_check.windows_download_url.is_empty()
                || version_check.windows_checksum_url.is_empty()
                || version_check.windows_signature_url.is_empty())
        {
            ui.colored_label(
                egui::Color32::from_rgb(230, 100, 80),
                "Automatic update is unavailable for this release. Use the release link.",
            );
        }
    });
    ui.add_space(6.0);
}

pub(super) fn render_launch_controls(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &Arc<AppState>,
    is_launched: bool,
    config_edit: &mut crate::state::Config,
    changed: &mut bool,
    confirm_modal: &mut Option<super::app::ConfirmAction>,
) -> Option<ArrangeHudAction> {
    let mut arrange_action = None;
    ui.horizontal(|ui| {
        ui.set_width(ui.available_width());
        let btn_text = if is_launched {
            "Stop Overlay"
        } else {
            "Launch Overlay"
        };
        if ui
            .add_sized(
                [124.0, 26.0],
                egui::Button::new(egui::RichText::new(btn_text).strong()),
            )
            .clicked()
        {
            if is_launched {
                state.flags.is_launched.store(false, Ordering::SeqCst);
            } else if crate::input::try_launch_overlay_at_path(
                state,
                &config_edit.rocket_league_path,
                "settings_button",
            ) {
                if config_edit.dashboard_open_with_overlay {
                    config_edit.dashboard_enabled = true;
                    *changed = true;
                }
                state
                    .flags
                    .is_settings_visible
                    .store(false, Ordering::SeqCst);
            }
        }

        ui.add_space(10.0);
        let is_visible = state.flags.is_visible.load(Ordering::SeqCst);
        ui.horizontal(|ui| {
            ui.label("HUD:");
            if !is_launched {
                ui.colored_label(status_color(StatusTone::Error), "STOPPED");
            } else if !is_visible {
                ui.colored_label(status_color(StatusTone::Warning), "HIDDEN");
            } else {
                ui.colored_label(status_color(StatusTone::Success), "VISIBLE");
            }
        });

        ui.add_space(8.0);
        if config_edit.layout_mode {
            if ui
                .add(egui::Button::new(egui::RichText::new("Done").strong()))
                .on_hover_text("Save the current HUD positions")
                .clicked()
            {
                config_edit.layout_mode = false;
                *changed = true;
                arrange_action = Some(ArrangeHudAction::Done);
            }
            if ui
                .button("Cancel")
                .on_hover_text("Restore the HUD positions from before arranging")
                .clicked()
            {
                config_edit.layout_mode = false;
                *changed = true;
                arrange_action = Some(ArrangeHudAction::Cancel);
            }
            if ui
                .button("Reset All")
                .on_hover_text("Return every movable HUD panel to its default position")
                .clicked()
            {
                config_edit.lobby_manual_position = None;
                config_edit.teammate_boost_manual_position = None;
                config_edit.session_manual_position = None;
                *changed = true;
            }
        } else if ui
            .button(egui::RichText::new("Arrange HUD").strong())
            .on_hover_text("Preview and drag all enabled HUD panels")
            .clicked()
        {
            config_edit.layout_mode = true;
            *changed = true;
            arrange_action = Some(ArrangeHudAction::Start);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_sized([70.0, 24.0], egui::Button::new("Quit"))
                .clicked()
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if ui
                .add_sized([96.0, 24.0], egui::Button::new("Reset Config"))
                .clicked()
            {
                *confirm_modal = Some(super::app::ConfirmAction::ResetConfig);
            }
            ui.label(
                egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .size(9.0)
                    .color(egui::Color32::from_gray(100)),
            );
        });
    });
    arrange_action
}
