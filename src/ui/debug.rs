use crate::state::{AppState, DebugCaptureStatus};
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::common::debug_status_row;

pub(super) fn render_debug_settings_tab(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    is_launched: bool,
    is_rl_running: bool,
    rl_process_detection_detail: &str,
) {
    ui.group(|ui| {
        ui.heading("Parsed State");
        debug_status_row(
            ui,
            "Overlay",
            if is_launched { "Launched" } else { "Settings" },
        );
        debug_status_row(
            ui,
            "Connection",
            if state.flags.is_connected.load(Ordering::SeqCst) {
                "Connected"
            } else {
                "Disconnected"
            },
        );
        let process_status = if rl_process_detection_detail.trim().is_empty() {
            if is_rl_running {
                "Detected".to_string()
            } else {
                "Not detected".to_string()
            }
        } else if is_rl_running {
            format!("Detected ({rl_process_detection_detail})")
        } else {
            format!("Not detected ({rl_process_detection_detail})")
        };
        debug_status_row(ui, "Rocket League Process", &process_status);
        let local_name = state.game.local_player_name.load();
        debug_status_row(ui, "Local Player", local_name.as_str());
        let local_team = state.game.local_team.load(Ordering::SeqCst);
        let team_text = crate::state::standard_team(local_team)
            .map(|team| team.to_string())
            .unwrap_or_else(|| "Unknown".to_string());
        debug_status_row(ui, "Local Team", &team_text);

        let players = state.game.players.load();
        debug_status_row(ui, "Players", &players.len().to_string());
        debug_status_row(
            ui,
            "Hotkey Log",
            &crate::input::hotkey_debug_log_path().display().to_string(),
        );
        if ui.button("Clear Hotkey Log").clicked() {
            let path = crate::input::hotkey_debug_log_path();
            if path.exists() {
                match std::fs::write(&path, "") {
                    Ok(()) => crate::input::append_hotkey_debug_log(
                        state.debug_logging_enabled.load(Ordering::SeqCst),
                        "hotkey_log_cleared",
                    ),
                    Err(error) => crate::input::append_hotkey_debug_log(
                        state.debug_logging_enabled.load(Ordering::SeqCst),
                        format!("hotkey_log_clear_failed error={error}"),
                    ),
                }
            }
        }

        let diagnostics = state.system.network_diagnostics.load();
        debug_status_row(ui, "Transport", diagnostics.transport.label());
        debug_status_row(ui, "Last Event", diagnostics.last_event.as_str());
        debug_status_row(
            ui,
            "Last Event ms",
            &diagnostics.last_event_unix_ms.to_string(),
        );
        if !diagnostics.last_parse_error.is_empty() {
            debug_status_row(
                ui,
                "Last Parse Error",
                diagnostics.last_parse_error.as_str(),
            );
        }
        if !diagnostics.last_connection_error.is_empty() {
            debug_status_row(
                ui,
                "Last Connection Error",
                diagnostics.last_connection_error.as_str(),
            );
        }

        ui.separator();
        let version_check = state.system.version_check.load();
        debug_status_row(ui, "Current Version", env!("CARGO_PKG_VERSION"));
        let version_status = if !version_check.checked {
            "Checking...".to_string()
        } else if version_check.update_available {
            format!("Update available ({})", version_check.latest_tag)
        } else if !version_check.error.is_empty() {
            version_check.error.clone()
        } else {
            format!("Up to date ({})", version_check.latest_tag)
        };
        debug_status_row(ui, "Version Check", &version_status);

        let config_status = state.system.config_status.load();
        debug_status_row(ui, "Config Path", &config_status.path);
        debug_status_row(
            ui,
            "Config Status",
            if config_status.last_error.is_empty() {
                "OK"
            } else {
                config_status.last_error.as_str()
            },
        );

        ui.separator();
        for player in players.values() {
            ui.label(format!(
                "{} | team {} | {} | boost {}",
                player.name, player.team, player.platform, player.boost
            ));
        }
    });

    ui.add_space(10.0);
    render_performance_diagnostics(ui, state);

    ui.add_space(10.0);
    ui.group(|ui| {
        ui.heading("In-Game MMR Provider Logs");
        ui.add_space(6.0);

        let logs = if let Ok(lock) = state.mmr.debug_tracker_logs.lock() {
            lock.clone()
        } else {
            std::collections::VecDeque::new()
        };

        if ui.button("Clear MMR Logs").clicked()
            && let Ok(mut lock) = state.mmr.debug_tracker_logs.lock()
        {
            lock.clear();
        }
        ui.add_space(6.0);

        if logs.is_empty() {
            ui.label("No MMR profiles fetched yet in this session.");
        } else {
            egui::ScrollArea::vertical()
                .max_height(120.0)
                .show(ui, |ui| {
                    for log_line in logs.iter().rev() {
                        let color = if log_line.contains("Success") {
                            egui::Color32::from_rgb(100, 220, 100)
                        } else if log_line.contains("Error") {
                            egui::Color32::from_rgb(230, 80, 80)
                        } else {
                            egui::Color32::from_gray(160)
                        };
                        ui.label(
                            egui::RichText::new(log_line)
                                .font(egui::FontId::monospace(10.0))
                                .color(color),
                        );
                    }
                });
        }
    });

    ui.add_space(10.0);
    ui.group(|ui| {
        ui.heading("Stats API Capture");
        let capture = state.diagnostics.debug_capture_status.load();
        if capture.running {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.label(format!(
                    "Capturing {} seconds of Stats API output...",
                    capture.seconds
                ));
            });
        } else {
            ui.horizontal(|ui| {
                if ui.button("Capture 5s Output").clicked() {
                    start_debug_capture(state.clone(), 5);
                }
                if ui.button("Capture 30s Output").clicked() {
                    start_debug_capture(state.clone(), 30);
                }
            });
        }

        render_capture_status(ui, &capture);
    });

    ui.add_space(10.0);
    ui.group(|ui| {
        ui.heading("MMR Provider Debugger");
        ui.add_space(6.0);

        // Platform selection persistent state
        let platform_id = ui.make_persistent_id("debug_scrape_platform");
        let mut platform = ui.data(|d| {
            d.get_temp::<String>(platform_id)
                .unwrap_or_else(|| "epic".to_string())
        });

        // Name input persistent state
        let name_id = ui.make_persistent_id("debug_scrape_name");
        let mut name = ui.data(|d| {
            d.get_temp::<String>(name_id)
                .unwrap_or_else(|| "".to_string())
        });

        ui.horizontal(|ui| {
            ui.label("Platform:");
            egui::ComboBox::new("debug_platform_combo", "")
                .selected_text(match platform.as_str() {
                    "steam" => "Steam",
                    "psn" => "PlayStation",
                    "xbl" => "Xbox",
                    "switch" => "Switch",
                    _ => "Epic",
                })
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_value(&mut platform, "epic".to_string(), "Epic")
                        .changed()
                    {
                        ui.data_mut(|d| d.insert_temp(platform_id, platform.clone()));
                    }
                    if ui
                        .selectable_value(&mut platform, "steam".to_string(), "Steam")
                        .changed()
                    {
                        ui.data_mut(|d| d.insert_temp(platform_id, platform.clone()));
                    }
                    if ui
                        .selectable_value(&mut platform, "psn".to_string(), "PlayStation")
                        .changed()
                    {
                        ui.data_mut(|d| d.insert_temp(platform_id, platform.clone()));
                    }
                    if ui
                        .selectable_value(&mut platform, "xbl".to_string(), "Xbox")
                        .changed()
                    {
                        ui.data_mut(|d| d.insert_temp(platform_id, platform.clone()));
                    }
                    if ui
                        .selectable_value(&mut platform, "switch".to_string(), "Switch")
                        .changed()
                    {
                        ui.data_mut(|d| d.insert_temp(platform_id, platform.clone()));
                    }
                });
        });

        ui.horizontal(|ui| {
            ui.label("Account ID:");
            if ui.text_edit_singleline(&mut name).changed() {
                ui.data_mut(|d| d.insert_temp(name_id, name.clone()));
            }
        });

        ui.add_space(6.0);

        let status = if let Ok(lock) = state.mmr.debug_scrape_status.lock() {
            lock.clone()
        } else {
            "Idle".to_string()
        };

        let is_fetching = status == "Fetching...";

        ui.horizontal(|ui| {
            let fetch_btn = ui.add_enabled(
                !is_fetching && !name.trim().is_empty(),
                egui::Button::new("Fetch MMR Profile"),
            );
            if fetch_btn.clicked() {
                run_mmr_provider_debug(state.clone(), platform, name);
            }

            if is_fetching {
                ui.add(egui::Spinner::new());
            }
        });

        if status != "Idle" && !is_fetching {
            ui.add_space(8.0);
            ui.label("Result:");
            egui::ScrollArea::vertical()
                .max_height(150.0)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(&status)
                            .font(egui::FontId::monospace(10.0))
                            .color(if status.starts_with("Success") {
                                egui::Color32::from_rgb(120, 220, 120)
                            } else {
                                egui::Color32::from_rgb(220, 120, 120)
                            }),
                    );
                });
        }
    });
}

