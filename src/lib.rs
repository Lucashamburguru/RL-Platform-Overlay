pub mod json_utils;
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

#[cfg(target_os = "windows")]
fn configure_windows_transparent_window(cc: &eframe::CreationContext<'_>) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winapi::shared::windef::HWND;
    use winapi::um::dwmapi::DwmExtendFrameIntoClientArea;
    use winapi::um::uxtheme::MARGINS;
    use winapi::um::winuser::{
        GWL_EXSTYLE, GetWindowLongW, LWA_ALPHA, SetLayeredWindowAttributes, SetWindowLongW,
        WS_EX_LAYERED,
    };

    let Ok(window_handle) = cc.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = window_handle.as_raw() else {
        return;
    };
    let hwnd = handle.hwnd.get() as HWND;

    unsafe {
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_LAYERED as i32);
        SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);

        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        DwmExtendFrameIntoClientArea(hwnd, &margins);
    }
}

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
        Box::new(|cc| {
            #[cfg(target_os = "windows")]
            configure_windows_transparent_window(cc);

            egui_extras::install_image_loaders(&cc.egui_ctx);
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(ui::MainApp::new(state)))
        }),
    )
}
