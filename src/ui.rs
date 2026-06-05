use crate::session::SessionOverlayDisplay;
use crate::state::{AnchorPos, AppState, DebugCaptureStatus, TeammateBoostDisplay};
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub struct MainApp {
    state: Arc<AppState>,
    settings_tab: SettingsTab,
    is_rl_running: bool,
    last_rl_check: std::time::Instant,
    last_logged_show_settings: Option<bool>,
}

impl MainApp {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            settings_tab: SettingsTab::Overlay,
            is_rl_running: false,
            last_rl_check: std::time::Instant::now()
                .checked_sub(std::time::Duration::from_secs(5))
                .unwrap_or_else(std::time::Instant::now),
            last_logged_show_settings: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsTab {
    Setup,
    Overlay,
    Session,
    Boost,
    Debug,
}

impl eframe::App for MainApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let is_launched = self.state.is_launched.load(Ordering::SeqCst);
        let config = self.state.config.load();

        // 1. Unified Background (Transparent)
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 0)))
            .show(ctx, |_ui| {
                let show_settings = self.state.is_settings_visible.load(Ordering::SeqCst)
                    || self.state.is_recording_kb.load(Ordering::SeqCst)
                    || self.state.is_recording_ctrl.load(Ordering::SeqCst)
                    || self.state.is_recording_settings.load(Ordering::SeqCst);
                let show_hud = is_launched
                    && (self.state.is_visible.load(Ordering::SeqCst) || config.layout_mode);

                if show_hud {
                    render_overlay(ctx, &self.state);
                }
                if is_launched && config.session_overlay_enabled {
                    render_session_overlay(ctx, &self.state);
                }

                if self.last_logged_show_settings != Some(show_settings) {
                    crate::input::append_hotkey_debug_log(format!(
                        "ui_show_settings visible={show_settings} launched={is_launched} recording_kb={} recording_ctrl={} recording_settings={}",
                        self.state.is_recording_kb.load(Ordering::SeqCst),
                        self.state.is_recording_ctrl.load(Ordering::SeqCst),
                        self.state.is_recording_settings.load(Ordering::SeqCst)
                    ));
                    self.last_logged_show_settings = Some(show_settings);
                }

                // 2. Always-on Teammate Boost HUD
                // Settings mode uses the Boost tab preview instead of the floating in-game HUD.
                if is_launched && config.show_teammate_boost && config.layout_mode {
                    render_teammate_boost_position_preview(ctx, &self.state, true);
                } else if is_launched && config.show_teammate_boost && !show_settings {
                    render_teammate_boost(ctx, &self.state);
                } else if show_settings
                    && self.settings_tab == SettingsTab::Boost
                    && config.show_teammate_boost
                {
                    render_teammate_boost_position_preview(ctx, &self.state, false);
                }

                // 3. Settings UI (Floating Window)

                // Keep window on top every frame when launched
                if is_launched {
                    ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                        egui::WindowLevel::AlwaysOnTop,
                    ));

                    // If settings are visible, we need to be able to click them!
                    // If settings are hidden, we want clicks to pass through to the game.
                    ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(
                        !show_settings && !config.layout_mode,
                    ));
                }

                // Show gear icon ONLY if launched AND settings are hidden AND mouse is in top-left
                if is_launched && !show_settings {
                    let mouse_pos = ctx.input(|i| {
                        i.pointer
                            .interact_pos()
                            .unwrap_or(egui::Pos2::new(-100.0, -100.0))
                    });
                    let gear_rect =
                        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0));

                    if gear_rect.contains(mouse_pos) {
                        egui::Area::new("settings_toggle".into())
                            .anchor(egui::Align2::LEFT_TOP, egui::vec2(10.0, 10.0))
                            .show(ctx, |ui| {
                                let btn = ui.add(egui::Button::new("⚙ Settings").frame(true));
                                if btn.clicked() {
                                    crate::input::append_hotkey_debug_log(
                                        "gear_settings_button_clicked visible=true",
                                    );
                                    self.state.is_settings_visible.store(true, Ordering::SeqCst);
                                }
                            });
                    }
                }

                let settings_hotkey = config.hotkey_settings.clone();
                let hud_hotkey = config.hotkey_kb.clone();
                let hotkey_toggle = config.hotkey_toggle;
                ctx.input(|i| {
                    for event in &i.events {
                        if let egui::Event::Key { key, pressed, .. } = event
                            && let Some(name) = egui_to_rdev_key(*key)
                        {
                            if *pressed && name == settings_hotkey {
                                crate::input::append_hotkey_debug_log(format!(
                                    "egui_keypress key={name} settings_match=true"
                                ));
                                crate::input::toggle_settings_hotkey(&self.state, "egui");
                            }

                            if show_settings && name == hud_hotkey {
                                if hotkey_toggle {
                                    if *pressed {
                                        let curr = self.state.is_visible.load(Ordering::SeqCst);
                                        self.state.is_visible.store(!curr, Ordering::SeqCst);
                                    }
                                } else {
                                    self.state.is_visible.store(*pressed, Ordering::SeqCst);
                                }
                            }
                        }
                    }
                });

                if show_settings {
                    let mut settings_open = true;
                    egui::Window::new("RL Overlay Settings")
                        .collapsible(true)
                        .resizable(true)
                        .movable(true)
                        .default_pos([16.0, 16.0])
                        .default_size(if is_launched {
                            [450.0, 600.0]
                        } else {
                            [680.0, 760.0]
                        })
                        .min_width(420.0)
                        .min_height(520.0)
                        .constrain_to(ctx.screen_rect().shrink(8.0))
                        .open(&mut settings_open)
                        .show(ctx, |ui| {
                            ui.add_space(5.0);

                            let mut config_edit = (**self.state.config.load()).clone();
                            let mut changed = false;
                            if !self.state.debug_enabled
                                && self.settings_tab == SettingsTab::Debug
                            {
                                self.settings_tab = SettingsTab::Setup;
                            }

                            render_update_notice(ui, &self.state);
                            render_settings_tabs(
                                ui,
                                &mut self.settings_tab,
                                self.state.debug_enabled,
                            );

                            egui::ScrollArea::vertical().show(ui, |ui| match self.settings_tab {
                                SettingsTab::Setup => render_setup_settings_tab(
                                    ui,
                                    &self.state,
                                    &mut config_edit,
                                    &mut changed,
                                    self.is_rl_running,
                                ),
                                SettingsTab::Overlay => render_overlay_settings_tab(
                                    ui,
                                    ctx,
                                    &self.state,
                                    &config,
                                    &mut config_edit,
                                    &mut changed,
                                    is_launched,
                                ),
                                SettingsTab::Session => render_session_settings_tab(
                                    ui,
                                    &self.state,
                                    &mut config_edit,
                                    &mut changed,
                                ),
                                SettingsTab::Boost => {
                                    let now = std::time::Instant::now();
                                    if now.duration_since(self.last_rl_check).as_secs() >= 2 {
                                        self.is_rl_running =
                                            crate::assets::is_rocket_league_running();
                                        self.last_rl_check = now;
                                    }
                                    render_boost_settings_tab(
                                        ui,
                                        &self.state,
                                        &mut config_edit,
                                        &mut changed,
                                        self.is_rl_running,
                                    )
                                }
                                SettingsTab::Debug => {
                                    render_debug_settings_tab(ui, &self.state, is_launched)
                                }
                            });

                            if changed {
                                self.state.save_config(config_edit);
                            }
                        });
                    if !settings_open {
                        crate::input::append_hotkey_debug_log(
                            "settings_window_close_clicked visible=false",
                        );
                        self.state
                            .is_settings_visible
                            .store(false, Ordering::SeqCst);
                    }
                }
            });

        ctx.request_repaint();
    }
}

fn render_settings_tabs(ui: &mut egui::Ui, selected: &mut SettingsTab, debug_enabled: bool) {
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(selected, SettingsTab::Setup, "Setup");
        ui.selectable_value(selected, SettingsTab::Overlay, "Overlay");
        ui.selectable_value(selected, SettingsTab::Session, "Session");
        ui.selectable_value(selected, SettingsTab::Boost, "Boost");
        if debug_enabled {
            ui.selectable_value(selected, SettingsTab::Debug, "Debug");
        }
    });
    ui.separator();
}

