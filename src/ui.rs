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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. Show Settings UI
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("RL Overlay Settings");
            ui.add_space(10.0);

            let mut config = (**self.state.config.load()).clone();
            let mut changed = false;

            ui.group(|ui| {
                ui.label("Transparency");
                if ui.add(egui::Slider::new(&mut config.transparency, 0..=255)).changed() {
                    changed = true;
                }

                ui.label("HUD Scale");
                if ui.add(egui::Slider::new(&mut config.ui_scale, 0.5..=2.5)).changed() {
                    changed = true;
                }

                if ui.checkbox(&mut config.show_bots, "Show Bots").changed() {
                    changed = true;
                }

                ui.horizontal(|ui| {
                    ui.label("Anchor Position:");
                    let prev_anchor = config.anchor;
                    egui::ComboBox::from_id_source("anchor_pos")
                        .selected_text(format!("{:?}", config.anchor))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut config.anchor, AnchorPos::TopLeft, "Top Left");
                            ui.selectable_value(&mut config.anchor, AnchorPos::TopRight, "Top Right");
                            ui.selectable_value(&mut config.anchor, AnchorPos::BottomLeft, "Bottom Left");
                            ui.selectable_value(&mut config.anchor, AnchorPos::BottomRight, "Bottom Right");
                            ui.selectable_value(&mut config.anchor, AnchorPos::CenterRight, "Center Right");
                        });
                    if config.anchor != prev_anchor {
                        changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Monitor:");
                    let prev_monitor = config.monitor_index;
                    egui::ComboBox::from_id_source("monitor_idx")
                        .selected_text(format!("Monitor {}", config.monitor_index + 1))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut config.monitor_index, 0, "Monitor 1");
                            ui.selectable_value(&mut config.monitor_index, 1, "Monitor 2");
                            ui.selectable_value(&mut config.monitor_index, 2, "Monitor 3");
                        });
                    if config.monitor_index != prev_monitor {
                        changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Resolution:");
                    let current_res = config.window_size;
                    let res_text = format!("{}x{}", current_res[0], current_res[1]);

                    egui::ComboBox::from_id_source("res_presets")
                        .selected_text(res_text)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut config.window_size, [1920.0, 1080.0], "1080p");
                            ui.selectable_value(&mut config.window_size, [2560.0, 1440.0], "1440p");
                            ui.selectable_value(&mut config.window_size, [3840.0, 2160.0], "4K");
                            ui.selectable_value(&mut config.window_size, [3440.0, 1440.0], "Ultrawide");
                        });

                    if config.window_size != current_res {
                        changed = true;
                    }
                });
            });

            ui.add_space(10.0);

            ui.group(|ui| {
                ui.heading("Hotkeys");

                ui.horizontal(|ui| {
                    ui.label("Keyboard:");
                    let is_recording = self.state.is_recording_kb.load(Ordering::SeqCst);
                    if is_recording {
                        ui.colored_label(egui::Color32::YELLOW, "Listening... [Press any Key]");
                    } else {
                        ui.label(format!("[ {} ]", config.hotkey_kb));
                        if ui.button("Record").clicked() {
                            self.state.is_recording_kb.store(true, Ordering::SeqCst);
                            self.state.is_recording_ctrl.store(false, Ordering::SeqCst);
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Controller:");
                    let is_recording = self.state.is_recording_ctrl.load(Ordering::SeqCst);
                    if is_recording {
                        ui.colored_label(egui::Color32::YELLOW, "Listening... [Press any Button]");
                    } else {
                        ui.label(format!("[ {} ]", config.hotkey_ctrl));
                        if ui.button("Record").clicked() {
                            self.state.is_recording_ctrl.store(true, Ordering::SeqCst);
                            self.state.is_recording_kb.store(false, Ordering::SeqCst);
                        }
                    }
                });
            });

            if changed {
                config.save();
                self.state.config.store(Arc::new(config));
            }

            ui.add_space(10.0);
            ui.label(egui::RichText::new("Note: If the overlay appears on the wrong monitor, use your OS shortcuts (e.g. Win+Shift+Arrow) to move the window.").weak());

            ui.add_space(10.0);

            // Launch / Stop Button
            let is_launched = self.state.is_launched.load(Ordering::SeqCst);
            let btn_text = if is_launched { "Stop Overlay" } else { "Launch Overlay" };
            if ui.button(btn_text).clicked() {
                self.state.is_launched.store(!is_launched, Ordering::SeqCst);
            }

            ui.add_space(10.0);

            let is_visible = self.state.is_visible.load(Ordering::SeqCst);
            ui.horizontal(|ui| {
                ui.label("Overlay Visibility:");
                if is_visible {
                    ui.colored_label(egui::Color32::GREEN, "VISIBLE");
                } else {
                    ui.colored_label(egui::Color32::RED, "HIDDEN");
                }
            });

            let is_connected = self.state.is_connected.load(Ordering::SeqCst);
            ui.horizontal(|ui| {
                ui.label("Rocket League Connection:");
                if is_connected {
                    ui.colored_label(egui::Color32::GREEN, "CONNECTED");
                } else {
                    ui.colored_label(egui::Color32::RED, "DISCONNECTED");
                }
            });

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                if ui.button("Quit App").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });

        // 2. Show Overlay Viewport (The HUD)
        let is_launched = self.state.is_launched.load(Ordering::SeqCst);
        if is_launched {
            let config = self.state.config.load();
            let state = self.state.clone();
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("overlay"),
                egui::ViewportBuilder::default()
                    .with_title("RL Overlay HUD")
                    .with_inner_size(config.window_size)
                    .with_transparent(true)
                    .with_always_on_top()
                    .with_decorations(false)
                    .with_mouse_passthrough(true),
                move |ctx, class| {
                    assert!(class == egui::ViewportClass::Immediate);

                    if state.is_visible.load(Ordering::SeqCst) {
                        render_overlay(ctx, &state);
                    }
                },
            );
        }

        ctx.request_repaint();
    }
}

