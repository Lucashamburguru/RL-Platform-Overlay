use crate::session::SessionOverlayDisplay;
use crate::state::{AppState, TeammateBoostDisplay};
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::app::SettingsTab;
use super::boost_hud::{
    draw_teammate_boost_panel, preview_teammates, teammate_boost_display_label,
};
use super::common::{
    StatusTone, debug_status_row, helper_text, setting_row, settings_section, settings_two_column,
    status_color, status_text,
};
use super::hotkeys::render_hotkey_settings_section;
use super::mmr_panel::render_local_mmr_panel;
use super::session_hud::{draw_session_panel, session_display_label};

pub(super) fn render_settings_tabs(
    ui: &mut egui::Ui,
    selected: &mut SettingsTab,
    debug_enabled: bool,
) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.selectable_value(selected, SettingsTab::Setup, "Setup");
        ui.selectable_value(selected, SettingsTab::Overlay, "Lobby");
        ui.selectable_value(selected, SettingsTab::Session, "Session");
        ui.selectable_value(selected, SettingsTab::Boost, "Boost");
        ui.selectable_value(selected, SettingsTab::Replays, "Replays");
        if debug_enabled {
            ui.selectable_value(selected, SettingsTab::Debug, "Debug");
        }
    });
    ui.add_space(8.0);
}

pub(super) fn render_update_notice(ui: &mut egui::Ui, state: &Arc<AppState>) {
    let version_check = state.version_check.load();
    if !version_check.update_available {
        return;
    }

    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(55, 46, 18))
        .stroke(egui::Stroke::new(
            1.0,
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
        ui.hyperlink_to("Download release", &version_check.release_url);
    });
    ui.add_space(6.0);
}

pub(super) fn render_setup_settings_tab(
    ui: &mut egui::Ui,
    _ctx: &egui::Context,
    state: &Arc<AppState>,
    config_edit: &mut crate::state::Config,
    changed: &mut bool,
    is_rl_running: bool,
) {
    settings_section(ui, "Stats API Setup", |ui| {
        setting_row(ui, "Rocket League Folder", |ui| {
            ui.horizontal(|ui| {
                let input_width = (ui.available_width() - 96.0).max(160.0);
                if ui
                    .add_sized(
                        [input_width, 22.0],
                        egui::TextEdit::singleline(&mut config_edit.rocket_league_path),
                    )
                    .changed()
                {
                    *changed = true;
                }
                if ui.button("Auto-detect").clicked()
                    && let Some(path) = crate::state::detect_rocket_league_path()
                {
                    config_edit.rocket_league_path = path;
                    *changed = true;
                }
            });
        });

        let status = crate::setup::inspect_stats_api_setup(&config_edit.rocket_league_path);
        ui.add_space(8.0);
        debug_status_row(ui, "Config File", &status.ini_path);
        debug_status_row(
            ui,
            "PacketSendRate",
            &status
                .packet_send_rate
                .map(|rate| rate.to_string())
                .unwrap_or_else(|| "missing".to_string()),
        );
        debug_status_row(
            ui,
            "Port",
            &status
                .port
                .map(|port| port.to_string())
                .unwrap_or_else(|| "49123 default".to_string()),
        );

        if status.configured {
            status_text(ui, StatusTone::Success, status.message);
        } else if status.exists {
            status_text(ui, StatusTone::Warning, status.message);
        } else {
            status_text(ui, StatusTone::Error, status.message);
        }

        if is_rl_running {
            status_text(
                ui,
                StatusTone::Warning,
                "Rocket League is running. Restart the game after changing this config.",
            );
        }

        ui.add_space(8.0);
        if ui
            .add_sized([140.0, 24.0], egui::Button::new("Enable Stats API"))
            .clicked()
        {
            match crate::setup::ensure_stats_api_setup(&config_edit.rocket_league_path) {
                Ok(result) => state.stats_api_setup_result.store(Arc::new(result)),
                Err(error) => state.stats_api_setup_result.store(Arc::new(
                    crate::setup::StatsApiSetupResult {
                        message: error,
                        ..Default::default()
                    },
                )),
            }
        }

        let result = state.stats_api_setup_result.load();
        if !result.message.is_empty() {
            ui.add_space(6.0);
            let tone = if result.changed {
                StatusTone::Success
            } else {
                StatusTone::Neutral
            };
            status_text(ui, tone, &result.message);
            if let Some(path) = &result.backup_path {
                debug_status_row(ui, "Backup", path);
            }
            if result.restart_required {
                status_text(
                    ui,
                    StatusTone::Warning,
                    "Restart Rocket League once before expecting the overlay to connect.",
                );
            }
        }
    });

    ui.add_space(10.0);
    render_positioning_settings_section(ui, config_edit, changed);
}