fn render_update_notice(ui: &mut egui::Ui, state: &Arc<AppState>) {
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

fn render_setup_settings_tab(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config_edit: &mut crate::state::Config,
    changed: &mut bool,
    is_rl_running: bool,
) {
    ui.group(|ui| {
        ui.heading("Stats API Setup");
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label("Rocket League Folder:");
            if ui
                .text_edit_singleline(&mut config_edit.rocket_league_path)
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

        let status = crate::setup::inspect_stats_api_setup(&config_edit.rocket_league_path);
        ui.add_space(6.0);
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
            ui.colored_label(egui::Color32::from_rgb(100, 220, 100), status.message);
        } else if status.exists {
            ui.colored_label(egui::Color32::from_rgb(220, 190, 90), status.message);
        } else {
            ui.colored_label(egui::Color32::from_rgb(230, 120, 80), status.message);
        }

        if is_rl_running {
            ui.colored_label(
                egui::Color32::from_rgb(220, 200, 100),
                "Rocket League is running. Restart the game after changing this config.",
            );
        }

        ui.add_space(8.0);
        if ui.button("Enable Stats API").clicked() {
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
            let color = if result.changed {
                egui::Color32::from_rgb(100, 220, 100)
            } else {
                egui::Color32::from_gray(180)
            };
            ui.colored_label(color, &result.message);
            if let Some(path) = &result.backup_path {
                debug_status_row(ui, "Backup", path);
            }
            if result.restart_required {
                ui.colored_label(
                    egui::Color32::from_rgb(220, 200, 100),
                    "Restart Rocket League once before expecting the overlay to connect.",
                );
            }
        }
    });
}

fn render_overlay_settings_tab(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &Arc<AppState>,
    config: &crate::state::Config,
    config_edit: &mut crate::state::Config,
    changed: &mut bool,
    is_launched: bool,
) {
    ui.group(|ui| {
        ui.label("Transparency");
        if ui
            .add(egui::Slider::new(&mut config_edit.transparency, 0..=255))
            .changed()
        {
            *changed = true;
        }

        ui.label("HUD Scale");
        if ui
            .add(egui::Slider::new(&mut config_edit.ui_scale, 0.5..=2.5))
            .changed()
        {
            *changed = true;
        }

        ui.horizontal(|ui| {
            ui.label("Resolution:");
            let res_text = format!(
                "{}x{}",
                config_edit.window_size[0], config_edit.window_size[1]
            );
            egui::ComboBox::new("res_presets", "")
                .selected_text(res_text)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut config_edit.window_size, [1920.0, 1080.0], "1080p");
                    ui.selectable_value(&mut config_edit.window_size, [2560.0, 1440.0], "1440p");
                    ui.selectable_value(&mut config_edit.window_size, [3840.0, 2160.0], "4K");
                });
            if config_edit.window_size != config.window_size {
                *changed = true;
            }
        });

        ui.horizontal(|ui| {
            ui.label("Monitor:");
            egui::ComboBox::new("monitor_select", "")
                .selected_text(format!("Monitor {}", config_edit.monitor_index))
                .show_ui(ui, |ui| {
                    for i in 0..4 {
                        ui.selectable_value(
                            &mut config_edit.monitor_index,
                            i,
                            format!("Monitor {}", i),
                        );
                    }
                });
            if config_edit.monitor_index != config.monitor_index {
                *changed = true;
            }
        });

        if ui
            .checkbox(&mut config_edit.show_bots, "Show Bots")
            .changed()
        {
            *changed = true;
        }

        if ui
            .checkbox(&mut config_edit.show_stats, "Show Player Stats")
            .changed()
        {
            *changed = true;
        }

        ui.horizontal(|ui| {
            ui.label("Anchor:");
            egui::ComboBox::new("anchor_pos", "")
                .selected_text(format!("{:?}", config_edit.anchor))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut config_edit.anchor, AnchorPos::TopLeft, "Top Left");
                    ui.selectable_value(&mut config_edit.anchor, AnchorPos::TopRight, "Top Right");
                    ui.selectable_value(
                        &mut config_edit.anchor,
                        AnchorPos::BottomLeft,
                        "Bottom Left",
                    );
                    ui.selectable_value(
                        &mut config_edit.anchor,
                        AnchorPos::BottomRight,
                        "Bottom Right",
                    );
                    ui.selectable_value(
                        &mut config_edit.anchor,
                        AnchorPos::CenterRight,
                        "Center Right",
                    );
                });
            if config_edit.anchor != config.anchor {
                *changed = true;
            }
        });
    });

    ui.add_space(10.0);
    render_hotkey_settings_section(ui, ctx, state, config_edit, changed);

    ui.add_space(10.0);
    render_positioning_settings_section(ui, config_edit, changed);

    ui.add_space(10.0);
    render_launch_controls(ui, ctx, state, config_edit, is_launched);
}

fn render_session_settings_tab(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config_edit: &mut crate::state::Config,
    changed: &mut bool,
) {
    ui.columns(2, |columns| {
        columns[0].group(|ui| {
            ui.heading("Session Overlay");
            if ui
                .checkbox(
                    &mut config_edit.session_overlay_enabled,
                    "Enable Session Overlay",
                )
                .changed()
            {
                *changed = true;
            }

            ui.horizontal(|ui| {
                ui.label("Display:");
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

            ui.label("Scale");
            if ui
                .add(egui::Slider::new(
                    &mut config_edit.session_overlay_scale,
                    0.6..=2.5,
                ))
                .changed()
            {
                *changed = true;
            }

            ui.label("Opacity");
            if ui
                .add(egui::Slider::new(
                    &mut config_edit.session_overlay_opacity,
                    40..=255,
                ))
                .changed()
            {
                *changed = true;
            }

            ui.horizontal(|ui| {
                ui.label("Anchor:");
                egui::ComboBox::new("session_anchor", "")
                    .selected_text(format!("{:?}", config_edit.session_overlay_anchor))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut config_edit.session_overlay_anchor,
                            AnchorPos::TopLeft,
                            "Top Left",
                        );
                        ui.selectable_value(
                            &mut config_edit.session_overlay_anchor,
                            AnchorPos::TopRight,
                            "Top Right",
                        );
                        ui.selectable_value(
                            &mut config_edit.session_overlay_anchor,
                            AnchorPos::BottomLeft,
                            "Bottom Left",
                        );
                        ui.selectable_value(
                            &mut config_edit.session_overlay_anchor,
                            AnchorPos::BottomRight,
                            "Bottom Right",
                        );
                        ui.selectable_value(
                            &mut config_edit.session_overlay_anchor,
                            AnchorPos::CenterRight,
                            "Center Right",
                        );
                    });
                if config_edit.session_overlay_anchor != state.config.load().session_overlay_anchor
                {
                    *changed = true;
                }
            });

            ui.label("X Offset");
            if ui
                .add(egui::Slider::new(
                    &mut config_edit.session_overlay_offset[0],
                    -800.0..=800.0,
                ))
                .changed()
            {
                *changed = true;
            }
            ui.label("Y Offset");
            if ui
                .add(egui::Slider::new(
                    &mut config_edit.session_overlay_offset[1],
                    -800.0..=800.0,
                ))
                .changed()
            {
                *changed = true;
            }
        });

        columns[1].group(|ui| {
            render_local_mmr_panel(ui, state);
        });
    });

    ui.add_space(10.0);
    ui.group(|ui| {
        ui.label(egui::RichText::new("Preview").strong());
        draw_session_panel(
            ui,
            &state.session.load(),
            config_edit.session_overlay_scale.min(1.4),
            config_edit.session_overlay_display,
            config_edit.session_overlay_opacity,
        );
    });
}

fn render_local_mmr_panel(ui: &mut egui::Ui, state: &Arc<AppState>) {
    let identity = state.local_player_identity.load();
    let local_mmr = state.local_mmr.load();

    ui.heading("Local MMR");
    if identity.is_known() {
        debug_status_row(ui, "Player", identity.name.as_str());
        debug_status_row(ui, "Platform", identity.platform.as_str());
    } else {
        ui.colored_label(
            egui::Color32::from_rgb(220, 200, 100),
            "Waiting for local player identity.",
        );
    }

    ui.add_space(6.0);
    let can_refresh = identity.is_known() && !local_mmr.fetching;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(can_refresh, egui::Button::new("Refresh"))
            .clicked()
        {
            crate::mmr::start_local_mmr_refresh(state.clone());
        }
        if local_mmr.fetching {
            ui.add(egui::Spinner::new());
            ui.label("Fetching...");
        }
    });

    if local_mmr.last_updated_unix_ms > 0 {
        debug_status_row(
            ui,
            "Updated",
            &format_age(crate::stats_api::now_ms(), local_mmr.last_updated_unix_ms),
        );
    }
    if !local_mmr.error.is_empty() {
        ui.colored_label(
            egui::Color32::from_rgb(230, 120, 80),
            local_mmr.error.as_str(),
        );
    }

    ui.add_space(8.0);
    let Some(current) = &local_mmr.current else {
        ui.label(egui::RichText::new("No local MMR snapshot yet.").color(egui::Color32::GRAY));
        return;
    };

    let mut rows: Vec<_> = current.playlists.iter().collect();
    rows.sort_by_key(|(playlist_id, playlist)| {
        (
            ranked_playlist_sort_priority(**playlist_id, playlist.name.as_str()),
            **playlist_id,
        )
    });

    if rows.is_empty() {
        ui.label(egui::RichText::new("No ranked playlist data found.").color(egui::Color32::GRAY));
        return;
    }

    egui::Grid::new("local_mmr_grid")
        .num_columns(3)
        .spacing(egui::vec2(8.0, 4.0))
        .striped(true)
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Mode").strong());
            ui.label(egui::RichText::new("MMR").strong());
            ui.label(egui::RichText::new("Delta").strong());
            ui.end_row();

            for (playlist_id, playlist) in rows {
                let previous_rating = local_mmr
                    .previous
                    .as_ref()
                    .and_then(|snapshot| snapshot.playlists.get(playlist_id))
                    .map(|playlist| playlist.rating);
                ui.label(compact_playlist_name(playlist.name.as_str()));
                ui.label(playlist.rating.to_string());
                render_mmr_delta(ui, previous_rating.map(|rating| playlist.rating - rating));
                ui.end_row();
            }
        });
}