fn render_performance_diagnostics(ui: &mut egui::Ui, state: &Arc<AppState>) {
    ui.group(|ui| {
        ui.heading("Performance Diagnostics");
        ui.add_space(6.0);

        let is_polling = state
            .diagnostics
            .resource_poller
            .lock()
            .map(|p| p.is_running())
            .unwrap_or(false);
        if ui
            .button(if is_polling {
                "Stop Resource Polling"
            } else {
                "Start Resource Polling"
            })
            .clicked()
            && let Ok(mut poller) = state.diagnostics.resource_poller.lock()
        {
            if is_polling {
                poller.stop();
            } else {
                poller.start();
            }
        }
        ui.add_space(6.0);

        let recording = state.diagnostics.foreground_tracker.enabled();
        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new(if recording {
                    "■ Stop Recording"
                } else {
                    "▶ Start Recording"
                }))
                .clicked()
            {
                let next_recording = !recording;
                state.diagnostics.foreground_tracker.set_enabled(next_recording);
                state.diagnostics.frame_tracker.set_enabled(next_recording);

                if let Ok(mut poller) = state.diagnostics.resource_poller.lock() {
                    if next_recording && !poller.is_running() {
                        poller.start();
                    } else if !next_recording && poller.is_running() {
                        poller.stop();
                    }
                }
            }
            ui.label(if recording {
                "Recording foreground-window changes..."
            } else {
                "Start before Alt-Tabbing out of Rocket League, then return here to inspect the timeline."
            });
        });

        if !recording {
            ui.add_space(6.0);
        }

        let events = state.diagnostics.foreground_tracker.events();
        let process_samples = state.diagnostics.foreground_tracker.process_samples();
        let system_diagnostics = crate::diagnostics::system_diagnostics();
        render_foreground_timeline(ui, &events);

        // System diagnostics (Windows-specific)
        ui.add_space(6.0);
        ui.separator();
        ui.label(
            egui::RichText::new("System (cached for 5s)")
                .size(10.0)
                .strong(),
        );
        for (label, value) in system_diagnostics.iter() {
            debug_status_row(ui, label, value);
        }

        ui.add_space(6.0);
        debug_status_row(
            ui,
            "Diagnostics Log",
            &crate::diagnostics::alt_tab_diagnostics_log_path()
                .display()
                .to_string(),
        );
        debug_status_row(
            ui,
            "Process Samples",
            &format!("{} collected", process_samples.len()),
        );
        if ui.button("Save Alt-Tab Diagnostics Log").clicked() {
            let frame_stats = state.diagnostics.frame_tracker.stats();
            match crate::diagnostics::write_alt_tab_diagnostics_log(
                &events,
                &process_samples,
                &system_diagnostics,
                &state.diagnostics.resource_tracker.get_snapshots(),
                &frame_stats,
            ) {
                Ok(path) => {
                    let message = format!("Saved {}", path.display());
                    if let Ok(mut status) = state.diagnostics.alt_tab_diagnostics_status.lock() {
                        *status = message.clone();
                    }
                    crate::input::append_hotkey_debug_log(state.debug_logging_enabled.load(Ordering::SeqCst), format!(
                        "alt_tab_diagnostics_saved path={}",
                        path.display()
                    ));
                }
                Err(error) => {
                    if let Ok(mut status) = state.diagnostics.alt_tab_diagnostics_status.lock() {
                        *status = format!("Error: {error}");
                    }
                    crate::input::append_hotkey_debug_log(state.debug_logging_enabled.load(Ordering::SeqCst), format!(
                        "alt_tab_diagnostics_save_failed error={error}"
                    ));
                }
            }
        }
        let status = state
            .diagnostics
            .alt_tab_diagnostics_status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_else(|_| "Unavailable".to_string());
        debug_status_row(ui, "Log Status", &status);
    });
}

