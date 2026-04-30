use crate::state::AppState;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use eframe::egui;

pub struct OverlayApp {
    state: Arc<AppState>,
}

impl OverlayApp {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

impl eframe::App for OverlayApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let is_visible = self.state.is_visible.load(Ordering::SeqCst);
        if !is_visible {
            ctx.request_repaint();
            return;
        }

        let players = self.state.players.load();
        if !players.is_empty() {
            println!("UI Rendering {} players", players.len());
        }
        
        egui::Area::new("overlay".into())
            .anchor(egui::Align2::RIGHT_CENTER, egui::vec2(-20.0, 0.0))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(egui::Color32::from_black_alpha(150))
                    .rounding(5.0)
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.heading("Lobby Platforms");
                        ui.add_space(5.0);
                        
                        let mut sorted_players: Vec<_> = players.values().collect();
                        sorted_players.sort_by_key(|p| p.team);

                        for p in sorted_players {
                            ui.horizontal(|ui| {
                                let color = if p.team == 0 {
                                    egui::Color32::from_rgb(0, 100, 255)
                                } else {
                                    egui::Color32::from_rgb(255, 140, 0)
                                };
                                ui.colored_label(color, "■");
                                ui.label(&p.name);
                                ui.label(egui::RichText::new(&p.platform).strong());
                            });
                        }
                    });
            });
        
        ctx.request_repaint();
    }
}
