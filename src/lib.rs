pub mod automation;
pub mod history;
pub mod hoops_fixer;
pub mod json_utils;
pub mod replay_ledger;
pub mod replay_metadata;
pub mod replays;
pub mod stats_api;
pub mod stats_api_parser;

mod assets;
mod diagnostics;
mod input;
mod mmr;
pub mod network;
pub mod session;
mod setup;
pub mod state;
mod ui;
#[cfg(not(feature = "microsoft-store"))]
pub mod update;

use eframe::egui;
use state::AppState;

pub async fn run(debug_enabled: bool) -> eframe::Result<()> {
    let mut builder = env_logger::Builder::new();
    builder.filter_level(log::LevelFilter::Warn);
    let level = if debug_enabled {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    builder.filter_module("rl_platform_overlay", level);
    let _ = builder.try_init();

    let state = AppState::new_with_debug(debug_enabled);

    refresh_stats_api_setup_on_startup(&state);

    history::refresh_totals(&state);

    if state.game.local_player_identity.load().is_known() {
        mmr::start_local_mmr_refresh(state.clone());
    }

    let net_state = state.clone();
    tokio::spawn(async move {
        network::start_network_task(net_state).await;
        log::error!("Network task exited unexpectedly.");
    });

    mmr::start_mmr_fetch_task(state.clone());
    input::start_input_tasks(state.clone());
    #[cfg(not(feature = "microsoft-store"))]
    update::start_version_check(state.clone());
    replays::trigger_replay_upload(state.clone(), true);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([760.0, 820.0])
            .with_min_inner_size([640.0, 600.0])
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
            ui::fonts::install_fallbacks(&cc.egui_ctx);
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(ui::MainApp::new(state, hwnd)))
        }),
    )
}

fn refresh_stats_api_setup_on_startup(state: &std::sync::Arc<AppState>) {
    let config = state.system.config.load();
    let rocket_league_path = config.rocket_league_path.clone();
    let packet_send_rate = config.stats_api_packet_send_rate;
    drop(config);

    match setup::ensure_stats_api_enabled_on_startup(&rocket_league_path, packet_send_rate) {
        Ok(result) => {
            if result.changed {
                log::info!("{}", result.message);
            }
            state
                .system
                .stats_api_setup_result
                .store(std::sync::Arc::new(result));
        }
        Err(error) => {
            log::warn!("Could not verify Stats API config at startup: {error}");
            state
                .system
                .stats_api_setup_result
                .store(std::sync::Arc::new(setup::StatsApiSetupResult {
                    message: error,
                    ..Default::default()
                }));
        }
    }

    state
        .system
        .stats_api_setup_status
        .store(std::sync::Arc::new(setup::inspect_stats_api_setup(
            &rocket_league_path,
        )));
}
