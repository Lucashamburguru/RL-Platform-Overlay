use crate::state::{AppState, AnchorPos};
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
                
                ui.label("UI Scale");
                if ui.add(egui::Slider::new(&mut config.ui_scale, 0.5..=2.5)).changed() {
                    changed = true;
                }

                if ui.checkbox(&mut config.show_bots, "Show Bots").changed() {
                    changed = true;
                }

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
            });

            if changed {
                self.state.config.store(Arc::new(config));
            }
            
            ui.add_space(10.0);
            ui.label(egui::RichText::new("Note: If the overlay appears on the wrong monitor, use your OS shortcuts (e.g. Win+Shift+Arrow) to move the window. Persistent position saving coming soon.").small().weak());
            
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
            ui.label("Use your configured hotkey to toggle the overlay.");
            
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
            let state = self.state.clone();
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("overlay"),
                egui::ViewportBuilder::default()
                    .with_title("RL Overlay HUD")
                    .with_transparent(true)
                    .with_always_on_top()
                    .with_decorations(false)
                    .with_mouse_passthrough(true)
                    .with_inner_size(self.state.config.load().window_size),
                move |ctx, class| {
                    assert!(class == egui::ViewportClass::Immediate);
                    
                    // Clear color MUST be transparent for overlay
                    let visuals = egui::Visuals::dark();
                    ctx.set_visuals(visuals);
                    
                    // Scale ONLY the overlay
                    ctx.set_pixels_per_point(state.config.load().ui_scale);

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
    let (anchor, offset) = match config.anchor {
        AnchorPos::TopLeft => (egui::Align2::LEFT_TOP, egui::vec2(20.0, 20.0)),
        AnchorPos::TopRight => (egui::Align2::RIGHT_TOP, egui::vec2(-20.0, 20.0)),
        AnchorPos::BottomLeft => (egui::Align2::LEFT_BOTTOM, egui::vec2(20.0, -20.0)),
        AnchorPos::BottomRight => (egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0, -20.0)),
        AnchorPos::CenterRight => (egui::Align2::RIGHT_CENTER, egui::vec2(-20.0, 0.0)),
    };

    egui::Area::new("overlay_area".into())
        .anchor(anchor, offset)
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(egui::Color32::from_black_alpha(config.transparency))
                .rounding(5.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.heading("Lobby Platforms");
                    ui.add_space(5.0);
                    
                    let mut sorted_players: Vec<_> = players.values()
                        .filter(|p| config.show_bots || !p.is_bot)
                        .collect();
                        
                    sorted_players.sort_by(|a, b| {
                        a.team.cmp(&b.team).then_with(|| a.name.cmp(&b.name))
                    });

                    if sorted_players.is_empty() {
                        ui.label(egui::RichText::new("Waiting for players...").italics());
                    } else {
                        for p in sorted_players {
                            ui.horizontal(|ui| {
                                let color = if p.team == 0 {
                                    egui::Color32::from_rgb(0, 150, 255) // Blue team
                                } else {
                                    egui::Color32::from_rgb(255, 140, 0) // Orange team
                                };
                                ui.colored_label(color, "■");
                                ui.label(&p.name);
                                ui.add_space(10.0);
                                ui.label(egui::RichText::new(&p.platform).strong());
                            });
                        }
                    }
                });
        });
    
    ctx.request_repaint();
}