fn render_foreground_timeline(ui: &mut egui::Ui, events: &[crate::diagnostics::FocusEvent]) {
    ui.add_space(6.0);
    ui.separator();
    ui.label(
        egui::RichText::new("Alt-Tab / Foreground Timeline")
            .size(10.0)
            .strong(),
    );

    if events.is_empty() {
        ui.label("No foreground-window changes recorded yet.");
        return;
    }

    egui::ScrollArea::vertical()
        .max_height(110.0)
        .show(ui, |ui| {
            for event in events.iter().rev() {
                let status = if event.rocket_league_foreground {
                    "RL foreground"
                } else {
                    "RL unfocused"
                };
                let color = if event.rocket_league_foreground {
                    egui::Color32::from_rgb(100, 220, 140)
                } else {
                    egui::Color32::from_rgb(230, 200, 80)
                };
                ui.label(
                    egui::RichText::new(format!(
                        "{:>6.2}s | {:<13} | {} | {}",
                        event.elapsed_ms as f64 / 1000.0,
                        status,
                        event.process_name,
                        event.title
                    ))
                    .font(egui::FontId::monospace(10.0))
                    .color(color),
                );
            }
        });
}

fn run_mmr_provider_debug(state: Arc<AppState>, platform: String, player_name_or_id: String) {
    if let Ok(mut status) = state.mmr.debug_scrape_status.lock() {
        *status = "Fetching...".to_string();
    }

    tokio::spawn(async move {
        let player = crate::mmr::TrackerPlayer {
            platform: platform.clone(),
            player_name: player_name_or_id.clone(),
            player_id: player_name_or_id.clone(),
            primary_id: if player_name_or_id.matches('|').count() >= 2 {
                player_name_or_id.clone()
            } else {
                String::new()
            },
        };

        let result = crate::mmr::fetch_mmr_snapshot(
            &state.system.http_client,
            Some(&state.mmr.xuid_gamertag_cache),
            &player,
        )
        .await;

        let status_msg = match result {
            Ok(snapshot) => {
                let mut lines = Vec::new();
                lines.push(format!(
                    "Success! Fetched Profile for {}/{}",
                    platform, player_name_or_id
                ));
                if let Some(season) = snapshot.current_season {
                    lines.push(format!("Current Season: {}", season));
                }
                if let Some(updated) = snapshot.last_updated {
                    lines.push(format!("Last Updated: {}", updated));
                }
                lines.push(String::new());
                lines.push("Playlists:".to_string());
                for (id, playlist) in snapshot.playlists {
                    lines.push(format!(
                        "  * {} (ID {}): {} MMR | Tier: {} | Matches: {}",
                        playlist.name, id, playlist.rating, playlist.tier_name, playlist.matches
                    ));
                }
                lines.join("\n")
            }
            Err(e) => {
                format!(
                    "Error fetching profile for {}/{}: {}",
                    platform, player_name_or_id, e
                )
            }
        };

        if let Ok(mut status) = state.mmr.debug_scrape_status.lock() {
            *status = status_msg;
        }
    });
}

