mod assets;
mod input;
mod mmr;
mod network;
mod session;
mod setup;
mod state;
mod stats_api;
mod ui;
mod update;

use crate::state::AppState;
use eframe::egui;

#[tokio::main]
async fn main() -> eframe::Result<()> {
    let debug_enabled = std::env::args().any(|arg| arg == "--debug");
    let state = AppState::new_with_debug(debug_enabled);

    // Start background tasks
    let net_state = state.clone();
    tokio::spawn(async move {
        network::start_network_task(net_state).await;
    });

    mmr::start_mmr_fetch_task(state.clone());
    input::start_input_tasks(state.clone());
    update::start_version_check(state.clone());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([720.0, 820.0])
            .with_transparent(true)
            .with_decorations(false)
            .with_title("RL Overlay Settings"),
        depth_buffer: 0,
        stencil_buffer: 0,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "RL Overlay Settings",
        options,
        Box::new(|_cc| {
            egui_extras::install_image_loaders(&_cc.egui_ctx);
            _cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(ui::MainApp::new(state)))
        }),
    )
}
