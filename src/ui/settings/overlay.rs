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
        settings_two_column(ui, |left, right| {
            setting_row(left, "Transparency", |ui| {
                if ui
                    .add_sized(
                        [ui.available_width(), 20.0],
                        egui::Slider::new(&mut config_edit.transparency, 0..=255),
                    )
                    .on_hover_text("Adjust the transparency of the lobby overlay panel (0 = fully transparent, 255 = fully solid)")
                    .changed()
                {
                    *changed = true;
                }
            });

            left.add_space(8.0);
            setting_row(left, "HUD Scale", |ui| {
                if ui
                    .add_sized(
                        [ui.available_width(), 20.0],
                        egui::Slider::new(&mut config_edit.ui_scale, 0.5..=2.5),
                    )
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
                        ui.selectable_value(&mut config_edit.window_size, [3840.0, 2160.0], "4K");
                    });
                if config_edit.window_size != config.window_size {
                    *changed = true;
                }
            });

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

            right.add_space(8.0);
            setting_row(right, "Auto Chat", |ui| {
                if ui
                    .checkbox(&mut config_edit.auto_gg, "Auto 'GG' at Match End")
                    .changed()
                {
                    *changed = true;
                }
            });

            right.add_space(8.0);
            setting_row(right, "GG Keys", |ui| {
                if ui
                    .text_edit_singleline(&mut config_edit.auto_gg_sequence)
                    .on_hover_text("Key sequence format: keys separated by commas (e.g. T,G,G,Enter). Supports delay tokens like Delay400 or Wait200.")
                    .changed()
                {
                    *changed = true;
                }
            });

            right.add_space(8.0);
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
        });
    });

    ui.add_space(10.0);
    settings_section(ui, "Live Preview", |ui| {
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
            Some(config_edit.ui_scale.min(1.4)),
        );
    });
}