fn render_boost_settings_tab(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config_edit: &mut crate::state::Config,
    changed: &mut bool,
    is_rl_running: bool,
) {
    ui.group(|ui| {
        if ui
            .checkbox(
                &mut config_edit.show_teammate_boost,
                "Always-on Teammate Boost HUD",
            )
            .changed()
        {
            *changed = true;
        }

        ui.add_space(5.0);
        ui.horizontal(|ui| {
            ui.label("Display:");
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

        ui.add_space(5.0);
        ui.label("Teammate HUD Scale");
        if ui
            .add(egui::Slider::new(
                &mut config_edit.teammate_hud_scale,
                0.5..=2.5,
            ))
            .changed()
        {
            *changed = true;
        }

        ui.add_space(5.0);
        ui.label("Horizontal Offset");
        let max_horizontal_offset =
            (config_edit.window_size[0] / config_edit.teammate_hud_scale.max(0.1)).max(600.0);
        if ui
            .add(egui::Slider::new(
                &mut config_edit.teammate_boost_horizontal_offset,
                0.0..=max_horizontal_offset,
            ))
            .changed()
        {
            *changed = true;
        }

        ui.add_space(5.0);
        ui.label("Vertical Offset");
        if ui
            .add(egui::Slider::new(
                &mut config_edit.teammate_boost_offset,
                50.0..=600.0,
            ))
            .changed()
        {
            *changed = true;
        }
    });

    ui.add_space(10.0);
    ui.group(|ui| {
        ui.label(egui::RichText::new("Live Preview").strong());
        ui.add_space(4.0);
        let preview = preview_teammates(state);
        draw_teammate_boost_panel(
            ui,
            &preview,
            0,
            config_edit.teammate_hud_scale.min(1.4),
            config_edit.teammate_boost_display,
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(
                "Placement preview is only accurate while the overlay is launched.",
            )
            .size(10.0)
            .color(egui::Color32::from_gray(150)),
        );
    });

    ui.add_space(12.0);
    ui.group(|ui| {
        ui.heading("Alpha Boost (Gold Rush) Swap");
        ui.add_space(6.0);

        // 1. Rocket League Folder Path Input
        ui.horizontal(|ui| {
            ui.label("Rocket League Folder:");
            let path_edit = ui.text_edit_singleline(&mut config_edit.rocket_league_path);
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

        // Path validation feedback
        let path_valid = if config_edit.rocket_league_path.trim().is_empty() {
            None
        } else {
            let path = std::path::Path::new(&config_edit.rocket_league_path);
            Some(path.exists() && path.join("TAGame").join("CookedPCConsole").exists())
        };

        match path_valid {
            Some(true) => {
                ui.colored_label(egui::Color32::from_rgb(100, 220, 100), "✔ Valid Rocket League installation found.");
            }
            Some(false) => {
                ui.colored_label(egui::Color32::from_rgb(230, 80, 80), "❌ Invalid folder (TAGame/CookedPCConsole not found).");
            }
            None => {
                ui.colored_label(egui::Color32::from_rgb(220, 200, 100), "⚠ Path unconfigured. Paste path or click Auto-detect.");
            }
        }

        ui.add_space(8.0);

        let inspection = crate::assets::inspect_boost_swap(&config_edit.rocket_league_path);
        debug_status_row(
            ui,
            "Backup Metadata",
            if inspection.metadata_exists { "yes" } else { "no" },
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
        ui.label(
            egui::RichText::new(&inspection.message)
                .size(10.0)
                .color(egui::Color32::from_gray(160)),
        );
        ui.add_space(8.0);

        // Warning message required by user
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 140, 0), // Dark Orange
                    "⚠ Warning: Editing game files can technically be bannable (violates ToS). Use at your own risk.",
                );
            });
        });

        ui.add_space(8.0);

        let mut enabled =
            inspection.game_file_state == crate::assets::BoostGameFileState::Alpha;
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
                *status = "Error: Invalid Rocket League directory. Check the path and try again.".to_string();
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
            ui.colored_label(
                egui::Color32::from_rgb(220, 200, 100),
                "No backup metadata yet. First apply will back up the current game files as originals.",
            );
        } else if !can_toggle && path_valid == Some(true) {
            ui.colored_label(
                egui::Color32::from_rgb(220, 200, 100),
                "Current boost files are not a clean original/Alpha pair. Restore originals before applying.",
            );
        }

        if inspection.metadata_exists && ui.button("Restore Original Boost").clicked() {
            if config_edit.rocket_league_path.trim().is_empty() {
                let mut status = state.boost_swap_status.lock().unwrap();
                *status = "Error: Configure your Rocket League path first.".to_string();
            } else if path_valid != Some(true) {
                let mut status = state.boost_swap_status.lock().unwrap();
                *status =
                    "Error: Invalid Rocket League directory. Check the path and try again."
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
                ui.colored_label(egui::Color32::from_rgb(230, 80, 80), format!("❌ {status}"));
            } else if status.starts_with("Success") {
                ui.colored_label(
                    egui::Color32::from_rgb(100, 225, 100),
                    format!("✔ {status}"),
                );
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
            ui.colored_label(
                egui::Color32::from_rgb(220, 200, 100),
                "ℹ Rocket League is currently running. You must restart the game once to see boost changes.",
            );
        }
    });
}

fn teammate_boost_display_label(display: TeammateBoostDisplay) -> &'static str {
    match display {
        TeammateBoostDisplay::Bars => "Bars",
        TeammateBoostDisplay::Circles => "Circles",
        TeammateBoostDisplay::Compact => "Compact",
        TeammateBoostDisplay::Numbers => "Numbers",
    }
}

fn preview_teammates(state: &Arc<AppState>) -> Vec<crate::state::PlayerInfo> {
    let players = state.players.load();
    let local_name = state.local_player_name.load().trim().to_lowercase();
    let local_team = state.local_team.load(Ordering::SeqCst);
    let mut teammates: Vec<_> = players
        .values()
        .filter(|p| {
            local_team != 255
                && p.team == local_team
                && !p.is_local
                && (local_name.is_empty() || p.name.trim().to_lowercase() != local_name)
        })
        .cloned()
        .collect();

    if teammates.is_empty() {
        teammates = vec![
            crate::state::PlayerInfo {
                name: "C-Block".to_string(),
                team: 0,
                boost: 18,
                is_bot: true,
                platform: "BOT".to_string(),
                ..Default::default()
            },
            crate::state::PlayerInfo {
                name: "Caveman".to_string(),
                team: 0,
                boost: 72,
                is_bot: true,
                platform: "BOT".to_string(),
                ..Default::default()
            },
        ];
    }

    teammates.sort_by(|a, b| a.boost.cmp(&b.boost).then_with(|| a.name.cmp(&b.name)));
    teammates
}

fn render_hotkey_settings_section(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &Arc<AppState>,
    config_edit: &mut crate::state::Config,
    changed: &mut bool,
) {
    ui.group(|ui| {
        ui.heading("Hotkeys");
        render_keyboard_hotkey_row(ui, ctx, state, config_edit);
        render_controller_hotkey_row(ui, state, config_edit);
        render_settings_hotkey_row(ui, ctx, state, config_edit);

        if ui
            .checkbox(
                &mut config_edit.hotkey_toggle,
                "Toggle Hotkey (Instead of Hold)",
            )
            .changed()
        {
            *changed = true;
        }
    });
}

