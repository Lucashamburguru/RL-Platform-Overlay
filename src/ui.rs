use crate::state::{AnchorPos, AppState, TeammateBoostDisplay};
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub struct MainApp {
    state: Arc<AppState>,
    settings_tab: SettingsTab,
    is_rl_running: bool,
    last_rl_check: std::time::Instant,
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
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsTab {
    Overlay,
    Boost,
    Hotkeys,
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
                // Render the HUD
                // If launched: obey the toggle hotkey
                // If not launched: always show for preview
                let show_hud = if is_launched {
                    self.state.is_visible.load(Ordering::SeqCst)
                } else {
                    true
                };

                if show_hud {
                    render_overlay(ctx, &self.state);
                }

                let show_settings = self.state.is_settings_visible.load(Ordering::SeqCst)
                    || self.state.is_recording_kb.load(Ordering::SeqCst)
                    || self.state.is_recording_ctrl.load(Ordering::SeqCst)
                    || self.state.is_recording_settings.load(Ordering::SeqCst);

                // 2. Always-on Teammate Boost HUD
                // Settings mode uses the Boost tab preview instead of the floating in-game HUD.
                if is_launched && !show_settings && config.show_teammate_boost {
                    render_teammate_boost(ctx, &self.state);
                }
                if show_settings
                    && self.settings_tab == SettingsTab::Boost
                    && config.show_teammate_boost
                {
                    render_teammate_boost_position_preview(ctx, &self.state);
                }

                // 3. Settings UI (Floating Window)

                // Keep window on top every frame when launched
                if is_launched {
                    ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                        egui::WindowLevel::AlwaysOnTop,
                    ));

                    // If settings are visible, we need to be able to click them!
                    // If settings are hidden, we want clicks to pass through to the game.
                    ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(!show_settings));
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
                                    self.state.is_settings_visible.store(true, Ordering::SeqCst);
                                }
                            });
                    }
                }

                if show_settings {
                    // Fallback: Check for Settings Toggle hotkey inside the UI too
                    // so it works when the window has focus
                    let settings_hotkey = config.hotkey_settings.clone();
                    let hud_hotkey = config.hotkey_kb.clone();
                    let hotkey_toggle = config.hotkey_toggle;

                    ctx.input(|i| {
                        for event in &i.events {
                            if let egui::Event::Key { key, pressed, .. } = event
                                && let Some(name) = egui_to_rdev_key(*key)
                            {
                                // Handle Settings Toggle
                                if *pressed && name == settings_hotkey {
                                    let curr =
                                        self.state.is_settings_visible.load(Ordering::SeqCst);
                                    self.state
                                        .is_settings_visible
                                        .store(!curr, Ordering::SeqCst);
                                }

                                // Handle HUD Hotkey fallback when focused
                                if name == hud_hotkey {
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

                    egui::Window::new("RL Overlay Settings")
                        .collapsible(true)
                        .resizable(true)
                        .title_bar(false)
                        .default_size([450.0, 600.0])
                        .show(ctx, |ui| {
                            let title_bar_response = ui
                                .horizontal(|ui| {
                                    ui.label(egui::RichText::new("RL Overlay Settings").strong());
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let close_btn = ui.add(
                                                egui::Button::new(
                                                    egui::RichText::new("  X  ")
                                                        .strong()
                                                        .color(egui::Color32::WHITE),
                                                )
                                                .fill(egui::Color32::from_rgb(180, 40, 40))
                                                .min_size(egui::vec2(40.0, 24.0)),
                                            );
                                            if close_btn.clicked() {
                                                self.state
                                                    .is_settings_visible
                                                    .store(false, Ordering::SeqCst);
                                            }
                                        },
                                    );
                                })
                                .response;

                            if title_bar_response
                                .interact(egui::Sense::drag())
                                .drag_started()
                            {
                                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                            }

                            ui.separator();

                            ui.add_space(5.0);

                            let mut config_edit = (**self.state.config.load()).clone();
                            let mut changed = false;

                            render_update_notice(ui, &self.state);
                            render_settings_tabs(ui, &mut self.settings_tab);

                            egui::ScrollArea::vertical().show(ui, |ui| match self.settings_tab {
                                SettingsTab::Overlay => render_overlay_settings_tab(
                                    ui,
                                    ctx,
                                    &self.state,
                                    &config,
                                    &mut config_edit,
                                    &mut changed,
                                    is_launched,
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
                                SettingsTab::Hotkeys => render_hotkey_settings_tab(
                                    ui,
                                    ctx,
                                    &self.state,
                                    &mut config_edit,
                                    &mut changed,
                                ),
                                SettingsTab::Debug => {
                                    render_debug_settings_tab(ui, &self.state, is_launched)
                                }
                            });

                            if changed {
                                self.state.save_config(config_edit);
                            }
                        });
                }
            });

        ctx.request_repaint();
    }
}

