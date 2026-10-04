//! Manual README capture. Render real settings pages with isolated sample data.
use super::*;
use crate::history::{HistoryPlayersSnapshot, HistoryTotals, PlayerHistorySummary};
use crate::item_swapper::{CatalogPackage, ItemSlot, ItemSwapperSnapshot};
use crate::replay_metadata::{ReplayMetadataEntry, ReplayMetadataSnapshot, ReplayPlayerMetadata};

#[tokio::test]
#[ignore = "manual README capture; set RL_PAGE_TOUR_DIR"]
async fn capture_readme_pages() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("RL_PAGE_TOUR_DIR").expect("set RL_PAGE_TOUR_DIR"),
    );
    std::fs::create_dir_all(&directory).unwrap();
    let sample_install = directory.join("RocketLeague");
    let sample_ini = crate::setup::stats_ini_path(sample_install.to_str().unwrap());
    std::fs::create_dir_all(sample_ini.parent().unwrap()).unwrap();
    std::fs::write(
        &sample_ini,
        "[TAGame.MatchStatsExporter_TA]\nPacketSendRate=30\nPort=49123\n",
    )
    .unwrap();
    let state = AppState::new_with_debug(true);
    state.update_config(|config| {
        config.rocket_league_path.clear();
        config.replays_folder.clear();
        config.ballchasing_api_key.clear();
        config.history_enabled = true;
    });
    state.history.totals.store(Arc::new(HistoryTotals {
        matches: 66,
        players: 6,
        wins: 40,
        losses: 26,
    }));
    state
        .history
        .all_players_snapshot
        .store(Arc::new(HistoryPlayersSnapshot {
            loaded: true,
            players: [
                "cyberPeng",
                "Atlas5563",
                "Sokyo69",
                "platymusPrime",
                "Next player",
                "Teammate",
            ]
            .into_iter()
            .enumerate()
            .map(|(i, name)| PlayerHistorySummary {
                player_key: format!("sample-{i}"),
                name: name.into(),
                name_normalized: name.to_lowercase(),
                platform: if i % 2 == 0 {
                    "Steam".into()
                } else {
                    "Epic".into()
                },
                games_with: 12,
                games_against: 23,
                wins_with: [7, 4, 6][i % 3],
                losses_with: [5, 8, 6][i % 3],
                wins_against: 15,
                losses_against: 8,
                last_seen_unix_ms: 1_791_000_000_000 - i as i64 * 3_600_000,
                ..Default::default()
            })
            .collect(),
            ..Default::default()
        }));
    state
        .replays
        .merged_metadata_cache
        .store(Arc::new(ReplayMetadataSnapshot {
            entries: (0..8)
                .map(|i| {
                    let entry = ReplayMetadataEntry {
                        filename: format!("sample-{i}.replay"),
                        display_name: [
                            "Overtime winner",
                            "Ranked Doubles",
                            "Passing play",
                            "Hoops match",
                        ][i % 4]
                            .into(),
                        date: format!("2026-10-02:19-{:02}-00", 50 - i),
                        map_name: "Stadium_P".into(),
                        match_type: "Online".into(),
                        file_size: 1024,
                        game_replay_id: format!("{i:032x}"),
                        team0_score: Some(3),
                        team1_score: Some(2),
                        duration_seconds: Some(327),
                        players: [
                            ("cyberPeng", 0),
                            ("Teammate", 0),
                            ("Opponent", 1),
                            ("Second opponent", 1),
                        ]
                        .into_iter()
                        .map(|(name, team)| ReplayPlayerMetadata {
                            name: name.into(),
                            team: Some(team),
                            score: Some(515),
                            goals: Some(2),
                            assists: Some(1),
                            saves: Some(3),
                            shots: Some(4),
                            is_bot: Some(false),
                        })
                        .collect(),
                        ..Default::default()
                    };
                    (entry.filename.clone(), entry)
                })
                .collect(),
            ..Default::default()
        }));
    state
        .item_swapper
        .snapshot
        .store(Arc::new(ItemSwapperSnapshot {
            catalog_loaded: true,
            message: "Sample catalog".into(),
            packages: [
                ("Standard", ItemSlot::RocketBoost),
                ("Gold Rush", ItemSlot::RocketBoost),
                ("Sparkles", ItemSlot::RocketBoost),
                ("Octane", ItemSlot::EngineAudio),
                ("Dominus", ItemSlot::EngineAudio),
                ("Fennec", ItemSlot::EngineAudio),
            ]
            .into_iter()
            .enumerate()
            .map(|(i, (name, slot))| CatalogPackage {
                package: format!("sample-{i}"),
                labels: vec![name.into()],
                slot,
                key: String::new(),
                unavailable: None,
            })
            .collect(),
            ..Default::default()
        }));
    for (name, tab, subtab, click, engine) in [
        (
            "01-setup",
            SettingsTab::Setup,
            OverlaySubtab::Lobby,
            Some("Technical connection details"),
            false,
        ),
        (
            "02-lobby",
            SettingsTab::Overlay,
            OverlaySubtab::Lobby,
            None,
            false,
        ),
        (
            "03-session",
            SettingsTab::Overlay,
            OverlaySubtab::Session,
            None,
            false,
        ),
        (
            "04-boost",
            SettingsTab::Overlay,
            OverlaySubtab::Boost,
            None,
            false,
        ),
        (
            "05-dashboard",
            SettingsTab::Dashboard,
            OverlaySubtab::Lobby,
            None,
            false,
        ),
        (
            "06-item-swapper",
            SettingsTab::ItemSwapper,
            OverlaySubtab::Lobby,
            None,
            false,
        ),
        (
            "07-engine-audio",
            SettingsTab::ItemSwapper,
            OverlaySubtab::Lobby,
            None,
            true,
        ),
        (
            "08-uploader",
            SettingsTab::Replays,
            OverlaySubtab::Lobby,
            None,
            false,
        ),
        (
            "09-replay-library",
            SettingsTab::Replays,
            OverlaySubtab::Lobby,
            Some("Replay Library"),
            false,
        ),
        (
            "10-replay-tools",
            SettingsTab::Replays,
            OverlaySubtab::Lobby,
            Some("Tools & Maintenance"),
            false,
        ),
        (
            "09b-replay-details",
            SettingsTab::Replays,
            OverlaySubtab::Lobby,
            Some("Replay Library"),
            false,
        ),
        (
            "11-history",
            SettingsTab::History,
            OverlaySubtab::Lobby,
            None,
            false,
        ),
        (
            "12-support",
            SettingsTab::Support,
            OverlaySubtab::Lobby,
            None,
            false,
        ),
        (
            "11b-history-record",
            SettingsTab::History,
            OverlaySubtab::Lobby,
            Some("▸ cyberPeng"),
            false,
        ),
        (
            "13-debug",
            SettingsTab::Debug,
            OverlaySubtab::Lobby,
            None,
            false,
        ),
    ] {
        if tab == SettingsTab::Setup {
            let install_path = sample_install.to_str().unwrap().to_owned();
            state.update_config(|config| {
                config.rocket_league_path = install_path.clone();
                config.stats_api_packet_send_rate = 30;
            });
            let status = crate::setup::inspect_stats_api_setup(&install_path);
            assert!(status.installation_found && status.configured);
            state.system.stats_api_setup_status.store(Arc::new(status));
            state.flags.is_connected.store(true, Ordering::SeqCst);
        }
        let ctx = egui::Context::default();
        crate::ui::fonts::install_fallbacks(&ctx);
        egui_extras::install_image_loaders(&ctx);
        let mut app = MainApp::new(state.clone(), None);
        app.settings_tab = tab;
        app.overlay_subtab = subtab;
        app.last_rl_check = std::time::Instant::now();
        app.is_rl_running = tab == SettingsTab::Setup;
        if engine {
            app.item_swapper_ui.slot = Some(ItemSlot::EngineAudio);
        }
        let size = [1000.0, 820.0];
        let mut renderer = crate::ui::review_renderer::ReviewRenderer::default();
        let mut click_position = None;
        for frame in 0..12 {
            if tab == SettingsTab::Setup {
                state.system.network_diagnostics.store(Arc::new(
                    crate::state::NetworkDiagnostics {
                        last_event: "UpdateState".into(),
                        last_event_unix_ms: crate::stats_api::now_ms(),
                        ..Default::default()
                    },
                ));
            }
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(size[0], size[1]),
                )),
                ..Default::default()
            };
            let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
            viewport.inner_rect = input.screen_rect;
            viewport.native_pixels_per_point = Some(1.0);
            if let Some(pos) = click_position.filter(|_| matches!(frame, 3 | 4 | 7 | 8)) {
                input.events.push(egui::Event::PointerMoved(pos));
                input.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: frame == 3 || frame == 7,
                    modifiers: egui::Modifiers::default(),
                });
            }
            let output = ctx.run(input, |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| app.render_settings_content(ui, ctx, false));
            });
            if frame == 2
                && let Some(label) = click
            {
                click_position = output.shapes.iter().find_map(|shape| {
                    if let egui::epaint::Shape::Text(text) = &shape.shape
                        && text.galley.text() == label
                    {
                        let mut position = text.visual_bounding_rect().center();
                        if label.starts_with("▸") {
                            position.x = 400.0;
                        }
                        Some(position)
                    } else {
                        None
                    }
                });
                assert!(click_position.is_some(), "missing {label}");
            }
            if frame == 6 {
                click_position = if name == "09b-replay-details" {
                    let position = output.shapes.iter().find_map(|shape| {
                        if let egui::epaint::Shape::Text(text) = &shape.shape
                            && text.galley.text() == "▸ Overtime winner"
                        {
                            Some(egui::pos2(400.0, text.visual_bounding_rect().center().y))
                        } else {
                            None
                        }
                    });
                    assert!(position.is_some(), "missing replay row");
                    position
                } else {
                    None
                };
            }
            let path = directory.join(format!("{name}.png"));
            if frame == 11 && tab == SettingsTab::Setup {
                assert!(output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.text() == "Ready for live matches")
                }), "sample setup is not ready");
            }
            if frame == 11 && matches!(name, "09b-replay-details" | "11b-history-record") {
                let expected = if name == "09b-replay-details" {
                    "Players"
                } else {
                    "Together"
                };
                assert!(output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.text() == expected)
                }), "details did not expand: {name}");
            }
            renderer.capture(&ctx, &output, size, (frame == 11).then_some(path.as_path()));
        }
        if tab == SettingsTab::Setup {
            state.update_config(|config| config.rocket_league_path.clear());
            state.flags.is_connected.store(false, Ordering::SeqCst);
            state
                .system
                .stats_api_setup_status
                .store(Arc::new(Default::default()));
            state
                .system
                .network_diagnostics
                .store(Arc::new(Default::default()));
        }
    }
}