fn render_positioning_settings_section(
    ui: &mut egui::Ui,
    config_edit: &mut crate::state::Config,
    changed: &mut bool,
) {
    ui.group(|ui| {
        ui.heading("Overlay Positioning");
        if ui
            .checkbox(&mut config_edit.layout_mode, "Enable Drag Positioning")
            .changed()
        {
            *changed = true;
        }

        if config_edit.layout_mode {
            ui.colored_label(
                egui::Color32::from_rgb(220, 200, 100),
                "Launch the overlay, open settings, then drag visible overlay panels into place.",
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

fn render_keyboard_hotkey_row(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &Arc<AppState>,
    config_edit: &mut crate::state::Config,
) {
    ui.horizontal(|ui| {
        ui.label("Keyboard:");
        if state.is_recording_kb.load(Ordering::SeqCst) {
            ui.colored_label(egui::Color32::YELLOW, "Listening...");
            if ui.button("Cancel").clicked() {
                state.is_recording_kb.store(false, Ordering::SeqCst);
            }
            if let Some(name) = capture_egui_key(ctx) {
                config_edit.hotkey_kb = name;
                state.save_config(config_edit.clone());
                state.is_recording_kb.store(false, Ordering::SeqCst);
            }
        } else {
            ui.label(format!("[ {} ]", format_key_name(&config_edit.hotkey_kb)));
            if ui.button("Record").clicked() {
                state.is_recording_kb.store(true, Ordering::SeqCst);
                state.is_recording_ctrl.store(false, Ordering::SeqCst);
                state.is_recording_settings.store(false, Ordering::SeqCst);
            }
        }
    });
}

fn render_controller_hotkey_row(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config_edit: &crate::state::Config,
) {
    ui.horizontal(|ui| {
        ui.label("Controller:");
        if state.is_recording_ctrl.load(Ordering::SeqCst) {
            ui.colored_label(egui::Color32::YELLOW, "Listening...");
            if ui.button("Cancel").clicked() {
                state.is_recording_ctrl.store(false, Ordering::SeqCst);
            }
        } else {
            ui.label(format!("[ {} ]", config_edit.hotkey_ctrl));
            if ui.button("Record").clicked() {
                state.is_recording_ctrl.store(true, Ordering::SeqCst);
                state.is_recording_kb.store(false, Ordering::SeqCst);
                state.is_recording_settings.store(false, Ordering::SeqCst);
            }
        }
    });
}

fn render_settings_hotkey_row(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &Arc<AppState>,
    config_edit: &mut crate::state::Config,
) {
    ui.horizontal(|ui| {
        ui.label("Settings Toggle:");
        if state.is_recording_settings.load(Ordering::SeqCst) {
            ui.colored_label(egui::Color32::YELLOW, "Listening...");
            if ui.button("Cancel").clicked() {
                state.is_recording_settings.store(false, Ordering::SeqCst);
            }
            if let Some(name) = capture_egui_key(ctx) {
                config_edit.hotkey_settings = name;
                state.save_config(config_edit.clone());
                state.is_recording_settings.store(false, Ordering::SeqCst);
            }
        } else {
            ui.label(format!(
                "[ {} ]",
                format_key_name(&config_edit.hotkey_settings)
            ));
            if ui.button("Record").clicked() {
                state.is_recording_settings.store(true, Ordering::SeqCst);
                state.is_recording_kb.store(false, Ordering::SeqCst);
                state.is_recording_ctrl.store(false, Ordering::SeqCst);
            }
        }
    });
}

fn capture_egui_key(ctx: &egui::Context) -> Option<String> {
    let mut captured_name = None;
    ctx.input(|i| {
        if i.modifiers.ctrl {
            captured_name = Some("ControlLeft".to_string());
        } else if i.modifiers.shift {
            captured_name = Some("ShiftLeft".to_string());
        } else if i.modifiers.alt {
            captured_name = Some("Alt".to_string());
        } else if i.modifiers.command {
            captured_name = Some("MetaLeft".to_string());
        }

        for event in &i.events {
            if let egui::Event::Key {
                key, pressed: true, ..
            } = event
                && let Some(name) = egui_to_rdev_key(*key)
            {
                captured_name = Some(name);
            }
        }
    });
    captured_name
}

fn render_debug_settings_tab(ui: &mut egui::Ui, state: &Arc<AppState>, is_launched: bool) {
    ui.group(|ui| {
        ui.heading("Parsed State");
        debug_status_row(
            ui,
            "Overlay",
            if is_launched { "Launched" } else { "Settings" },
        );
        debug_status_row(
            ui,
            "Connection",
            if state.is_connected.load(Ordering::SeqCst) {
                "Connected"
            } else {
                "Disconnected"
            },
        );
        let local_name = state.local_player_name.load();
        debug_status_row(ui, "Local Player", local_name.as_str());
        let local_team = state.local_team.load(Ordering::SeqCst);
        let team_text = if local_team == 255 {
            "Unknown".to_string()
        } else {
            local_team.to_string()
        };
        debug_status_row(ui, "Local Team", &team_text);

        let players = state.players.load();
        debug_status_row(ui, "Players", &players.len().to_string());
        debug_status_row(
            ui,
            "Hotkey Log",
            &crate::input::hotkey_debug_log_path().display().to_string(),
        );
        if ui.button("Clear Hotkey Log").clicked() {
            let path = crate::input::hotkey_debug_log_path();
            match std::fs::write(&path, "") {
                Ok(()) => crate::input::append_hotkey_debug_log("hotkey_log_cleared"),
                Err(error) => crate::input::append_hotkey_debug_log(format!(
                    "hotkey_log_clear_failed error={error}"
                )),
            }
        }

        let diagnostics = state.network_diagnostics.load();
        debug_status_row(ui, "Transport", diagnostics.transport.label());
        debug_status_row(ui, "Last Event", diagnostics.last_event.as_str());
        debug_status_row(
            ui,
            "Last Event ms",
            &diagnostics.last_event_unix_ms.to_string(),
        );
        if !diagnostics.last_parse_error.is_empty() {
            debug_status_row(
                ui,
                "Last Parse Error",
                diagnostics.last_parse_error.as_str(),
            );
        }
        if !diagnostics.last_connection_error.is_empty() {
            debug_status_row(
                ui,
                "Last Connection Error",
                diagnostics.last_connection_error.as_str(),
            );
        }

        ui.separator();
        let version_check = state.version_check.load();
        debug_status_row(ui, "Current Version", env!("CARGO_PKG_VERSION"));
        let version_status = if !version_check.checked {
            "Checking...".to_string()
        } else if version_check.update_available {
            format!("Update available ({})", version_check.latest_tag)
        } else if !version_check.error.is_empty() {
            version_check.error.clone()
        } else {
            format!("Up to date ({})", version_check.latest_tag)
        };
        debug_status_row(ui, "Version Check", &version_status);

        let config_status = state.config_status.load();
        debug_status_row(ui, "Config Path", &config_status.path);
        debug_status_row(
            ui,
            "Config Status",
            if config_status.last_error.is_empty() {
                "OK"
            } else {
                config_status.last_error.as_str()
            },
        );

        ui.separator();
        for player in players.values() {
            ui.label(format!(
                "{} | team {} | {} | boost {}",
                player.name, player.team, player.platform, player.boost
            ));
        }
    });

    ui.add_space(10.0);
    ui.group(|ui| {
        ui.heading("Stats API Capture");
        let capture = state.debug_capture_status.load();
        if capture.running {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.label("Capturing 30 seconds of Stats API output...");
            });
        } else if ui.button("Capture 30s Stats API Output").clicked() {
            start_debug_capture(state.clone());
        }

        render_capture_status(ui, &capture);
    });
}

fn render_capture_status(ui: &mut egui::Ui, capture: &DebugCaptureStatus) {
    if !capture.message.is_empty() {
        ui.colored_label(egui::Color32::from_rgb(100, 220, 100), &capture.message);
    }
    if !capture.error.is_empty() {
        ui.colored_label(egui::Color32::from_rgb(230, 80, 80), &capture.error);
    }
    if !capture.last_output_path.is_empty() {
        debug_status_row(ui, "Output", &capture.last_output_path);
    }
}

fn start_debug_capture(state: Arc<AppState>) {
    let output = crate::stats_api::default_capture_path(crate::state::config_dir());
    state
        .debug_capture_status
        .store(Arc::new(DebugCaptureStatus {
            running: true,
            last_output_path: output.display().to_string(),
            message: String::new(),
            error: String::new(),
        }));

    tokio::spawn(async move {
        let result = crate::stats_api::capture_to_file(&output, 30).await;
        let status = match result {
            Ok(()) => DebugCaptureStatus {
                running: false,
                last_output_path: output.display().to_string(),
                message: "Capture complete.".to_string(),
                error: String::new(),
            },
            Err(error) => DebugCaptureStatus {
                running: false,
                last_output_path: output.display().to_string(),
                message: String::new(),
                error: format!("Capture failed: {error}"),
            },
        };
        state.debug_capture_status.store(Arc::new(status));
    });
}

fn debug_status_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(egui::Color32::from_gray(150)));
        ui.label(value);
    });
}

