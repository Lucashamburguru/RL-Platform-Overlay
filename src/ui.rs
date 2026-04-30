use crate::state::AppState;
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
        // Settings Window UI
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
            });

            if changed {
                self.state.config.store(Arc::new(config));
            }
            
            ui.add_space(20.0);
            let is_visible = self.state.is_visible.load(Ordering::SeqCst);
            ui.horizontal(|ui| {
                ui.label("Overlay Status:");
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
        });

        // Spawn/Render Overlay Window
        if self.state.is_visible.load(Ordering::SeqCst) {
            let state = self.state.clone();
            let overlay_id = egui::ViewportId::from_hash_of("rl_overlay");
            
            ctx.show_viewport_immediate(
                overlay_id,
                egui::ViewportBuilder::default()
                    .with_transparent(true)
                    .with_decorations(false)
                    .with_always_on_top()
                    .with_mouse_passthrough(true)
                    .with_title("RL Platform Overlay"),
                move |ctx, _class| {
                    render_overlay(ctx, &state);
                },
            );
        }

        ctx.request_repaint();
    }
}

fn render_overlay(ctx: &egui::Context, state: &Arc<AppState>) {
    let config = state.config.load();
    let players = state.players.load();
    
    // Apply UI scale
    ctx.set_zoom_factor(config.ui_scale);

    egui::Area::new("overlay_area".into())
        .anchor(egui::Align2::RIGHT_CENTER, egui::vec2(-20.0, 0.0))
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
    
    // We need to request repaint to keep the overlay updating
    ctx.request_repaint();
}