fn render_settings_tabs(ui: &mut egui::Ui, selected: &mut SettingsTab) {
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(selected, SettingsTab::Overlay, "Overlay");
        ui.selectable_value(selected, SettingsTab::Boost, "Boost");
        ui.selectable_value(selected, SettingsTab::Hotkeys, "Hotkeys");
        ui.selectable_value(selected, SettingsTab::Debug, "Debug");
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
    render_launch_controls(ui, ctx, state, config_edit, is_launched);
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

        // Swapping Checkbox
        let mut enabled = config_edit.alpha_boost_enabled;
        let checkbox_resp = ui.checkbox(&mut enabled, "Replace Standard Boost with Alpha Boost (Gold Rush)");
        if checkbox_resp.changed() {
            if config_edit.rocket_league_path.trim().is_empty() {
                let mut status = state.boost_swap_status.lock().unwrap();
                *status = "Error: Configure your Rocket League path first.".to_string();
            } else if path_valid != Some(true) {
                let mut status = state.boost_swap_status.lock().unwrap();
                *status = "Error: Invalid Rocket League directory. Check the path and try again.".to_string();
            } else {
                if enabled {
                    crate::assets::start_apply_alpha_boost(state.clone(), config_edit.rocket_league_path.clone());
                } else {
                    crate::assets::start_restore_standard_boost(state.clone(), config_edit.rocket_league_path.clone());
                }
            }
        }

        // Render swap operation feedback
        let status = state.boost_swap_status.lock().unwrap().clone();
        if status != "Idle" {
            ui.add_space(6.0);
            if status.starts_with("Error") || status.starts_with("Download failed") || status.starts_with("Backup failed") || status.starts_with("Swap failed") || status.starts_with("Restore failed") {
                ui.colored_label(egui::Color32::from_rgb(230, 80, 80), format!("❌ {}", status));
            } else if status.starts_with("Success") {
                ui.colored_label(egui::Color32::from_rgb(100, 225, 100), format!("✔ {}", status));
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

fn render_hotkey_settings_tab(
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
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize([400.0, 600.0].into()));
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

    egui::Area::new("overlay_area".into())
        .anchor(anchor, offset)
        .show(ctx, |ui| {
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

            frame.show(ui, |ui| {
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
                                    } else if !p.is_bot
                                        && (p.platform.eq_ignore_ascii_case("Steam")
                                            || p.platform.eq_ignore_ascii_case("Epic"))
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
                });
            });
        });
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

    egui::Area::new("teammate_boost_panel".into())
        .fixed_pos(egui::pos2(base_x.max(0.0), base_y.max(0.0)))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            draw_teammate_boost_panel(
                ui,
                &teammates,
                my_team,
                config.teammate_hud_scale,
                config.teammate_boost_display,
            );
        });
}

fn render_teammate_boost_position_preview(ctx: &egui::Context, state: &Arc<AppState>) {
    let config = state.config.load();
    let teammates = preview_teammates(state);
    let screen_rect = ctx.input(|i| i.screen_rect());
    let scale = config.teammate_hud_scale;
    let width = teammate_boost_width(scale, config.teammate_boost_display);
    let height = teammate_boost_panel_height(teammates.len(), scale, config.teammate_boost_display);
    let base_x = screen_rect.max.x - config.teammate_boost_horizontal_offset * scale - width;
    let base_y = screen_rect.max.y - config.teammate_boost_offset * scale - height;

    egui::Area::new("teammate_boost_position_preview".into())
        .fixed_pos(egui::pos2(base_x.max(0.0), base_y.max(0.0)))
        .order(egui::Order::Background)
        .show(ctx, |ui| {
            ui.set_opacity(0.72);
            draw_teammate_boost_panel(ui, &teammates, 0, scale, config.teammate_boost_display);
        });
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
