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
                // If launched, the main window acts as the full-screen overlay
                if is_launched {
                    // Render the HUD
                    if self.state.is_visible.load(Ordering::SeqCst) {
                        render_overlay(ctx, &self.state);
                    }
                }

                // 2. Settings UI (Floating Window)
                let show_settings =
                    !is_launched || self.state.is_recording_kb.load(Ordering::SeqCst);

                // Keep window on top every frame when launched
                if is_launched {
                    ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                        egui::WindowLevel::AlwaysOnTop,
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
                                    self.state.is_launched.store(false, Ordering::SeqCst);
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                                    ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(
                                        false,
                                    ));
                                }
                            });
                    }
                }

                if show_settings {
                    egui::Window::new("RL Overlay Settings")
                        .collapsible(true)
                        .resizable(true)
                        .title_bar(false)
                        .default_size([350.0, 450.0])
                        .show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("RL Overlay Settings").strong());
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.button("X").clicked() {
                                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                        }
                                    },
                                );
                            });
                            ui.separator();

                            if ui
                                .interact(ui.max_rect(), ui.id(), egui::Sense::drag())
                                .drag_started()
                            {
                                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                            }

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
                                        } else {
                                            ui.label(format!("[ {} ]", config_edit.hotkey_kb));
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
                                    if ui
                                        .checkbox(
                                            &mut config_edit.hotkey_toggle,
                                            "Toggle Hotkey (Instead of Hold)",
                                        )
                                        .changed()
                                    {
                                        changed = true;
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