fn render_launch_controls(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &Arc<AppState>,
    config_edit: &crate::state::Config,
    is_launched: bool,
) {
    let btn_text = if is_launched {
        "Stop Overlay (HUD Active)"
    } else {
        "Launch Overlay"
    };
    if ui.button(egui::RichText::new(btn_text).heading()).clicked() {
        let new_val = !is_launched;
        state.is_launched.store(new_val, Ordering::SeqCst);
        if new_val {
            state.is_settings_visible.store(false, Ordering::SeqCst);
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                config_edit.window_size.into(),
            ));
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(true));
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize([720.0, 820.0].into()));
        }
    }

    ui.add_space(10.0);
    let is_visible = state.is_visible.load(Ordering::SeqCst);
    ui.horizontal(|ui| {
        ui.label("HUD Visibility:");
        if is_visible || is_launched {
            ui.colored_label(egui::Color32::GREEN, "ACTIVE");
        } else {
            ui.colored_label(egui::Color32::RED, "HIDDEN (Hold Hotkey)");
        }
    });

    ui.separator();
    if ui.button("Reset to Defaults").clicked() {
        let default_config = crate::state::Config::default();
        state.save_config(default_config);
    }
    if ui.button("Quit").clicked() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    ui.label(
        egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
            .size(9.0)
            .color(egui::Color32::from_gray(100)),
    );
}

fn render_overlay(ctx: &egui::Context, state: &Arc<AppState>) {
    let config = state.config.load();
    let players = state.players.load();

    let (anchor, base_offset) = match config.anchor {
        AnchorPos::TopLeft => (egui::Align2::LEFT_TOP, egui::vec2(20.0, 20.0)),
        AnchorPos::TopRight => (egui::Align2::RIGHT_TOP, egui::vec2(-20.0, 20.0)),
        AnchorPos::BottomLeft => (egui::Align2::LEFT_BOTTOM, egui::vec2(20.0, -20.0)),
        AnchorPos::BottomRight => (egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0, -20.0)),
        AnchorPos::CenterRight => (egui::Align2::RIGHT_CENTER, egui::vec2(-20.0, 0.0)),
    };

    let offset = base_offset * config.ui_scale;

    let area = egui::Area::new("overlay_area".into()).order(egui::Order::Foreground);
    let area = if let Some(position) = active_layout_drag_position(ctx, "lobby") {
        area.fixed_pos(position)
    } else if let Some(position) = config.lobby_manual_position {
        area.fixed_pos(normalized_to_pos(ctx, position))
    } else {
        area.anchor(anchor, offset)
    };

    let area_response = area.show(ctx, |ui| {
        let frame = egui::Frame::default()
            .fill(egui::Color32::from_rgba_unmultiplied(
                20,
                20,
                25,
                config.transparency,
            ))
            .stroke(egui::Stroke::new(
                1.0,
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, 20),
            ))
            .corner_radius(6.0 * config.ui_scale)
            .inner_margin(8.0 * config.ui_scale);

        frame
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("LOBBY")
                                .size(10.0 * config.ui_scale)
                                .color(egui::Color32::from_gray(180))
                                .strong(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let status_color = if state.is_connected.load(Ordering::SeqCst) {
                                egui::Color32::from_rgb(0, 255, 150)
                            } else {
                                egui::Color32::from_rgb(255, 80, 80)
                            };
                            ui.label(
                                egui::RichText::new("●")
                                    .color(status_color)
                                    .size(7.0 * config.ui_scale),
                            );
                        });
                    });
                    let drag_response =
                        render_drag_position_handle(ui, config.layout_mode, config.ui_scale);

                    ui.add_space(4.0 * config.ui_scale);

                    let mut sorted_players: Vec<_> = players
                        .values()
                        .filter(|p| config.show_bots || !p.is_bot)
                        .collect();
                    sorted_players
                        .sort_by(|a, b| a.team.cmp(&b.team).then_with(|| a.name.cmp(&b.name)));

                    if sorted_players.is_empty() {
                        ui.label(
                            egui::RichText::new("Waiting...")
                                .size(11.0 * config.ui_scale)
                                .italics()
                                .color(egui::Color32::from_gray(120)),
                        );
                    } else {
                        for p in sorted_players {
                            let team_color = if p.team == 0 {
                                egui::Color32::from_rgb(0, 212, 255)
                            } else {
                                egui::Color32::from_rgb(255, 140, 0)
                            };

                            ui.horizontal(|ui| {
                                // Vertical Team Accent
                                let (rect, _) = ui.allocate_at_least(
                                    egui::vec2(2.5 * config.ui_scale, 14.0 * config.ui_scale),
                                    egui::Sense::hover(),
                                );
                                ui.painter()
                                    .rect_filled(rect, 1.5 * config.ui_scale, team_color);

                                ui.add_space(4.0 * config.ui_scale);

                                // Player Name and MMR
                                ui.vertical(|ui| {
                                    let name_color = if p.is_bot {
                                        egui::Color32::from_gray(140)
                                    } else {
                                        egui::Color32::WHITE
                                    };
                                    ui.label(
                                        egui::RichText::new(&p.name)
                                            .color(name_color)
                                            .size(12.0 * config.ui_scale)
                                            .strong(),
                                    );

                                    // Render MMR if available
                                    if let Some(snapshot) = &p.mmr {
                                        let mut display_rank = "Unranked".to_string();
                                        let mut mmr_val = 0;

                                        // Find highest ranked playlist
                                        if let Some(playlist) = snapshot
                                            .playlists
                                            .values()
                                            .filter(|p| !p.tier_name.is_empty())
                                            .max_by_key(|p| p.rating)
                                        {
                                            display_rank = playlist.tier_name.clone();
                                            mmr_val = playlist.rating;
                                        }

                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{} ({} MMR)",
                                                display_rank, mmr_val
                                            ))
                                            .color(egui::Color32::from_rgb(180, 200, 255))
                                            .size(8.5 * config.ui_scale),
                                        );
                                    } else if !p.is_local
                                        && !p.is_bot
                                        && p.platform.to_lowercase() != "bot"
                                        && p.platform.to_lowercase() != "unknown"
                                    {
                                        ui.label(
                                            egui::RichText::new("Fetching rank...")
                                                .color(egui::Color32::from_gray(120))
                                                .size(8.5 * config.ui_scale),
                                        );
                                    }
                                });

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        // Platform Icon on the right
                                        let icon_source = if p.is_bot {
                                            egui::include_image!("../assets/bot.png")
                                        } else {
                                            let plat = p.platform.to_lowercase();
                                            if plat.contains("steam") {
                                                egui::include_image!("../assets/steam.png")
                                            } else if plat.contains("epic") {
                                                egui::include_image!("../assets/epic.png")
                                            } else if plat.contains("xbox") || plat.contains("xbl")
                                            {
                                                egui::include_image!("../assets/xbox.png")
                                            } else if plat.contains("playstation")
                                                || plat.contains("ps")
                                            {
                                                egui::include_image!("../assets/ps.png")
                                            } else if plat.contains("switch")
                                                || plat.contains("nintendo")
                                            {
                                                egui::include_image!("../assets/switch.png")
                                            } else {
                                                egui::include_image!("../assets/bot.png")
                                            }
                                        };

                                        ui.add(
                                            egui::Image::new(icon_source)
                                                .max_width(10.0 * config.ui_scale)
                                                .maintain_aspect_ratio(true),
                                        );

                                        ui.add_space(4.0 * config.ui_scale);
                                        ui.label(
                                            egui::RichText::new(&p.platform)
                                                .size(8.5 * config.ui_scale)
                                                .color(egui::Color32::from_gray(160)),
                                        );

                                        if config.show_stats {
                                            ui.add_space(6.0 * config.ui_scale);
                                            ui.vertical(|ui| {
                                                ui.label(
                                                    egui::RichText::new(format!("{}%", p.boost))
                                                        .size(10.0 * config.ui_scale)
                                                        .color(if p.boost > 50 {
                                                            egui::Color32::from_rgb(255, 255, 100)
                                                        } else {
                                                            egui::Color32::from_rgb(255, 150, 50)
                                                        })
                                                        .strong(),
                                                );
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "S:{} G:{} Sv:{}",
                                                        p.score, p.goals, p.saves
                                                    ))
                                                    .size(7.0 * config.ui_scale)
                                                    .color(egui::Color32::from_gray(120)),
                                                );
                                            });
                                        }
                                    },
                                );
                            });
                            ui.add_space(1.0 * config.ui_scale);
                        }
                    }
                    drag_response
                })
                .inner
            })
            .inner
    });

    if let Some(drag_response) = area_response.inner {
        persist_dragged_position(
            ctx,
            state,
            area_response.response.rect.min,
            "lobby",
            &drag_response,
        );
    }
}

