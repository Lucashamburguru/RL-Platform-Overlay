use crate::state::{AppState, Config};
use crate::ui::common::{setting_row, settings_section, settings_two_column};
use crate::ui::lobby_overlay::{
    draw_lobby_panel, lobby_display_mode_label, lobby_theme_label, preview_lobby_players,
};
use eframe::egui;
use std::sync::Arc;

pub(crate) fn render_overlay_settings_tab(
    ui: &mut egui::Ui,
    _ctx: &egui::Context,
    state: &Arc<AppState>,
    config: &Config,
    config_edit: &mut Config,
    changed: &mut bool,
    _is_launched: bool,
) {
    settings_section(ui, "Lobby Overlay Settings", |ui| {
        settings_two_column(ui, |column, is_right| {
            if !is_right {
                let left = column;
                left.heading("Appearance");
                setting_row(left, "Opacity", |ui| match config_edit.lobby_theme {
                    crate::state::LobbyTheme::Solid => {
                        ui.label("100% — Solid theme uses an opaque background.");
                    }
                    crate::state::LobbyTheme::Minimalist => {
                        ui.label("0% — Minimalist theme has no background.");
                    }
                    theme => {
                        let minimum = if theme == crate::state::LobbyTheme::Modern {
                            220
                        } else {
                            0
                        };
                        let mut effective = config_edit.transparency.max(minimum);
                        if crate::ui::common::opacity_slider(ui, &mut effective, minimum).on_hover_text("Background opacity. Modern Cyber retains a minimum of 86% for readability.").changed() {
                                config_edit.transparency = effective;
                                *changed = true;
                            }
                    }
                });

                left.add_space(8.0);
                setting_row(left, "HUD Scale", |ui| {
                    if crate::ui::common::scale_slider(ui, &mut config_edit.ui_scale, 0.5, 2.5)
                        .on_hover_text(
                            "Adjust the sizing scale of the lobby overlay HUD (0.5x to 2.5x)",
                        )
                        .changed()
                    {
                        *changed = true;
                    }
                });

                if config_edit.lobby_manual_position.is_some() {
                    left.add_space(8.0);
                    if left.button("Reset Lobby Overlay Position").clicked() {
                        config_edit.lobby_manual_position = None;
                        *changed = true;
                    }
                }

                left.add_space(8.0);
                setting_row(left, "Resolution", |ui| {
                    let res_text = format!(
                        "{}x{}",
                        config_edit.window_size[0], config_edit.window_size[1]
                    );
                    egui::ComboBox::new("res_presets", "")
                        .selected_text(res_text)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut config_edit.window_size,
                                [1920.0, 1080.0],
                                "1080p",
                            );
                            ui.selectable_value(
                                &mut config_edit.window_size,
                                [2560.0, 1440.0],
                                "1440p",
                            );
                            ui.selectable_value(
                                &mut config_edit.window_size,
                                [3840.0, 2160.0],
                                "4K",
                            );
                        });
                    if config_edit.window_size != config.window_size {
                        *changed = true;
                    }
                });
            } else {
                let right = column;
                setting_row(right, "Theme", |ui| {
                    egui::ComboBox::new("lobby_theme", "")
                        .selected_text(lobby_theme_label(config_edit.lobby_theme))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut config_edit.lobby_theme,
                                crate::state::LobbyTheme::Glass,
                                "Glassmorphism",
                            );
                            ui.selectable_value(
                                &mut config_edit.lobby_theme,
                                crate::state::LobbyTheme::Solid,
                                "High-Contrast Solid",
                            );
                            ui.selectable_value(
                                &mut config_edit.lobby_theme,
                                crate::state::LobbyTheme::Modern,
                                "Modern Cyber",
                            );
                            ui.selectable_value(
                                &mut config_edit.lobby_theme,
                                crate::state::LobbyTheme::Minimalist,
                                "Minimalist Floating",
                            );
                        });
                    if config_edit.lobby_theme != config.lobby_theme {
                        *changed = true;
                    }
                });

                right.add_space(8.0);
                setting_row(right, "Display Mode", |ui| {
                    egui::ComboBox::new("lobby_display_mode", "")
                        .selected_text(lobby_display_mode_label(config_edit.lobby_display_mode))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut config_edit.lobby_display_mode,
                                crate::state::LobbyDisplayMode::Compact,
                                "Compact",
                            );
                            ui.selectable_value(
                                &mut config_edit.lobby_display_mode,
                                crate::state::LobbyDisplayMode::Expanded,
                                "Expanded",
                            );
                        });
                    if config_edit.lobby_display_mode != config.lobby_display_mode {
                        *changed = true;
                    }
                });

                right.add_space(8.0);
                right.heading("Player information");
                setting_row(right, "Players", |ui| {
                    if ui
                        .checkbox(&mut config_edit.show_bots, "Show Bots")
                        .changed()
                    {
                        *changed = true;
                    }
                });

                right.add_space(8.0);
                setting_row(right, "Stats", |ui| {
                    if ui
                        .checkbox(&mut config_edit.show_stats, "Show Player Stats")
                        .changed()
                    {
                        *changed = true;
                    }
                });

                right.add_space(8.0);
                setting_row(right, "Matches", |ui| {
                    if ui
                        .checkbox(&mut config_edit.show_lobby_matches, "Show Match Counts")
                        .changed()
                    {
                        *changed = true;
                    }
                });

                right.add_space(8.0);
                setting_row(right, "Ranks", |ui| {
                    if ui
                        .checkbox(&mut config_edit.show_lobby_ranks, "Show Ranks & MMR")
                        .changed()
                    {
                        *changed = true;
                    }
                });

                #[cfg(not(feature = "microsoft-store"))]
                right.heading("Automation");
                #[cfg(not(feature = "microsoft-store"))]
                setting_row(right, "Auto Chat", |ui| {
                    if ui
                        .checkbox(&mut config_edit.auto_gg, "Auto 'GG' at Match End")
                        .changed()
                    {
                        *changed = true;
                    }
                });

                #[cfg(not(feature = "microsoft-store"))]
                right.add_space(8.0);
                #[cfg(not(feature = "microsoft-store"))]
                setting_row(right, "GG Keys", |ui| {
                    if ui
                    .text_edit_singleline(&mut config_edit.auto_gg_sequence)
                    .on_hover_text("Key sequence format: keys separated by commas (e.g. T,G,G,Enter). Supports delay tokens like Delay400 or Wait200.")
                    .changed()
                {
                    *changed = true;
                }
                });
                #[cfg(not(feature = "microsoft-store"))]
                right.label("Separate keys with commas, e.g. T,G,G,Enter. Delay400 waits 400 ms.");

                #[cfg(not(feature = "microsoft-store"))]
                right.add_space(8.0);
                #[cfg(not(feature = "microsoft-store"))]
                setting_row(right, "Auto Play", |ui| {
                    if ui
                        .checkbox(
                            &mut config_edit.auto_freeplay,
                            "Auto Free Play at Match End",
                        )
                        .changed()
                    {
                        *changed = true;
                    }
                });
            }
        });
    });

    ui.add_space(8.0);
    egui::CollapsingHeader::new("Preview")
        .id_salt("lobby_settings_preview")
        .default_open(false)
        .show(ui, |ui| {
        ui.label("Sample layout with available player data • reduced to fit. Percentages show boost; TCH = touches, BMP = bumps, DEM = demos.");
        let preview = preview_lobby_players(state);
        let session = state.game.session.load();
        let local_identity = state.game.local_player_identity.load();
        let local_player_name = state.game.local_player_name.load();
        let local_mmr = state.mmr.local_mmr.load();
        draw_lobby_panel(
            ui,
            &preview,
            config_edit,
            true,
            Some(&local_identity),
            Some(local_player_name.as_str()),
            local_mmr.current.as_ref(),
            session.active_mode,
            None,
            Some(
                config_edit
                    .ui_scale
                    .min(1.4)
                    .min((ui.available_width() - 24.0) / 350.0),
            ),
        );
        });
}