pub(super) fn render_overlay_settings_tab(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &Arc<AppState>,
    config: &crate::state::Config,
    config_edit: &mut crate::state::Config,
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
                    .changed()
                {
                    *changed = true;
                }
            });

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
                    .selected_text(super::lobby_overlay::lobby_theme_label(
                        config_edit.lobby_theme,
                    ))
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
                    .selected_text(super::lobby_overlay::lobby_display_mode_label(
                        config_edit.lobby_display_mode,
                    ))
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
        });
    });

    ui.add_space(10.0);
    render_hotkey_settings_section(ui, ctx, state, config_edit, changed);

    ui.add_space(10.0);
    settings_section(ui, "Live Preview", |ui| {
        let preview = super::lobby_overlay::preview_lobby_players(state);
        super::lobby_overlay::draw_lobby_panel(
            ui,
            &preview,
            config_edit,
            true,
            None,
            None,
            Some(config_edit.ui_scale.min(1.4)),
        );
    });
}

pub(super) fn render_session_settings_tab(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config_edit: &mut crate::state::Config,
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
                    != state.config.load().session_overlay_display
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
                    .changed()
                {
                    *changed = true;
                }
            });
        });
        settings_section(right, "Local MMR", |ui| {
            render_local_mmr_panel(ui, state);
        });
    });

    ui.add_space(10.0);
    settings_section(ui, "Preview", |ui| {
        draw_session_panel(
            ui,
            &state.session.load(),
            config_edit.session_overlay_scale.min(1.4),
            config_edit.session_overlay_display,
            config_edit.session_overlay_opacity,
        );
    });
}

pub(super) fn render_boost_settings_tab(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config_edit: &mut crate::state::Config,
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
                    let mut status = state.boost_swap_status.lock().unwrap();
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
                let mut status = state.boost_swap_status.lock().unwrap();
                *status = "Error: Configure your Rocket League path first.".to_string();
            } else if path_valid != Some(true) {
                let mut status = state.boost_swap_status.lock().unwrap();
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
                let mut status = state.boost_swap_status.lock().unwrap();
                *status = "Error: Configure your Rocket League path first.".to_string();
            } else if path_valid != Some(true) {
                let mut status = state.boost_swap_status.lock().unwrap();
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
        let status = state.boost_swap_status.lock().unwrap().clone();
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

fn render_positioning_settings_section(
    ui: &mut egui::Ui,
    config_edit: &mut crate::state::Config,
    changed: &mut bool,
) {
    settings_section(ui, "Overlay Positioning", |ui| {
        if ui
            .checkbox(&mut config_edit.layout_mode, "Enable Drag Positioning")
            .changed()
        {
            *changed = true;
        }

        if config_edit.layout_mode {
            status_text(
                ui,
                StatusTone::Warning,
                "Drag the visible panels into place. Drag positioning will automatically turn off when settings are closed to restore game click-through.",
            );
        }

        ui.horizontal_wrapped(|ui| {
            if ui.button("Reset Lobby").clicked() {
                config_edit.lobby_manual_position = None;
                *changed = true;
            }
            if ui.button("Reset Boost").clicked() {
                config_edit.teammate_boost_manual_position = None;
                *changed = true;
            }
            if ui.button("Reset Session").clicked() {
                config_edit.session_manual_position = None;
                *changed = true;
            }
        });
    });
}

pub(super) fn render_launch_controls(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &Arc<AppState>,
    is_launched: bool,
    config_edit: &mut crate::state::Config,
    changed: &mut bool,
) {
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
            let new_val = !is_launched;
            state.is_launched.store(new_val, Ordering::SeqCst);
            if new_val {
                state.is_settings_visible.store(false, Ordering::SeqCst);
            }
        }

        ui.add_space(10.0);
        let is_visible = state.is_visible.load(Ordering::SeqCst);
        ui.horizontal(|ui| {
            ui.label("HUD:");
            if is_visible || is_launched {
                ui.colored_label(status_color(StatusTone::Success), "ACTIVE");
            } else {
                ui.colored_label(status_color(StatusTone::Error), "HIDDEN");
            }
        });

        ui.add_space(8.0);
        if ui
            .checkbox(&mut config_edit.layout_mode, "Drag Position")
            .changed()
        {
            *changed = true;
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
                let default_config = crate::state::Config::default();
                state.save_config(default_config);
            }
            ui.label(
                egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .size(9.0)
                    .color(egui::Color32::from_gray(100)),
            );
        });
    });
}