fn render_teammate_boost(ctx: &egui::Context, state: &Arc<AppState>) {
    let players = state.players.load();
    let local_name_raw = state.local_player_name.load();
    let local_name = local_name_raw.trim().to_lowercase();
    let config = state.config.load();

    // Find our team (preferring the stabilized local_team from state)
    // Do not guess if not found, because a bad fallback shows the wrong team.
    let my_team = {
        let stored_team = state.local_team.load(Ordering::SeqCst);
        if stored_team != 255 {
            Some(stored_team)
        } else {
            players
                .values()
                .find(|p| {
                    p.is_local
                        || (!local_name.is_empty() && p.name.trim().to_lowercase() == local_name)
                })
                .map(|p| p.team)
        }
    };
    let Some(my_team) = my_team else {
        return;
    };

    // Find all teammates (excluding ourselves)
    let mut teammates: Vec<crate::state::PlayerInfo> = players
        .values()
        .filter(|p| {
            p.team == my_team
                && !p.is_local
                && (local_name.is_empty() || p.name.trim().to_lowercase() != local_name)
        })
        .cloned()
        .collect();

    if teammates.is_empty() {
        return;
    }

    teammates.sort_by(|a, b| a.boost.cmp(&b.boost).then_with(|| a.name.cmp(&b.name)));

    let screen_rect = ctx.input(|i| i.screen_rect());
    let width = teammate_boost_width(config.teammate_hud_scale, config.teammate_boost_display);
    let height = teammate_boost_panel_height(
        teammates.len(),
        config.teammate_hud_scale,
        config.teammate_boost_display,
    );
    let base_x = screen_rect.max.x
        - config.teammate_boost_horizontal_offset * config.teammate_hud_scale
        - width;
    let base_y =
        screen_rect.max.y - config.teammate_boost_offset * config.teammate_hud_scale - height;

    let position = active_layout_drag_position(ctx, "boost")
        .or_else(|| {
            config
                .teammate_boost_manual_position
                .map(|position| normalized_to_pos(ctx, position))
        })
        .unwrap_or_else(|| egui::pos2(base_x.max(0.0), base_y.max(0.0)));

    let response = egui::Area::new("teammate_boost_panel".into())
        .fixed_pos(position)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            draw_teammate_boost_panel(
                ui,
                &teammates,
                my_team,
                config.teammate_hud_scale,
                config.teammate_boost_display,
            );
            render_drag_position_handle(ui, config.layout_mode, config.teammate_hud_scale)
        });

    if let Some(drag_response) = response.inner {
        persist_dragged_position(
            ctx,
            state,
            response.response.rect.min,
            "boost",
            &drag_response,
        );
    }
}

fn render_teammate_boost_position_preview(
    ctx: &egui::Context,
    state: &Arc<AppState>,
    draggable: bool,
) {
    let config = state.config.load();
    let teammates = preview_teammates(state);
    let screen_rect = ctx.input(|i| i.screen_rect());
    let scale = config.teammate_hud_scale;
    let width = teammate_boost_width(scale, config.teammate_boost_display);
    let height = teammate_boost_panel_height(teammates.len(), scale, config.teammate_boost_display);
    let base_x = screen_rect.max.x - config.teammate_boost_horizontal_offset * scale - width;
    let base_y = screen_rect.max.y - config.teammate_boost_offset * scale - height;
    let position = active_layout_drag_position(ctx, "boost")
        .or_else(|| {
            config
                .teammate_boost_manual_position
                .map(|position| normalized_to_pos(ctx, position))
        })
        .unwrap_or_else(|| egui::pos2(base_x.max(0.0), base_y.max(0.0)));

    let response = egui::Area::new("teammate_boost_position_preview".into())
        .fixed_pos(position)
        .order(if draggable {
            egui::Order::Foreground
        } else {
            egui::Order::Background
        })
        .show(ctx, |ui| {
            ui.set_opacity(0.72);
            draw_teammate_boost_panel(ui, &teammates, 0, scale, config.teammate_boost_display);
            render_drag_position_handle(ui, draggable, scale)
        });

    if let Some(drag_response) = response.inner {
        persist_dragged_position(
            ctx,
            state,
            response.response.rect.min,
            "boost",
            &drag_response,
        );
    }
}

fn render_session_overlay(ctx: &egui::Context, state: &Arc<AppState>) {
    let config = state.config.load();
    let position = active_layout_drag_position(ctx, "session").or_else(|| {
        config
            .session_manual_position
            .map(|position| normalized_to_pos(ctx, position))
    });
    let area = egui::Area::new("session_overlay_panel".into()).order(egui::Order::Foreground);
    let area = if let Some(position) = position {
        area.fixed_pos(position)
    } else {
        let (anchor, offset) = anchor_offset(
            config.session_overlay_anchor,
            egui::vec2(
                config.session_overlay_offset[0],
                config.session_overlay_offset[1],
            ),
        );
        area.anchor(anchor, offset)
    };

    let response = area.show(ctx, |ui| {
        draw_session_panel(
            ui,
            &state.session.load(),
            config.session_overlay_scale,
            config.session_overlay_display,
            config.session_overlay_opacity,
        );
        render_drag_position_handle(ui, config.layout_mode, config.session_overlay_scale)
    });

    if let Some(drag_response) = response.inner {
        persist_dragged_position(
            ctx,
            state,
            response.response.rect.min,
            "session",
            &drag_response,
        );
    }
}

fn draw_session_panel(
    ui: &mut egui::Ui,
    session: &crate::session::SessionState,
    scale: f32,
    display: SessionOverlayDisplay,
    opacity: u8,
) {
    let width = match display {
        SessionOverlayDisplay::Compact => 155.0 * scale,
        SessionOverlayDisplay::Expanded => 220.0 * scale,
    };
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgba_unmultiplied(16, 18, 24, opacity))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(24)))
        .corner_radius(6.0 * scale)
        .inner_margin(8.0 * scale);

    frame.show(ui, |ui| {
        ui.set_min_width(width);
        ui.label(
            egui::RichText::new("SESSION")
                .size(9.0 * scale)
                .strong()
                .color(egui::Color32::from_gray(170)),
        );
        ui.add_space(4.0 * scale);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("{}W", session.wins))
                    .size(17.0 * scale)
                    .strong()
                    .color(egui::Color32::from_rgb(90, 230, 150)),
            );
            ui.label(
                egui::RichText::new(format!("{}L", session.losses))
                    .size(17.0 * scale)
                    .strong()
                    .color(egui::Color32::from_rgb(255, 105, 105)),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(streak_label(session.streak))
                        .size(12.0 * scale)
                        .color(egui::Color32::from_rgb(220, 220, 245)),
                );
            });
        });

        if display == SessionOverlayDisplay::Expanded {
            ui.separator();
            debug_status_row(ui, "Matches", &session.matches_played.to_string());
            debug_status_row(ui, "Last", session.last_result.label());
            debug_status_row(
                ui,
                "Score",
                &format!("{} - {}", session.blue_score, session.orange_score),
            );
        }
    });
}

fn render_drag_position_handle(
    ui: &mut egui::Ui,
    enabled: bool,
    scale: f32,
) -> Option<egui::Response> {
    if !enabled {
        return None;
    }

    ui.add_space(2.0 * scale);
    let size = egui::vec2(78.0 * scale, 15.0 * scale);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::drag());
    let rounding = 3.0 * scale;
    ui.painter().rect_filled(
        rect,
        rounding,
        egui::Color32::from_rgba_unmultiplied(80, 68, 24, 190),
    );
    ui.painter().rect_stroke(
        rect,
        rounding,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(220, 190, 90)),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "Drag to move",
        egui::FontId::proportional(8.0 * scale),
        egui::Color32::from_rgb(245, 220, 130),
    );
    Some(response)
}

fn streak_label(streak: i32) -> String {
    if streak > 0 {
        format!("+{} streak", streak)
    } else if streak < 0 {
        format!("{} streak", streak)
    } else {
        "no streak".to_string()
    }
}

fn draw_teammate_boost_panel(
    ui: &mut egui::Ui,
    teammates: &[crate::state::PlayerInfo],
    my_team: u8,
    scale: f32,
    display: TeammateBoostDisplay,
) {
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_black_alpha(96))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(18)))
        .corner_radius(6.0 * scale)
        .inner_margin(5.0 * scale);

    frame.show(ui, |ui| {
        ui.set_min_width(teammate_boost_width(scale, display) - 10.0 * scale);
        for (index, player) in teammates.iter().enumerate() {
            draw_teammate_boost_row(ui, player, my_team, scale, display);
            if index + 1 < teammates.len() {
                ui.add_space(3.0 * scale);
            }
        }
    });
}

