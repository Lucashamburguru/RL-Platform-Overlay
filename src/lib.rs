pub mod hoops_fixer;
pub mod json_utils;
pub mod replays;
pub mod stats_api;

mod assets;
mod input;
mod mmr;
mod network;
mod session;
mod setup;
mod state;
mod ui;
mod update;

use eframe::egui;
use state::AppState;

pub async fn run(debug_enabled: bool) -> eframe::Result<()> {
    let state = AppState::new_with_debug(debug_enabled);

    if state.local_player_identity.load().is_known() {
        mmr::start_local_mmr_refresh(state.clone());
    }

    let net_state = state.clone();
    tokio::spawn(async move {
        let network_task =
            tokio::spawn(async move { network::start_network_task(net_state).await });
        match network_task.await {
            Ok(()) => eprintln!("Network task exited unexpectedly."),
            Err(error) => eprintln!("Network task failed: {error}"),
        }
    });

    mmr::start_mmr_fetch_task(state.clone());
    input::start_input_tasks(state.clone());
    update::start_version_check(state.clone());
    replays::trigger_replay_upload(state.clone(), true);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([760.0, 820.0])
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
        Box::new(|cc| {
            #[allow(unused_mut)]
            let mut hwnd = None;
            #[cfg(target_os = "windows")]
            {
                use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                if let Ok(handle) = cc.window_handle()
                    && let RawWindowHandle::Win32(win32_handle) = handle.as_raw()
                {
                    hwnd = Some(win32_handle.hwnd.get());
                }
            }

            egui_extras::install_image_loaders(&cc.egui_ctx);
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(ui::MainApp::new(state, hwnd)))
        }),
    )
}