fn render_overlay(ctx: &egui::Context, state: &Arc<AppState>) {
    let config = state.config.load();
    let players = state.players.load();

    // Position based on AnchorPos
    let (anchor, base_offset) = match config.anchor {
        AnchorPos::TopLeft => (egui::Align2::LEFT_TOP, egui::vec2(20.0, 20.0)),
        AnchorPos::TopRight => (egui::Align2::RIGHT_TOP, egui::vec2(-20.0, 20.0)),
        AnchorPos::BottomLeft => (egui::Align2::LEFT_BOTTOM, egui::vec2(20.0, -20.0)),
        AnchorPos::BottomRight => (egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0, -20.0)),
        AnchorPos::CenterRight => (egui::Align2::RIGHT_CENTER, egui::vec2(-20.0, 0.0)),
    };

    // Apply UI scale to the offset as well
    let offset = base_offset * config.ui_scale;

    egui::Area::new("overlay_area".into())
        .anchor(anchor, offset)
        .show(ctx, |ui| {
            // Apply scale to this UI block
            ui.set_row_height(ui.spacing().interact_size.y * config.ui_scale);

            egui::Frame::none()
                .fill(egui::Color32::from_black_alpha(config.transparency))
                .rounding(5.0 * config.ui_scale)
                .inner_margin(10.0 * config.ui_scale)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        let header_text = egui::RichText::new("Lobby Platforms")
                            .size(14.0 * config.ui_scale)
                            .strong();
                        ui.label(header_text);
                        ui.add_space(5.0 * config.ui_scale);

                        let mut sorted_players: Vec<_> = players
                            .values()
                            .filter(|p| config.show_bots || !p.is_bot)
                            .collect();

                        sorted_players
                            .sort_by(|a, b| a.team.cmp(&b.team).then_with(|| a.name.cmp(&b.name)));

                        if sorted_players.is_empty() {
                            ui.label(
                                egui::RichText::new("Waiting for players...")
                                    .size(12.0 * config.ui_scale)
                                    .italics(),
                            );
                        } else {
                            for p in sorted_players {
                                ui.horizontal(|ui| {
                                    let team_color = if p.team == 0 {
                                        egui::Color32::from_rgb(0, 150, 255)
                                    } else {
                                        egui::Color32::from_rgb(255, 140, 0)
                                    };

                                    let dot = egui::RichText::new("■")
                                        .color(team_color)
                                        .size(12.0 * config.ui_scale);
                                    ui.label(dot);

                                    let name_color = if p.is_bot {
                                        egui::Color32::from_gray(150)
                                    } else {
                                        egui::Color32::WHITE
                                    };

                                    let name = egui::RichText::new(&p.name)
                                        .color(name_color)
                                        .size(12.0 * config.ui_scale);
                                    ui.label(name);

                                    ui.add_space(10.0 * config.ui_scale);

                                    let platform = egui::RichText::new(&p.platform)
                                        .size(12.0 * config.ui_scale)
                                        .strong();
                                    ui.label(platform);
                                });
                            }
                        }
                    });
                });
        });
}