fn draw_teammate_boost_row(
    ui: &mut egui::Ui,
    player: &crate::state::PlayerInfo,
    my_team: u8,
    scale: f32,
    display: TeammateBoostDisplay,
) {
    let row_size = egui::vec2(
        teammate_boost_width(scale, display) - 10.0 * scale,
        teammate_boost_row_height(scale, display),
    );
    let (rect, _) = ui.allocate_exact_size(row_size, egui::Sense::hover());
    let painter = ui.painter();
    let rounding = 4.0 * scale;
    let team_color = if my_team == 0 {
        egui::Color32::from_rgb(0, 176, 255)
    } else {
        egui::Color32::from_rgb(255, 132, 36)
    };

    let low_boost_alpha = if player.boost <= 20 {
        let pulse = (ui.input(|i| i.time) * 5.0).sin() as f32;
        (24.0 + 20.0 * ((pulse + 1.0) * 0.5)) as u8
    } else {
        0
    };

    painter.rect_filled(rect, rounding, egui::Color32::from_black_alpha(92));
    if low_boost_alpha > 0 {
        painter.rect_filled(
            rect,
            rounding,
            egui::Color32::from_rgba_unmultiplied(255, 40, 24, low_boost_alpha),
        );
    }

    let accent_rect = egui::Rect::from_min_max(
        rect.left_top(),
        egui::pos2(rect.left() + 3.0 * scale, rect.bottom()),
    );
    painter.rect_filled(accent_rect, rounding, team_color);

    match display {
        TeammateBoostDisplay::Bars => draw_teammate_boost_bar_content(ui, rect, player, scale),
        TeammateBoostDisplay::Circles => {
            draw_teammate_boost_circle_content(ui, rect, player, scale)
        }
        TeammateBoostDisplay::Compact => {
            draw_teammate_boost_compact_content(ui, rect, player, scale)
        }
        TeammateBoostDisplay::Numbers => {
            draw_teammate_boost_number_content(ui, rect, player, scale)
        }
    }
}

fn draw_teammate_boost_bar_content(
    ui: &egui::Ui,
    rect: egui::Rect,
    player: &crate::state::PlayerInfo,
    scale: f32,
) {
    let painter = ui.painter();
    let boost_color = teammate_boost_color(player.boost);
    let inner = rect.shrink2(egui::vec2(8.0 * scale, 4.0 * scale));
    let value_width = 34.0 * scale;
    let bar_height = 5.0 * scale;
    let bar_rect = egui::Rect::from_min_max(
        egui::pos2(inner.left(), inner.bottom() - bar_height),
        egui::pos2(inner.right() - value_width - 8.0 * scale, inner.bottom()),
    );
    let fill_width = bar_rect.width() * (player.boost as f32 / 100.0).clamp(0.0, 1.0);
    let fill_rect =
        egui::Rect::from_min_size(bar_rect.left_top(), egui::vec2(fill_width, bar_height));

    painter.text(
        egui::pos2(inner.left(), inner.top() - 1.0 * scale),
        egui::Align2::LEFT_TOP,
        &player.name,
        egui::FontId::proportional(10.5 * scale),
        egui::Color32::from_gray(232),
    );

    painter.text(
        egui::pos2(inner.right(), inner.center().y - 1.0 * scale),
        egui::Align2::RIGHT_CENTER,
        format!("{:>3}", player.boost),
        egui::FontId::monospace(16.0 * scale),
        boost_color,
    );

    painter.rect_filled(bar_rect, 2.0 * scale, egui::Color32::from_white_alpha(32));
    painter.rect_filled(fill_rect, 2.0 * scale, boost_color);
}

fn draw_teammate_boost_circle_content(
    ui: &egui::Ui,
    rect: egui::Rect,
    player: &crate::state::PlayerInfo,
    scale: f32,
) {
    let painter = ui.painter();
    let boost_color = teammate_boost_color(player.boost);
    let inner = rect.shrink2(egui::vec2(8.0 * scale, 4.0 * scale));
    let radius = 11.0 * scale;
    let center = egui::pos2(inner.right() - radius, inner.center().y);

    painter.text(
        egui::pos2(inner.left(), inner.center().y),
        egui::Align2::LEFT_CENTER,
        &player.name,
        egui::FontId::proportional(10.0 * scale),
        egui::Color32::from_gray(232),
    );

    painter.circle_filled(center, radius, egui::Color32::from_black_alpha(130));
    painter.circle_stroke(
        center,
        radius,
        egui::Stroke::new(2.0 * scale, egui::Color32::from_white_alpha(34)),
    );

    let start_angle = -std::f32::consts::PI * 0.5;
    let end_angle = start_angle + std::f32::consts::TAU * (player.boost as f32 / 100.0);
    if player.boost > 0 {
        let segments = 28;
        let mut points = Vec::with_capacity(segments + 1);
        for i in 0..=segments {
            let t = i as f32 / segments as f32;
            let angle = start_angle + (end_angle - start_angle) * t;
            points.push(center + egui::vec2(angle.cos(), angle.sin()) * radius);
        }
        painter.add(egui::Shape::Path(egui::epaint::PathShape {
            points,
            closed: false,
            fill: egui::Color32::TRANSPARENT,
            stroke: egui::Stroke::new(3.0 * scale, boost_color).into(),
        }));
    }

    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        player.boost.to_string(),
        egui::FontId::monospace(9.5 * scale),
        egui::Color32::WHITE,
    );
}

fn draw_teammate_boost_compact_content(
    ui: &egui::Ui,
    rect: egui::Rect,
    player: &crate::state::PlayerInfo,
    scale: f32,
) {
    let painter = ui.painter();
    let boost_color = teammate_boost_color(player.boost);
    let inner = rect.shrink2(egui::vec2(8.0 * scale, 3.0 * scale));
    painter.text(
        egui::pos2(inner.left(), inner.center().y),
        egui::Align2::LEFT_CENTER,
        &player.name,
        egui::FontId::proportional(10.0 * scale),
        egui::Color32::from_gray(232),
    );
    painter.text(
        egui::pos2(inner.right(), inner.center().y),
        egui::Align2::RIGHT_CENTER,
        format!("{:>3}", player.boost),
        egui::FontId::monospace(15.0 * scale),
        boost_color,
    );
}

fn draw_teammate_boost_number_content(
    ui: &egui::Ui,
    rect: egui::Rect,
    player: &crate::state::PlayerInfo,
    scale: f32,
) {
    let painter = ui.painter();
    let boost_color = teammate_boost_color(player.boost);
    let inner = rect.shrink2(egui::vec2(8.0 * scale, 2.0 * scale));
    painter.text(
        egui::pos2(inner.left(), inner.center().y),
        egui::Align2::LEFT_CENTER,
        player.name.chars().take(10).collect::<String>(),
        egui::FontId::proportional(9.0 * scale),
        egui::Color32::from_gray(210),
    );
    painter.text(
        egui::pos2(inner.right(), inner.center().y),
        egui::Align2::RIGHT_CENTER,
        player.boost.to_string(),
        egui::FontId::monospace(18.0 * scale),
        boost_color,
    );
}

fn teammate_boost_color(boost: u8) -> egui::Color32 {
    match boost {
        0..=20 => egui::Color32::from_rgb(255, 56, 48),
        21..=50 => egui::Color32::from_rgb(255, 157, 28),
        51..=80 => egui::Color32::from_rgb(255, 224, 74),
        _ => egui::Color32::from_rgb(102, 232, 255),
    }
}

fn teammate_boost_width(scale: f32, display: TeammateBoostDisplay) -> f32 {
    match display {
        TeammateBoostDisplay::Bars => 178.0 * scale,
        TeammateBoostDisplay::Circles => 142.0 * scale,
        TeammateBoostDisplay::Compact => 142.0 * scale,
        TeammateBoostDisplay::Numbers => 96.0 * scale,
    }
}

fn teammate_boost_panel_height(count: usize, scale: f32, display: TeammateBoostDisplay) -> f32 {
    let rows = count as f32 * teammate_boost_row_height(scale, display);
    let gaps = count.saturating_sub(1) as f32 * 3.0 * scale;
    rows + gaps + 10.0 * scale
}

fn teammate_boost_row_height(scale: f32, display: TeammateBoostDisplay) -> f32 {
    match display {
        TeammateBoostDisplay::Bars => 27.0 * scale,
        TeammateBoostDisplay::Circles => 30.0 * scale,
        TeammateBoostDisplay::Compact => 21.0 * scale,
        TeammateBoostDisplay::Numbers => 20.0 * scale,
    }
}

fn session_display_label(display: SessionOverlayDisplay) -> &'static str {
    match display {
        SessionOverlayDisplay::Compact => "Compact",
        SessionOverlayDisplay::Expanded => "Expanded",
    }
}

fn ranked_playlist_sort_priority(playlist_id: i32, playlist_name: &str) -> i32 {
    let name = playlist_name.to_lowercase();
    if playlist_id == 10 || name.contains("duel") || name.contains("1v1") {
        0
    } else if playlist_id == 11 || name.contains("doubles") || name.contains("2v2") {
        1
    } else if playlist_id == 13 || name.contains("standard") || name.contains("3v3") {
        2
    } else {
        10
    }
}

fn compact_playlist_name(playlist_name: &str) -> String {
    let name = playlist_name.to_lowercase();
    if name.contains("duel") || name.contains("1v1") {
        "1v1".to_string()
    } else if name.contains("doubles") || name.contains("2v2") {
        "2v2".to_string()
    } else if name.contains("standard") || name.contains("3v3") {
        "3v3".to_string()
    } else {
        playlist_name
            .trim_start_matches("Ranked ")
            .trim()
            .to_string()
    }
}

