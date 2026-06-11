use crate::session::SessionOverlayDisplay;
use crate::state::{AppState, Config};
use crate::ui::common::{setting_row, settings_section, settings_two_column};
use crate::ui::mmr_panel::render_local_mmr_panel;
use crate::ui::session_hud::{SessionHudOptions, draw_session_panel, session_display_label};
use eframe::egui;
use std::sync::Arc;

pub(crate) fn render_session_settings_tab(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config_edit: &mut Config,
    changed: &mut bool,
) {
    settings_two_column(ui, |left, right| {
        settings_section(left, "Session Overlay", |ui| {
            if ui
                .checkbox(
                    &mut config_edit.session_overlay_enabled,
                    "Enable Session Overlay",
                )
                .changed()
            {
                *changed = true;
            }

            ui.add_space(8.0);
            setting_row(ui, "Hotkey", |ui| {
                if ui
                    .checkbox(
                        &mut config_edit.session_overlay_follow_lobby_hotkey,
                        "Show with lobby overlay hotkey.",
                    )
                    .changed()
                {
                    *changed = true;
                }
            });

            ui.add_space(8.0);
            setting_row(ui, "Display", |ui| {
                egui::ComboBox::new("session_display", "")
                    .selected_text(session_display_label(config_edit.session_overlay_display))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut config_edit.session_overlay_display,
                            SessionOverlayDisplay::Compact,
                            "Compact",
                        );
                        ui.selectable_value(
                            &mut config_edit.session_overlay_display,
                            SessionOverlayDisplay::Expanded,
                            "Expanded",
                        );
                    });
                if config_edit.session_overlay_display
                    != state.system.config.load().session_overlay_display
                {
                    *changed = true;
                }
            });

            ui.add_space(8.0);
            setting_row(ui, "Scale", |ui| {
                if ui
                    .add_sized(
                        [ui.available_width(), 20.0],
                        egui::Slider::new(&mut config_edit.session_overlay_scale, 0.6..=2.5),
                    )
                    .on_hover_text(
                        "Adjust the sizing scale of the session tracking HUD (0.6x to 2.5x)",
                    )
                    .changed()
                {
                    *changed = true;
                }
            });

            ui.add_space(8.0);
            setting_row(ui, "Opacity", |ui| {
                if ui
                    .add_sized(
                        [ui.available_width(), 20.0],
                        egui::Slider::new(&mut config_edit.session_overlay_opacity, 40..=255),
                    )
                    .on_hover_text("Adjust the opacity of the session panel background (40 to 255)")
                    .changed()
                {
                    *changed = true;
                }
            });

            ui.add_space(8.0);
            setting_row(ui, "Expanded", |ui| {
                ui.vertical(|ui| {
                    if ui
                        .checkbox(
                            &mut config_edit.session_expanded_show_streaks,
                            "Streaks & Stats",
                        )
                        .changed()
                    {
                        *changed = true;
                    }
                    if ui
                        .checkbox(
                            &mut config_edit.session_expanded_show_breakdown,
                            "Mode Breakdown",
                        )
                        .changed()
                    {
                        *changed = true;
                    }
                    if ui
                        .checkbox(
                            &mut config_edit.session_expanded_show_mmr_delta,
                            "MMR Change",
                        )
                        .changed()
                    {
                        *changed = true;
                    }
                });
            });

            if config_edit.session_manual_position.is_some() {
                ui.add_space(8.0);
                if ui.button("Reset Session HUD Position").clicked() {
                    config_edit.session_manual_position = None;
                    *changed = true;
                }
            }
        });
        settings_section(right, "Local MMR", |ui| {
            render_local_mmr_panel(ui, state, config_edit, changed);
        });
    });

    ui.add_space(10.0);
    settings_section(ui, "Preview", |ui| {
        let local_mmr = state.mmr.local_mmr.load();
        draw_session_panel(
            ui,
            &state.game.session.load(),
            &local_mmr,
            config_edit.session_overlay_scale.min(1.4),
            config_edit.session_overlay_display,
            config_edit.session_overlay_opacity,
            SessionHudOptions {
                show_streaks: config_edit.session_expanded_show_streaks,
                show_breakdown: config_edit.session_expanded_show_breakdown,
                show_mmr_delta: config_edit.session_expanded_show_mmr_delta,
            },
        );
    });
}