pub(super) fn render_replays_settings_tab(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config_edit: &mut crate::state::Config,
    changed: &mut bool,
) {
    settings_section(ui, "Ballchasing.com Replay Uploader", |ui| {
        if ui
            .checkbox(&mut config_edit.ballchasing_enabled, "Enable Auto-Upload")
            .changed()
        {
            *changed = true;
        }

        ui.add_space(6.0);

        // API Key Section
        setting_row(ui, "API Key", |ui| {
            let show_key_id = ui.make_persistent_id("show_bc_api_key");
            let mut show_key = ui.data(|d| d.get_temp::<bool>(show_key_id).unwrap_or(false));

            let input_width = (ui.available_width() - 58.0).max(160.0);
            let response = if show_key {
                ui.add_sized(
                    [input_width, 22.0],
                    egui::TextEdit::singleline(&mut config_edit.ballchasing_api_key),
                )
            } else {
                ui.add_sized(
                    [input_width, 22.0],
                    egui::TextEdit::singleline(&mut config_edit.ballchasing_api_key).password(true),
                )
            };

            if response.changed() {
                *changed = true;
            }

            if ui.checkbox(&mut show_key, "Show").changed() {
                ui.data_mut(|d| d.insert_temp(show_key_id, show_key));
            }
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(helper_text("Get your API key at:"));
            ui.hyperlink_to("ballchasing.com/upload", "https://ballchasing.com/upload");
        });

        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label(helper_text(
                "Free tier quotas: 20 uploads/day, 70/week. To get higher limits, support them on:",
            ));
            ui.hyperlink_to("Patreon", "https://www.patreon.com/ballchasing");
        });

        ui.add_space(8.0);

        // Verify key button
        let verify_status_id = ui.make_persistent_id("bc_verify_status");
        let verify_status = ui.data(|d| {
            d.get_temp::<String>(verify_status_id)
                .unwrap_or_else(|| "".to_string())
        });

        ui.horizontal(|ui| {
            if ui.button("Verify Token").clicked() {
                let api_key = config_edit.ballchasing_api_key.trim().to_string();
                let ui_ctx = ui.ctx().clone();

                ui.data_mut(|d| d.insert_temp(verify_status_id, "Checking...".to_string()));

                tokio::spawn(async move {
                    let result = crate::replays::verify_token(&api_key).await;
                    let msg = match result {
                        Ok(()) => "✔ Token Valid".to_string(),
                        Err(e) => format!("❌ Invalid: {}", e),
                    };
                    ui_ctx.data_mut(|d| d.insert_temp(verify_status_id, msg));
                });
            }

            if !verify_status.is_empty() {
                let color = if verify_status.starts_with("✔") {
                    egui::Color32::from_rgb(100, 220, 100)
                } else if verify_status.starts_with("Checking") {
                    egui::Color32::from_gray(160)
                } else {
                    egui::Color32::from_rgb(230, 80, 80)
                };
                ui.colored_label(color, &verify_status);
            }
        });

        ui.add_space(10.0);

        // Visibility Preference
        setting_row(ui, "Replay Visibility", |ui| {
            egui::ComboBox::new("bc_visibility", "")
                .selected_text(match config_edit.ballchasing_visibility.as_str() {
                    "public" => "Public",
                    "unlisted" => "Unlisted",
                    "private" => "Private",
                    _ => "Public",
                })
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_value(
                            &mut config_edit.ballchasing_visibility,
                            "public".to_string(),
                            "Public",
                        )
                        .clicked()
                    {
                        *changed = true;
                    }
                    if ui
                        .selectable_value(
                            &mut config_edit.ballchasing_visibility,
                            "unlisted".to_string(),
                            "Unlisted",
                        )
                        .clicked()
                    {
                        *changed = true;
                    }
                    if ui
                        .selectable_value(
                            &mut config_edit.ballchasing_visibility,
                            "private".to_string(),
                            "Private",
                        )
                        .clicked()
                    {
                        *changed = true;
                    }
                });
        });

        ui.add_space(10.0);

        // Replays Directory
        setting_row(ui, "Replay Folder", |ui| {
            ui.horizontal(|ui| {
                let input_width = (ui.available_width() - 96.0).max(160.0);
                if ui
                    .add_sized(
                        [input_width, 22.0],
                        egui::TextEdit::singleline(&mut config_edit.replays_folder),
                    )
                    .changed()
                {
                    *changed = true;
                }
                if ui.button("Auto-detect").clicked()
                    && let Some(detected) = crate::state::detect_replays_path()
                {
                    config_edit.replays_folder = detected;
                    *changed = true;
                }
            });
        });

        // Folder Path Validation
        let path_valid = if config_edit.replays_folder.trim().is_empty() {
            None
        } else {
            let path = std::path::Path::new(&config_edit.replays_folder);
            Some(path.exists() && path.is_dir())
        };

        match path_valid {
            Some(true) => {
                status_text(ui, StatusTone::Success, "✔ Valid replay directory.");
            }
            Some(false) => {
                status_text(ui, StatusTone::Error, "❌ Directory not found.");
            }
            None => {
                status_text(
                    ui,
                    StatusTone::Warning,
                    "⚠ Path unconfigured. Click Auto-detect.",
                );
            }
        }

        ui.add_space(6.0);
        status_text(
            ui,
            StatusTone::Warning,
            "⚠ Note: Bulk uploading is rate-limited (30s delay per file) to respect Ballchasing.com limits.",
        );
        ui.add_space(6.0);

        // Sync and Upload buttons
        ui.horizontal(|ui| {
            let api_key_empty = config_edit.ballchasing_api_key.trim().is_empty();
            let path_invalid = path_valid != Some(true);

            // Upload Existing
            let upload_btn = ui.add_enabled(
                !api_key_empty && !path_invalid,
                egui::Button::new("Upload Existing Replays"),
            );
            if upload_btn.clicked() {
                crate::replays::start_bulk_upload_task(state.clone());
            }

            // Sync Cache
            let sync_btn = ui.add_enabled(!api_key_empty, egui::Button::new("Sync Uploaded Cache"));
            if sync_btn.clicked() {
                crate::replays::start_sync_replays_task(state.clone());
            }

            // Clear Cache
            let clear_btn = ui.button("Clear Upload Cache");
            if clear_btn.clicked() {
                config_edit.uploaded_replays.clear();
                *changed = true;
                if let Ok(mut status) = state.ballchasing_status.lock() {
                    *status = "Upload cache cleared.".to_string();
                }
            }
        });

        ui.add_space(8.0);

        // Display Cloud Count & Local Cache
        let cloud_count = state
            .ballchasing_cloud_count
            .load(std::sync::atomic::Ordering::SeqCst);
        if cloud_count > 0 {
            ui.label(format!("Replays on Ballchasing.com: {}", cloud_count));
            ui.add_space(4.0);
        }

        let cached_count = config_edit.uploaded_replays.len();
        if cached_count > 0 {
            egui::CollapsingHeader::new(format!("Locally Cached Uploads ({} files)", cached_count))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(100.0)
                        .show(ui, |ui| {
                            for filename in &config_edit.uploaded_replays {
                                ui.label(
                                    egui::RichText::new(filename)
                                        .font(egui::FontId::monospace(9.0))
                                        .color(egui::Color32::from_gray(160)),
                                );
                            }
                        });
                });
            ui.add_space(4.0);
        }

        ui.separator();
        ui.add_space(6.0);

        // Status Indicator
        let current_status = if let Ok(status) = state.ballchasing_status.lock() {
            status.clone()
        } else {
            "Idle".to_string()
        };

        setting_row(ui, "Uploader Status", |ui| {
            let tone = if current_status.starts_with("Success") {
                StatusTone::Success
            } else if current_status.starts_with("Error") {
                StatusTone::Error
            } else if current_status.contains("Uploading") || current_status.contains("Checking") {
                StatusTone::Warning
            } else {
                StatusTone::Neutral
            };
            status_text(ui, tone, &current_status);
        });
    });

    ui.add_space(10.0);

    settings_section(ui, "Hoops Replay Fixer", |ui| {
        ui.label("Fixes legacy/broken Rocket League Hoops replays in your folder by patching old mutator, stadium, and goal volume tags. Backups (.replay.bak) are automatically saved before patching.");

        ui.add_space(8.0);

        // Path validation feedback
        let folder_str = config_edit.replays_folder.trim();
        let path_valid = if folder_str.is_empty() {
            None
        } else {
            let path = std::path::Path::new(folder_str);
            Some(path.exists() && path.is_dir())
        };

        ui.horizontal(|ui| {
            let scan_btn = ui.add_enabled(
                path_valid == Some(true),
                egui::Button::new("Scan & Fix Replays Folder"),
            );
            if scan_btn.clicked() {
                crate::hoops_fixer::start_folder_fix_task(state.clone());
            }

            let restore_btn = ui.add_enabled(
                path_valid == Some(true),
                egui::Button::new("Restore Backups"),
            );
            if restore_btn.clicked() {
                crate::hoops_fixer::start_restore_backups_task(state.clone());
            }

            let delete_btn = ui.add_enabled(
                path_valid == Some(true),
                egui::Button::new("Delete Backups"),
            );
            if delete_btn.clicked() {
                crate::hoops_fixer::start_delete_backups_task(state.clone());
            }
        });

        // Status Indicator
        let fixer_status = if let Ok(status) = state.hoops_fixer_status.lock() {
            status.clone()
        } else {
            "Idle".to_string()
        };

        ui.add_space(6.0);
        setting_row(ui, "Fixer Status", |ui| {
            let tone = if fixer_status.starts_with("Success") {
                StatusTone::Success
            } else if fixer_status.starts_with("Error") {
                StatusTone::Error
            } else if fixer_status.contains("Scanning") || fixer_status.contains("Checking") {
                StatusTone::Warning
            } else {
                StatusTone::Neutral
            };
            status_text(ui, tone, &fixer_status);
        });

        // Output Logs Box
        let logs = if let Ok(l) = state.hoops_fixer_logs.lock() {
            l.clone()
        } else {
            Vec::new()
        };

        if !logs.is_empty() {
            ui.add_space(8.0);
            ui.label("Fixer Logs:");
            egui::ScrollArea::vertical()
                .max_height(120.0)
                .show(ui, |ui| {
                    for log_line in &logs {
                        ui.label(
                            egui::RichText::new(log_line)
                                .font(egui::FontId::monospace(10.0))
                                .color(if log_line.starts_with("✔") {
                                    egui::Color32::from_rgb(120, 220, 120)
                                } else if log_line.contains("❌") {
                                    egui::Color32::from_rgb(220, 120, 120)
                                } else {
                                    egui::Color32::from_gray(170)
                                }),
                        );
                    }
                });
        }
    });
}
