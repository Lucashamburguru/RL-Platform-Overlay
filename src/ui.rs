use crate::state::{AnchorPos, AppState};
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub struct MainApp {
    state: Arc<AppState>,
}

impl MainApp {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
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
                // Show if launched OR if settings are visible (for preview)
                if (is_launched || show_settings) && config.show_teammate_boost {
                    render_teammate_boost(ctx, &self.state);
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
                            if let egui::Event::Key { key, pressed, .. } = event {
                                if let Some(name) = egui_to_rdev_key(*key) {
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
                                                let curr =
                                                    self.state.is_visible.load(Ordering::SeqCst);
                                                self.state
                                                    .is_visible
                                                    .store(!curr, Ordering::SeqCst);
                                            }
                                        } else {
                                            self.state.is_visible.store(*pressed, Ordering::SeqCst);
                                        }
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

                            egui::ScrollArea::vertical().show(ui, |ui| {
                                ui.group(|ui| {
                                    ui.label("Transparency");
                                    if ui
                                        .add(egui::Slider::new(
                                            &mut config_edit.transparency,
                                            0..=255,
                                        ))
                                        .changed()
                                    {
                                        changed = true;
                                    }

                                    ui.label("HUD Scale");
                                    if ui
                                        .add(egui::Slider::new(
                                            &mut config_edit.ui_scale,
                                            0.5..=2.5,
                                        ))
                                        .changed()
                                    {
                                        changed = true;
                                    }

                                    ui.horizontal(|ui| {
                                        ui.label("Resolution:");
                                        let current_res = format!(
                                            "{}x{}",
                                            config_edit.window_size[0], config_edit.window_size[1]
                                        );
                                        egui::ComboBox::new("res_select", "")
                                            .selected_text(current_res)
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
                                            changed = true;
                                        }
                                    });

                                    ui.horizontal(|ui| {
                                        ui.label("Monitor:");
                                        egui::ComboBox::new("monitor_select", "")
                                            .selected_text(format!(
                                                "Monitor {}",
                                                config_edit.monitor_index
                                            ))
                                            .show_ui(ui, |ui| {
                                                // Support up to 4 monitors for now
                                                for i in 0..4 {
                                                    ui.selectable_value(
                                                        &mut config_edit.monitor_index,
                                                        i,
                                                        format!("Monitor {}", i),
                                                    );
                                                }
                                            });
                                        if config_edit.monitor_index != config.monitor_index {
                                            changed = true;
                                        }
                                    });

                                    if ui
                                        .checkbox(&mut config_edit.show_bots, "Show Bots")
                                        .changed()
                                    {
                                        changed = true;
                                    }

                                    ui.horizontal(|ui| {
                                        ui.label("Anchor:");
                                        egui::ComboBox::new("anchor_pos", "")
                                            .selected_text(format!("{:?}", config_edit.anchor))
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(
                                                    &mut config_edit.anchor,
                                                    AnchorPos::TopLeft,
                                                    "Top Left",
                                                );
                                                ui.selectable_value(
                                                    &mut config_edit.anchor,
                                                    AnchorPos::TopRight,
                                                    "Top Right",
                                                );
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
                                            changed = true;
                                        }
                                    });

                                    ui.horizontal(|ui| {
                                        ui.label("Resolution:");
                                        let current_res = config_edit.window_size;
                                        let res_text =
                                            format!("{}x{}", current_res[0], current_res[1]);
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
                                            changed = true;
                                        }
                                    });
                                });

                                ui.add_space(10.0);

                                ui.group(|ui| {
                                    ui.heading("Hotkeys");
                                    ui.horizontal(|ui| {
                                        ui.label("Keyboard:");
                                        if self.state.is_recording_kb.load(Ordering::SeqCst) {
                                            ui.colored_label(egui::Color32::YELLOW, "Listening...");
                                            if ui.button("Cancel").clicked() {
                                                self.state
                                                    .is_recording_kb
                                                    .store(false, Ordering::SeqCst);
                                            }

                                            // Fallback: Capture keys directly from egui if the window has focus
                                            let mut captured_name = None;
                                            ctx.input(|i| {
                                                // Check modifiers first
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
                                                        key,
                                                        pressed: true,
                                                        ..
                                                    } = event
                                                    {
                                                        if let Some(name) = egui_to_rdev_key(*key) {
                                                            captured_name = Some(name);
                                                        }
                                                    }
                                                }
                                            });

                                            if let Some(name) = captured_name {
                                                let mut new_config =
                                                    (**self.state.config.load()).clone();
                                                new_config.hotkey_kb = name.clone();
                                                new_config.save();
                                                self.state.config.store(Arc::new(new_config));
                                                self.state
                                                    .is_recording_kb
                                                    .store(false, Ordering::SeqCst);
                                                println!(
                                                    "Keyboard hotkey updated (via UI): {}",
                                                    name
                                                );
                                            }
                                        } else {
                                            ui.label(format!(
                                                "[ {} ]",
                                                format_key_name(&config_edit.hotkey_kb)
                                            ));
                                            if ui.button("Record").clicked() {
                                                self.state
                                                    .is_recording_kb
                                                    .store(true, Ordering::SeqCst);
                                                self.state
                                                    .is_recording_ctrl
                                                    .store(false, Ordering::SeqCst);
                                            }
                                        }
                                    });

