mod state;
mod network;
mod input;
mod ui;

use crate::state::AppState;
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
            .with_inner_size([400.0, 300.0])
            .with_decorations(true)
            .with_title("RL Overlay Settings"),
        ..Default::default()
    };

    eframe::run_native(
        "RL Overlay Settings",
        options,
        Box::new(|_cc| Box::new(ui::MainApp::new(state))),
    )
}