fn render_capture_status(ui: &mut egui::Ui, capture: &DebugCaptureStatus) {
    if !capture.message.is_empty() {
        ui.colored_label(egui::Color32::from_rgb(100, 220, 100), &capture.message);
    }
    if !capture.error.is_empty() {
        ui.colored_label(egui::Color32::from_rgb(230, 80, 80), &capture.error);
    }
    if !capture.last_output_path.is_empty() {
        debug_status_row(ui, "Output", &capture.last_output_path);
    }
}

fn start_debug_capture(state: Arc<AppState>, seconds: u64) {
    let output = crate::stats_api::default_capture_path(crate::state::config_dir());
    state
        .diagnostics
        .debug_capture_status
        .store(Arc::new(DebugCaptureStatus {
            running: true,
            seconds,
            last_output_path: output.display().to_string(),
            message: String::new(),
            error: String::new(),
        }));

    tokio::spawn(async move {
        let result = crate::stats_api::capture_to_file(&output, seconds).await;
        let status = match result {
            Ok(()) => DebugCaptureStatus {
                running: false,
                seconds,
                last_output_path: output.display().to_string(),
                message: "Capture complete.".to_string(),
                error: String::new(),
            },
            Err(error) => DebugCaptureStatus {
                running: false,
                seconds,
                last_output_path: output.display().to_string(),
                message: String::new(),
                error: format!("Capture failed: {error}"),
            },
        };
        state
            .diagnostics
            .debug_capture_status
            .store(Arc::new(status));
    });
}