                                    ui.horizontal(|ui| {
                                        ui.label("Controller:");
                                        if self.state.is_recording_ctrl.load(Ordering::SeqCst) {
                                            ui.colored_label(egui::Color32::YELLOW, "Listening...");
                                            if ui.button("Cancel").clicked() {
                                                self.state
                                                    .is_recording_ctrl
                                                    .store(false, Ordering::SeqCst);
                                            }
                                        } else {
                                            ui.label(format!("[ {} ]", config_edit.hotkey_ctrl));
                                            if ui.button("Record").clicked() {
                                                self.state
                                                    .is_recording_ctrl
                                                    .store(true, Ordering::SeqCst);
                                                self.state
                                                    .is_recording_kb
                                                    .store(false, Ordering::SeqCst);
                                            }
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("Settings Toggle:");
                                        if self.state.is_recording_settings.load(Ordering::SeqCst) {
                                            ui.colored_label(egui::Color32::YELLOW, "Listening...");
                                            if ui.button("Cancel").clicked() {
                                                self.state
                                                    .is_recording_settings
                                                    .store(false, Ordering::SeqCst);
                                            }

                                            let mut captured_name = None;
                                            ctx.input(|i| {
                                                // Check modifiers first
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
                                                        key,
                                                        pressed: true,
                                                        ..
                                                    } = event
                                                    {
                                                        if let Some(name) = egui_to_rdev_key(*key) {
                                                            captured_name = Some(name);
                                                        }
                                                    }
                                                }
                                            });

                                            if let Some(name) = captured_name {
                                                let mut new_config =
                                                    (**self.state.config.load()).clone();
                                                new_config.hotkey_settings = name.clone();
                                                new_config.save();
                                                self.state.config.store(Arc::new(new_config));
                                                self.state
                                                    .is_recording_settings
                                                    .store(false, Ordering::SeqCst);
                                                println!("Settings hotkey updated: {}", name);
                                            }
                                        } else {
                                            ui.label(format!(
                                                "[ {} ]",
                                                format_key_name(&config_edit.hotkey_settings)
                                            ));
                                            if ui.button("Record").clicked() {
                                                self.state
                                                    .is_recording_settings
                                                    .store(true, Ordering::SeqCst);
                                                self.state
                                                    .is_recording_kb
                                                    .store(false, Ordering::SeqCst);
                                                self.state
                                                    .is_recording_ctrl
                                                    .store(false, Ordering::SeqCst);
                                            }
                                        }
                                    });

                                    if ui
                                        .checkbox(
                                            &mut config_edit.hotkey_toggle,
                                            "Toggle Hotkey (Instead of Hold)",
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }

                                    if ui
                                        .checkbox(
                                            &mut config_edit.show_stats,
                                            "Show Player Stats (Boost, Score)",
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }

                                    if ui
                                        .checkbox(
                                            &mut config_edit.show_teammate_boost,
                                            "Always-on Teammate Boost HUD",
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }

                                    if config_edit.show_teammate_boost {
                                        ui.add_space(5.0);
                                        let players = self.state.players.load();
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "Detected Player: {}",
                                                self.state.local_player_name.load()
                                            ))
                                            .size(10.0)
                                            .color(egui::Color32::from_gray(140)),
                                        );
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "Players in Session: {}",
                                                players.len()
                                            ))
                                            .size(10.0)
                                            .color(egui::Color32::from_gray(140)),
                                        );

                                        ui.add_space(5.0);

                                        ui.label("Teammate HUD Scale");
                                        if ui
                                            .add(egui::Slider::new(
                                                &mut config_edit.teammate_hud_scale,
                                                0.5..=2.5,
                                            ))
                                            .changed()
                                        {
                                            changed = true;
                                        }

                                        ui.add_space(5.0);
                                        ui.label("Teammate HUD Horizontal Offset");
                                        if ui
                                            .add(egui::Slider::new(
                                                &mut config_edit.teammate_boost_horizontal_offset,
                                                20.0..=600.0,
                                            ))
                                            .changed()
                                        {
                                            changed = true;
                                        }

                                        ui.add_space(5.0);
                                        ui.label("Teammate HUD Vertical Offset");
                                        if ui
                                            .add(egui::Slider::new(
                                                &mut config_edit.teammate_boost_offset,
                                                50.0..=600.0,
                                            ))
                                            .changed()
                                        {
                                            changed = true;
                                        }
                                    }
                                });

                                ui.add_space(10.0);

                                let btn_text = if is_launched {
                                    "Stop Overlay (HUD Active)"
                                } else {
                                    "Launch Overlay"
                                };
                                if ui.button(egui::RichText::new(btn_text).heading()).clicked() {
                                    let new_val = !is_launched;
                                    self.state.is_launched.store(new_val, Ordering::SeqCst);
                                    if new_val {
                                        // Auto-hide settings when launching
                                        self.state
                                            .is_settings_visible
                                            .store(false, Ordering::SeqCst);

                                        // Fullscreen-like transparent window
                                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                                            config_edit.window_size.into(),
                                        ));
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(
                                            true,
                                        ));
                                        ctx.send_viewport_cmd(
                                            egui::ViewportCommand::MousePassthrough(true),
                                        );
                                    } else {
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(
                                            false,
                                        ));
                                        ctx.send_viewport_cmd(
                                            egui::ViewportCommand::MousePassthrough(false),
                                        );
                                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                                            [400.0, 600.0].into(),
                                        ));
                                    }
                                }

                                ui.add_space(10.0);
                                let is_visible = self.state.is_visible.load(Ordering::SeqCst);
                                ui.horizontal(|ui| {
                                    ui.label("HUD Visibility:");
                                    if is_visible || is_launched {
                                        ui.colored_label(egui::Color32::GREEN, "ACTIVE");
                                    } else {
                                        ui.colored_label(
                                            egui::Color32::RED,
                                            "HIDDEN (Hold Hotkey)",
                                        );
                                    }
                                });

                                if ui.button("Quit").clicked() {
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                }

                                ui.add_space(5.0);
                                ui.label(
                                    egui::RichText::new("v0.1.4")
                                        .size(9.0)
                                        .color(egui::Color32::from_gray(100)),
                                );

                                ui.separator();
                                if ui.button("Reset to Defaults").clicked() {
                                    let default_config = crate::state::Config::default();
                                    default_config.save();
                                    self.state.config.store(Arc::new(default_config));
                                }
                            });

                            if changed {
                                config_edit.save();
                                self.state.config.store(Arc::new(config_edit));
                            }
                        });
                }
            });

        ctx.request_repaint();
    }
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

                                // Player Name
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

    // Find our team. Do not guess, because a bad fallback shows the wrong team
    // and can include the local player's own boost.
    let Some(my_team) = players
        .values()
        .find(|p| {
            p.is_local || (!local_name.is_empty() && p.name.trim().to_lowercase() == local_name)
        })
        .map(|p| p.team)
    else {
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

    teammates.sort_by(|a, b| a.name.cmp(&b.name));

    let screen_rect = ctx.input(|i| i.screen_rect());
    // Dynamic offsets from right and bottom based on config
    let base_x =
        screen_rect.max.x - config.teammate_boost_horizontal_offset * config.teammate_hud_scale;
    let mut current_y =
        screen_rect.max.y - config.teammate_boost_offset * config.teammate_hud_scale;

    for p in teammates {
        egui::Area::new(format!("teammate_boost_{}", p.name).into())
            .fixed_pos(egui::pos2(base_x, current_y))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Small Name Tag
                    ui.add_space(20.0 * config.teammate_hud_scale);
                    ui.vertical(|ui| {
                        ui.add_space(10.0 * config.teammate_hud_scale);
                        ui.label(
                            egui::RichText::new(&p.name)
                                .size(11.0 * config.teammate_hud_scale)
                                .color(egui::Color32::from_gray(200))
                                .strong(),
                        );
                    });

                    // Boost Circle
                    let radius = 22.0 * config.teammate_hud_scale;
                    let (rect, _response) = ui.allocate_at_least(
                        egui::vec2(radius * 2.0, radius * 2.0),
                        egui::Sense::hover(),
                    );
                    let center = rect.center();

                    // Background fill
                    ui.painter().circle_filled(
                        center,
                        radius,
                        egui::Color32::from_black_alpha(150),
                    );

                    let boost_color = if p.boost > 50 {
                        egui::Color32::from_rgb(255, 200, 0) // Gold
                    } else {
                        egui::Color32::from_rgb(255, 100, 0) // Orange
                    };

                    // Circular Progress Arc
                    let start_angle = std::f32::consts::PI * 0.5; // Bottom
                    let boost_fraction = p.boost as f32 / 100.0;
                    let end_angle = start_angle + (std::f32::consts::PI * 2.0 * boost_fraction);

                    // Draw the arc using segments
                    let num_segments = 32;
                    let mut points = Vec::new();
                    for i in 0..=num_segments {
                        let angle = start_angle
                            + (end_angle - start_angle) * (i as f32 / num_segments as f32);
                        points.push(center + egui::vec2(angle.cos(), angle.sin()) * radius);
                    }

                    ui.painter().add(egui::Shape::Path(egui::epaint::PathShape {
                        points,
                        closed: false,
                        fill: egui::Color32::TRANSPARENT,
                        stroke: egui::Stroke::new(3.5 * config.teammate_hud_scale, boost_color)
                            .into(),
                    }));

                    // Dark outer ring for depth
                    ui.painter().circle_stroke(
                        center,
                        radius + 1.0,
                        egui::Stroke::new(1.0, egui::Color32::from_black_alpha(100)),
                    );

                    // Boost Value
                    ui.painter().text(
                        center,
                        egui::Align2::CENTER_CENTER,
                        p.boost.to_string(),
                        egui::FontId::proportional(16.0 * config.teammate_hud_scale),
                        egui::Color32::WHITE,
                    );
                });
            });
        current_y -= 55.0 * config.teammate_hud_scale; // Move up for next teammate
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
