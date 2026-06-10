use crate::state::{AppState, Config, TeammateBoostDisplay};
use crate::ui::boost_hud::{
    draw_teammate_boost_panel, preview_teammates, teammate_boost_display_label,
};
use crate::ui::common::{
    StatusTone, debug_status_row, helper_text, setting_row, settings_section, status_text,
};
use eframe::egui;
use std::sync::Arc;

pub(crate) fn render_boost_settings_tab(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config_edit: &mut Config,
    changed: &mut bool,
    is_rl_running: bool,
) {
    settings_section(ui, "Teammate Boost HUD", |ui| {
        if ui
            .checkbox(
                &mut config_edit.show_teammate_boost,
                "Always-on Teammate Boost HUD",
            )
            .changed()
        {
            *changed = true;
        }

        ui.add_space(8.0);
        setting_row(ui, "Display", |ui| {
            egui::ComboBox::new("teammate_boost_display", "")
                .selected_text(teammate_boost_display_label(
                    config_edit.teammate_boost_display,
                ))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut config_edit.teammate_boost_display,
                        TeammateBoostDisplay::Bars,
                        "Bars",
                    );
                    ui.selectable_value(
                        &mut config_edit.teammate_boost_display,
                        TeammateBoostDisplay::Circles,
                        "Circles",
                    );
                    ui.selectable_value(
                        &mut config_edit.teammate_boost_display,
                        TeammateBoostDisplay::Compact,
                        "Compact",
                    );
                    ui.selectable_value(
                        &mut config_edit.teammate_boost_display,
                        TeammateBoostDisplay::Numbers,
                        "Numbers",
                    );
                });
            if config_edit.teammate_boost_display != state.config.load().teammate_boost_display {
                *changed = true;
            }
        });

        ui.add_space(8.0);
        setting_row(ui, "HUD Scale", |ui| {
            if ui
                .add_sized(
                    [ui.available_width(), 20.0],
                    egui::Slider::new(&mut config_edit.teammate_hud_scale, 0.5..=2.5),
                )
                .changed()
            {
                *changed = true;
            }
        });
    });

    ui.add_space(10.0);
    settings_section(ui, "Live Preview", |ui| {
        let preview = preview_teammates(state);
        draw_teammate_boost_panel(
            ui,
            &preview,
            0,
            config_edit.teammate_hud_scale.min(1.4),
            config_edit.teammate_boost_display,
        );
        ui.add_space(6.0);
        ui.label(helper_text(
            "Placement preview is only accurate while the overlay is launched.",
        ));
    });

    ui.add_space(12.0);
    settings_section(ui, "Alpha Boost (Gold Rush) Swap", |ui| {
        // 1. Rocket League Folder Path Input
        setting_row(ui, "Rocket League Folder", |ui| {
            ui.horizontal(|ui| {
                let input_width = (ui.available_width() - 96.0).max(160.0);
                let path_edit = ui.add_sized(
                    [input_width, 22.0],
                    egui::TextEdit::singleline(&mut config_edit.rocket_league_path),
                );
                if path_edit.changed() {
                    *changed = true;
                }
                if ui.button("Auto-detect").clicked()
                    && let Some(detected) = crate::state::detect_rocket_league_path()
                {
                    config_edit.rocket_league_path = detected;
                    *changed = true;
                    let mut status = state
                        .boost
                        .boost_swap_status
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    *status = "Idle".to_string();
                }
            });
        });

        // Path validation feedback
        let path_valid = if config_edit.rocket_league_path.trim().is_empty() {
            None
        } else {
            let path = std::path::Path::new(&config_edit.rocket_league_path);
            Some(path.exists() && path.join("TAGame").join("CookedPCConsole").exists())
        };

        match path_valid {
            Some(true) => {
                status_text(
                    ui,
                    StatusTone::Success,
                    "✔ Valid Rocket League installation found.",
                );
            }
            Some(false) => {
                status_text(
                    ui,
                    StatusTone::Error,
                    "❌ Invalid folder (TAGame/CookedPCConsole not found).",
                );
            }
            None => {
                status_text(
                    ui,
                    StatusTone::Warning,
                    "⚠ Path unconfigured. Paste path or click Auto-detect.",
                );
            }
        }

        ui.add_space(8.0);

        let inspection = crate::assets::inspect_boost_swap(&config_edit.rocket_league_path);
        debug_status_row(
            ui,
            "Backup Metadata",
            if inspection.metadata_exists {
                "yes"
            } else {
                "no"
            },
        );
        debug_status_row(
            ui,
            "Cached Assets",
            if inspection.cache_verified {
                "verified"
            } else {
                "not verified"
            },
        );
        debug_status_row(ui, "Game Files", inspection.game_file_state.label());
        ui.label(helper_text(&inspection.message));
        ui.add_space(8.0);

        // Warning message required by user
        status_text(
            ui,
            StatusTone::Warning,
            "⚠ Warning: Editing game files can technically be bannable (violates ToS). Use at your own risk.",
        );

        ui.add_space(8.0);

        let mut enabled = inspection.game_file_state == crate::assets::BoostGameFileState::Alpha;
        let can_toggle = matches!(
            inspection.game_file_state,
            crate::assets::BoostGameFileState::Original
                | crate::assets::BoostGameFileState::Alpha
                | crate::assets::BoostGameFileState::Unbacked
        );
        let checkbox_resp = ui.add_enabled(
            can_toggle,
            egui::Checkbox::new(
                &mut enabled,
                "Replace Standard Boost with Alpha Boost (Gold Rush)",
            ),
        );
        if checkbox_resp.changed() {
            if config_edit.rocket_league_path.trim().is_empty() {
                let mut status = state
                    .boost
                    .boost_swap_status
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                *status = "Error: Configure your Rocket League path first.".to_string();
            } else if path_valid != Some(true) {
                let mut status = state
                    .boost
                    .boost_swap_status
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                *status = "Error: Invalid Rocket League directory. Check the path and try again."
                    .to_string();
            } else {
                if enabled {
                    crate::assets::start_apply_alpha_boost(
                        state.clone(),
                        config_edit.rocket_league_path.clone(),
                    );
                } else {
                    crate::assets::start_restore_standard_boost(
                        state.clone(),
                        config_edit.rocket_league_path.clone(),
                    );
                }
            }
        }

        if inspection.game_file_state == crate::assets::BoostGameFileState::Unbacked {
            status_text(
                ui,
                StatusTone::Warning,
                "No backup metadata yet. First apply will back up the current game files as originals.",
            );
        } else if !can_toggle && path_valid == Some(true) {
            status_text(
                ui,
                StatusTone::Warning,
                "Current boost files are not a clean original/Alpha pair. Restore originals before applying.",
            );
        }

        if inspection.metadata_exists && ui.button("Restore Original Boost").clicked() {
            if config_edit.rocket_league_path.trim().is_empty() {
                let mut status = state
                    .boost
                    .boost_swap_status
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                *status = "Error: Configure your Rocket League path first.".to_string();
            } else if path_valid != Some(true) {
                let mut status = state
                    .boost
                    .boost_swap_status
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                *status = "Error: Invalid Rocket League directory. Check the path and try again."
                    .to_string();
            } else {
                crate::assets::start_restore_standard_boost(
                    state.clone(),
                    config_edit.rocket_league_path.clone(),
                );
            }
        }

        // Render swap operation feedback
        let status = state
            .boost
            .boost_swap_status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if status != "Idle" {
            ui.add_space(6.0);
            if status.starts_with("Error")
                || status.starts_with("Download failed")
                || status.starts_with("Backup failed")
                || status.starts_with("Swap failed")
                || status.starts_with("Restore failed")
                || status.starts_with("Failed")
                || status.starts_with("Blocked")
            {
                status_text(ui, StatusTone::Error, format!("❌ {status}"));
            } else if status.starts_with("Success") {
                status_text(ui, StatusTone::Success, format!("✔ {status}"));
            } else {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.label(&status);
                });
            }
        }

        // Game running warning
        if is_rl_running {
            ui.add_space(6.0);
            status_text(
                ui,
                StatusTone::Warning,
                "ℹ Rocket League is currently running. You must restart the game once to see boost changes.",
            );
        }
    });
}
