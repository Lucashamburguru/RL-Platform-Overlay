mod state;
mod network;
mod input;
mod ui;

use crate::state::AppState;
use crate::ui::OverlayApp;
use eframe::egui;

#[tokio::main]
async fn main() -> eframe::Result<()> {
    let state = AppState::new();
    
    // Start background tasks
    let net_state = state.clone();
    tokio::spawn(async move {
        network::start_network_task(net_state).await;
    });
    
    input::start_input_tasks(state.clone());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_transparent(true)
            .with_always_on_top()
            .with_decorations(false)
            .with_active(false) // Don't steal focus
            .with_mouse_passthrough(true),
        ..Default::default()
    };

    eframe::run_native(
        "RL Platform Overlay",
        options,
        Box::new(|_cc| Box::new(OverlayApp::new(state))),
    )
}