fn render_mmr_delta(ui: &mut egui::Ui, delta: Option<i32>) {
    let Some(delta) = delta else {
        ui.label(egui::RichText::new("-").color(egui::Color32::GRAY));
        return;
    };

    let (text, color) = if delta > 0 {
        (format!("+{delta}"), egui::Color32::from_rgb(100, 220, 140))
    } else if delta < 0 {
        (delta.to_string(), egui::Color32::from_rgb(230, 120, 120))
    } else {
        ("0".to_string(), egui::Color32::from_gray(180))
    };
    ui.label(egui::RichText::new(text).color(color));
}

fn format_age(now_unix_ms: u128, then_unix_ms: u128) -> String {
    let elapsed_seconds = now_unix_ms.saturating_sub(then_unix_ms) / 1000;
    if elapsed_seconds < 60 {
        format!("{elapsed_seconds}s ago")
    } else if elapsed_seconds < 60 * 60 {
        format!("{}m ago", elapsed_seconds / 60)
    } else {
        format!("{}h ago", elapsed_seconds / (60 * 60))
    }
}

fn anchor_offset(anchor: AnchorPos, offset: egui::Vec2) -> (egui::Align2, egui::Vec2) {
    match anchor {
        AnchorPos::TopLeft => (egui::Align2::LEFT_TOP, offset),
        AnchorPos::TopRight => (egui::Align2::RIGHT_TOP, offset),
        AnchorPos::BottomLeft => (egui::Align2::LEFT_BOTTOM, offset),
        AnchorPos::BottomRight => (egui::Align2::RIGHT_BOTTOM, offset),
        AnchorPos::CenterRight => (egui::Align2::RIGHT_CENTER, offset),
    }
}

fn normalized_to_pos(ctx: &egui::Context, position: [f32; 2]) -> egui::Pos2 {
    let rect = ctx.input(|i| i.screen_rect());
    egui::pos2(
        rect.left() + rect.width() * position[0].clamp(0.0, 1.0),
        rect.top() + rect.height() * position[1].clamp(0.0, 1.0),
    )
}

fn pos_to_normalized(ctx: &egui::Context, pos: egui::Pos2) -> [f32; 2] {
    let rect = ctx.input(|i| i.screen_rect());
    [
        ((pos.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0),
        ((pos.y - rect.top()) / rect.height().max(1.0)).clamp(0.0, 1.0),
    ]
}

fn layout_drag_position_id(target: &str) -> egui::Id {
    egui::Id::new(("layout_drag_position", target))
}

fn layout_drag_start_id(target: &str) -> egui::Id {
    egui::Id::new(("layout_drag_pointer_offset", target))
}

fn active_layout_drag_position(ctx: &egui::Context, target: &str) -> Option<egui::Pos2> {
    ctx.data(|data| data.get_temp::<egui::Pos2>(layout_drag_position_id(target)))
}

fn persist_dragged_position(
    ctx: &egui::Context,
    state: &Arc<AppState>,
    panel_pos: egui::Pos2,
    target: &str,
    drag_response: &egui::Response,
) {
    if !drag_response.drag_started() && !drag_response.dragged() && !drag_response.drag_stopped() {
        return;
    }

    let drag_offset_id = layout_drag_start_id(target);
    let drag_position_id = layout_drag_position_id(target);

    if drag_response.drag_started()
        && let Some(pointer_pos) = drag_response.interact_pointer_pos()
    {
        ctx.data_mut(|data| data.insert_temp(drag_offset_id, pointer_pos - panel_pos));
    }

    let pointer_pos = drag_response
        .interact_pointer_pos()
        .unwrap_or(panel_pos + drag_response.drag_delta());
    let pointer_offset = ctx
        .data(|data| data.get_temp::<egui::Vec2>(drag_offset_id))
        .unwrap_or_default();
    let new_panel_pos = pointer_pos - pointer_offset;
    ctx.data_mut(|data| data.insert_temp(drag_position_id, new_panel_pos));

    let new_position = pos_to_normalized(ctx, new_panel_pos);
    let mut config = (**state.config.load()).clone();
    match target {
        "lobby" => config.lobby_manual_position = Some(new_position),
        "boost" => config.teammate_boost_manual_position = Some(new_position),
        "session" => config.session_manual_position = Some(new_position),
        _ => return,
    }
    state.save_config(config);
    ctx.request_repaint();

    if drag_response.drag_stopped() {
        ctx.data_mut(|data| {
            data.remove_temp::<egui::Vec2>(drag_offset_id);
            data.remove_temp::<egui::Pos2>(drag_position_id);
        });
    }
}

fn format_key_name(key: &str) -> &str {
    match key {
        "Insert" => "Num0 / Insert",
        "End" => "Num1 / End",
        "DownArrow" => "Num2 / Down",
        "PageDown" => "Num3 / PgDn",
        "LeftArrow" => "Num4 / Left",
        "RightArrow" => "Num6 / Right",
        "Home" => "Num7 / Home",
        "UpArrow" => "Num8 / Up",
        "PageUp" => "Num9 / PgUp",
        "Delete" => "Num. / Del",
        s => s,
    }
}

fn egui_to_rdev_key(key: egui::Key) -> Option<String> {
    use egui::Key::*;
    let s = match key {
        A => "KeyA",
        B => "KeyB",
        C => "KeyC",
        D => "KeyD",
        E => "KeyE",
        F => "KeyF",
        G => "KeyG",
        H => "KeyH",
        I => "KeyI",
        J => "KeyJ",
        K => "KeyK",
        L => "KeyL",
        M => "KeyM",
        N => "KeyN",
        O => "KeyO",
        P => "KeyP",
        Q => "KeyQ",
        R => "KeyR",
        S => "KeyS",
        T => "KeyT",
        U => "KeyU",
        V => "KeyV",
        W => "KeyW",
        X => "KeyX",
        Y => "KeyY",
        Z => "KeyZ",
        Num0 => "Num0",
        Num1 => "Num1",
        Num2 => "Num2",
        Num3 => "Num3",
        Num4 => "Num4",
        Num5 => "Num5",
        Num6 => "Num6",
        Num7 => "Num7",
        Num8 => "Num8",
        Num9 => "Num9",
        F1 => "F1",
        F2 => "F2",
        F3 => "F3",
        F4 => "F4",
        F5 => "F5",
        F6 => "F6",
        F7 => "F7",
        F8 => "F8",
        F9 => "F9",
        F10 => "F10",
        F11 => "F11",
        F12 => "F12",
        F13 => "F13",
        F14 => "F14",
        F15 => "F15",
        F16 => "F16",
        F17 => "F17",
        F18 => "F18",
        F19 => "F19",
        F20 => "F20",
        ArrowDown => "DownArrow",
        ArrowLeft => "LeftArrow",
        ArrowRight => "RightArrow",
        ArrowUp => "UpArrow",
        Escape => "Escape",
        Tab => "Tab",
        Backspace => "Backspace",
        Enter => "Return",
        Space => "Space",
        Insert => "Insert",
        Delete => "Delete",
        Home => "Home",
        End => "End",
        PageUp => "PageUp",
        PageDown => "PageDown",
        Semicolon | Colon => "Semicolon",
        Comma => "Comma",
        Period => "Dot",
        Slash | Questionmark => "Slash",
        Backslash | Pipe => "Backslash",
        Backtick => "Backquote",
        Minus => "Minus",
        Equals | Plus => "Equal",
        OpenBracket | OpenCurlyBracket => "LeftBracket",
        CloseBracket | CloseCurlyBracket => "RightBracket",
        Quote => "Quote",
        _ => return None,
    };
    Some(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_playlist_name_formats_common_ranked_modes() {
        assert_eq!(compact_playlist_name("Ranked Duel 1v1"), "1v1");
        assert_eq!(compact_playlist_name("Ranked Doubles 2v2"), "2v2");
        assert_eq!(compact_playlist_name("Ranked Standard 3v3"), "3v3");
        assert_eq!(compact_playlist_name("Ranked Hoops"), "Hoops");
    }

    #[test]
    fn ranked_playlist_sort_priority_orders_core_modes_first() {
        assert_eq!(ranked_playlist_sort_priority(10, "Ranked Duel"), 0);
        assert_eq!(ranked_playlist_sort_priority(11, "Ranked Doubles"), 1);
        assert_eq!(ranked_playlist_sort_priority(13, "Ranked Standard"), 2);
        assert_eq!(ranked_playlist_sort_priority(27, "Ranked Hoops"), 10);
    }

    #[test]
    fn format_age_handles_seconds_minutes_and_hours() {
        assert_eq!(format_age(10_000, 5_000), "5s ago");
        assert_eq!(format_age(120_000, 0), "2m ago");
        assert_eq!(format_age(7_200_000, 0), "2h ago");
    }
}
