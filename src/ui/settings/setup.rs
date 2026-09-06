use crate::state::{ApiLogExportStatus, AppState, Config};
use crate::ui::common::{
    StatusTone, debug_status_row, helper_text, setting_row, settings_section, status_color,
    status_text,
};
use crate::ui::hotkeys::render_hotkey_settings_section;
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub(crate) fn render_setup_settings_tab(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &Arc<AppState>,
    config_edit: &mut Config,
    changed: &mut bool,
    is_rl_running: bool,
) {
    render_setup_readiness(ui, state, is_rl_running);
    ui.add_space(12.0);
    settings_section(ui, "Stats API Setup", |ui| {
        setting_row(ui, "Rocket League Folder", |ui| {
            ui.horizontal(|ui| {
                let input_width = (ui.available_width() - 120.0).max(80.0);
                if ui
                    .add_sized(
                        [input_width, 22.0],
                        egui::TextEdit::singleline(&mut config_edit.rocket_league_path),
                    )
                    .changed()
                {
                    *changed = true;
                    request_stats_api_setup_refresh(
                        state,
                        config_edit.rocket_league_path.clone(),
                        false,
                    );
                }
                let auto_detect_btn = ui.button("Auto-detect");
                if auto_detect_btn.clicked() {
                    if let Some(path) = crate::state::detect_rocket_league_path() {
                        config_edit.rocket_league_path = path;
                        *changed = true;
                        request_stats_api_setup_refresh(
                            state,
                            config_edit.rocket_league_path.clone(),
                            true,
                        );
                        ui.data_mut(|d| {
                            d.insert_temp(ui.make_persistent_id("rl_path_autodetect_failed"), false)
                        });
                    } else {
                        ui.data_mut(|d| {
                            d.insert_temp(ui.make_persistent_id("rl_path_autodetect_failed"), true)
                        });
                    }
                }
            });
        });

        if ui.data(|d| {
            d.get_temp::<bool>(ui.make_persistent_id("rl_path_autodetect_failed"))
                .unwrap_or(false)
        }) {
            status_text(
                ui,
                StatusTone::Error,
                "❌ Auto-detection failed. Could not find common installation folders. Please select your Rocket League folder manually.",
            );
        }

        let expected_ini_path = crate::setup::stats_ini_path(&config_edit.rocket_league_path)
            .display()
            .to_string();
        let status = state.system.stats_api_setup_status.load();
        if status.message.is_empty() || status.ini_path != expected_ini_path {
            request_stats_api_setup_refresh(state, config_edit.rocket_league_path.clone(), false);
        }
        ui.add_space(8.0);
        ui.collapsing("Technical connection details", |ui| {
            debug_status_row(ui, "Config File", &status.ini_path);
            debug_status_row(
                ui,
                "PacketSendRate",
                &status
                    .packet_send_rate
                    .map(|rate| rate.to_string())
                    .unwrap_or_else(|| "missing".to_string()),
            );
            debug_status_row(
                ui,
                "Port",
                &status
                    .port
                    .map(|port| port.to_string())
                    .unwrap_or_else(|| "49123 default".to_string()),
            );
        });

        if status.configured {
            status_text(ui, StatusTone::Success, &status.message);
        } else if status.exists {
            status_text(ui, StatusTone::Warning, &status.message);
        } else {
            status_text(ui, StatusTone::Error, &status.message);
        }

        if is_rl_running {
            status_text(
                ui,
                StatusTone::Warning,
                "Rocket League is running. Restart the game after changing this config.",
            );
        }

        ui.add_space(8.0);
        ui.label(helper_text(
            "Choose the periodic UpdateState rate. Lower rates reduce overhead; 30 Hz is best for the smoothest teammate boost.",
        ));
        ui.horizontal_wrapped(|ui| {
            for rate in crate::setup::PACKET_SEND_RATE_OPTIONS {
                let label = match rate {
                    0 => "Turn Off",
                    5 => "5 Hz Minimal",
                    15 => "15 Hz Responsive",
                    30 => "30 Hz Smooth",
                    _ => "Custom",
                };
                let selected = status.packet_send_rate == Some(rate);
                if ui
                    .selectable_label(selected, label)
                    .on_hover_text("Writes this PacketSendRate to DefaultStatsAPI.ini.")
                    .clicked()
                {
                    let rocket_league_path = config_edit.rocket_league_path.clone();
                    apply_stats_api_setup_rate(
                        state,
                        &rocket_league_path,
                        rate,
                        &mut config_edit.stats_api_packet_send_rate,
                        changed,
                    );
                }
            }
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Enter own (Hz):");
            let mut custom_rate_str = ui.data_mut(|d| {
                d.get_temp::<String>(ui.make_persistent_id("custom_rate_input"))
                    .unwrap_or_else(|| {
                        if let Some(r) = status.packet_send_rate
                            && r != 0
                            && r != 5
                            && r != 15
                            && r != 30
                        {
                            return r.to_string();
                        }
                        "60".to_string()
                    })
            });

            let response =
                ui.add(egui::TextEdit::singleline(&mut custom_rate_str).desired_width(40.0));
            if response.changed() {
                ui.data_mut(|d| {
                    d.insert_temp(
                        ui.make_persistent_id("custom_rate_input"),
                        custom_rate_str.clone(),
                    )
                });
            }

            let is_custom_active = status
                .packet_send_rate
                .is_some_and(|r| r != 0 && r != 5 && r != 15 && r != 30);

            if ui
                .button("Apply")
                .on_hover_text("Writes your custom PacketSendRate to DefaultStatsAPI.ini.")
                .clicked()
            {
                if let Ok(rate) = custom_rate_str.trim().parse::<u16>() {
                    let rocket_league_path = config_edit.rocket_league_path.clone();
                    apply_stats_api_setup_rate(
                        state,
                        &rocket_league_path,
                        rate,
                        &mut config_edit.stats_api_packet_send_rate,
                        changed,
                    );
                } else {
                    state.system.stats_api_setup_result.store(Arc::new(
                        crate::setup::StatsApiSetupResult {
                            message: "Please enter a valid number between 0 and 120.".to_string(),
                            ..Default::default()
                        },
                    ));
                }
            }

            if is_custom_active {
                ui.label(egui::RichText::new("Active").color(egui::Color32::from_rgb(34, 197, 94)));
            }
        });

        let result = state.system.stats_api_setup_result.load();
        if !result.message.is_empty() {
            ui.add_space(6.0);
            let tone = if result.changed {
                StatusTone::Success
            } else {
                StatusTone::Neutral
            };
            status_text(ui, tone, &result.message);
            if let Some(path) = &result.backup_path {
                debug_status_row(ui, "Backup", path);
            }
            if result.restart_required {
                status_text(
                    ui,
                    StatusTone::Warning,
                    "Restart Rocket League once before expecting the overlay to connect.",
                );
            }
        }
    });

    ui.add_space(10.0);
    render_hotkey_settings_section(ui, ctx, state, config_edit, changed);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReadinessState {
    Complete,
    ActionNeeded,
    Waiting,
}

struct ReadinessItem {
    label: &'static str,
    detail: String,
    state: ReadinessState,
}

fn setup_readiness_items(
    setup: &crate::setup::StatsApiSetupStatus,
    result: &crate::setup::StatsApiSetupResult,
    is_rl_running: bool,
    is_connected: bool,
    last_event: &str,
    last_event_unix_ms: u128,
    now_unix_ms: u128,
) -> Vec<ReadinessItem> {
    let live_packets = is_connected
        && last_event_unix_ms > 0
        && now_unix_ms.saturating_sub(last_event_unix_ms) <= 10_000;

    vec![
        ReadinessItem {
            label: "Rocket League installation",
            detail: if setup.installation_found {
                "Installation folder found.".to_string()
            } else {
                "Select or auto-detect the Rocket League folder below.".to_string()
            },
            state: if setup.installation_found {
                ReadinessState::Complete
            } else {
                ReadinessState::ActionNeeded
            },
        },
        ReadinessItem {
            label: "Stats API enabled",
            detail: if setup.configured {
                format!(
                    "Enabled at {} Hz.",
                    setup.packet_send_rate.unwrap_or_default()
                )
            } else if setup.installation_found {
                "Choose a rate of 5 Hz or higher above.".to_string()
            } else {
                "Waiting for a valid installation folder.".to_string()
            },
            state: if setup.configured {
                ReadinessState::Complete
            } else if setup.installation_found {
                ReadinessState::ActionNeeded
            } else {
                ReadinessState::Waiting
            },
        },
        ReadinessItem {
            label: "Rocket League started with current settings",
            detail: if result.restart_required && is_rl_running {
                "Restart Rocket League to load the updated Stats API setting.".to_string()
            } else if result.restart_required {
                "Start Rocket League to load the updated Stats API setting.".to_string()
            } else if is_rl_running {
                "Rocket League is running.".to_string()
            } else {
                "Start Rocket League when configuration is complete.".to_string()
            },
            state: if result.restart_required {
                ReadinessState::ActionNeeded
            } else if is_rl_running {
                ReadinessState::Complete
            } else {
                ReadinessState::Waiting
            },
        },
        ReadinessItem {
            label: "Game connection",
            detail: if is_connected {
                "Connected to Rocket League.".to_string()
            } else if is_rl_running {
                "Waiting for Rocket League to open the Stats API connection.".to_string()
            } else {
                "Waiting for Rocket League to start.".to_string()
            },
            state: if is_connected {
                ReadinessState::Complete
            } else {
                ReadinessState::Waiting
            },
        },
        ReadinessItem {
            label: "Live game data",
            detail: if live_packets {
                format!("Receiving Stats API events ({last_event}).")
            } else if is_connected {
                "Connected; waiting for live Stats API events.".to_string()
            } else {
                "Waiting for the game connection.".to_string()
            },
            state: if live_packets {
                ReadinessState::Complete
            } else {
                ReadinessState::Waiting
            },
        },
    ]
}

fn render_setup_readiness(ui: &mut egui::Ui, state: &Arc<AppState>, is_rl_running: bool) {
    let setup = state.system.stats_api_setup_status.load();
    let result = state.system.stats_api_setup_result.load();
    let diagnostics = state.system.network_diagnostics.load();
    let items = setup_readiness_items(
        &setup,
        &result,
        is_rl_running,
        state.flags.is_connected.load(Ordering::SeqCst),
        &diagnostics.last_event,
        diagnostics.last_event_unix_ms,
        crate::stats_api::now_ms(),
    );
    let completed = items
        .iter()
        .filter(|item| item.state == ReadinessState::Complete)
        .count();

    settings_section(ui, "Setup Readiness", |ui| {
        let ready = completed == items.len();
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(if ready {
                    "Ready for live matches"
                } else {
                    "Finish setup and verify the connection"
                })
                .strong()
                .color(status_color(if ready {
                    StatusTone::Success
                } else {
                    StatusTone::Warning
                })),
            );
            ui.weak(format!("{completed}/{} checks complete", items.len()));
        });
        if let Some(item) = items
            .iter()
            .find(|item| item.state != ReadinessState::Complete)
        {
            let (icon, tone) = match item.state {
                ReadinessState::Complete => ("✓", StatusTone::Success),
                ReadinessState::ActionNeeded => ("!", StatusTone::Warning),
                ReadinessState::Waiting => ("○", StatusTone::Neutral),
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(icon).strong().color(status_color(tone)));
                ui.label(egui::RichText::new(item.label).strong());
                ui.label(helper_text(&item.detail));
            });
        }
        ui.collapsing(format!("All checks ({completed}/{})", items.len()), |ui| {
            for item in &items {
                let (icon, tone) = match item.state {
                    ReadinessState::Complete => ("✓", StatusTone::Success),
                    ReadinessState::ActionNeeded => ("!", StatusTone::Warning),
                    ReadinessState::Waiting => ("○", StatusTone::Neutral),
                };
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(icon).strong().color(status_color(tone)));
                    ui.label(egui::RichText::new(item.label).strong());
                    ui.label(helper_text(&item.detail));
                });
            }
        });
    });
}

fn apply_stats_api_setup_rate(
    state: &Arc<AppState>,
    rocket_league_path: &str,
    rate: u16,
    saved_rate: &mut u16,
    changed: &mut bool,
) {
    match crate::setup::ensure_stats_api_setup_with_rate(rocket_league_path, rate) {
        Ok(result) => {
            if *saved_rate != rate {
                *saved_rate = rate;
                *changed = true;
            }
            state.system.stats_api_setup_result.store(Arc::new(result));
            request_stats_api_setup_refresh(state, rocket_league_path.to_string(), true);
        }
        Err(error) => {
            state
                .system
                .stats_api_setup_result
                .store(Arc::new(crate::setup::StatsApiSetupResult {
                    message: error,
                    ..Default::default()
                }))
        }
    }
}

pub(crate) fn request_stats_api_setup_refresh(
    state: &Arc<AppState>,
    rocket_league_path: String,
    force: bool,
) -> bool {
    if !force
        && state
            .system
            .stats_api_setup_refresh_running
            .load(Ordering::SeqCst)
    {
        return false;
    }
    if state
        .system
        .stats_api_setup_refresh_running
        .swap(true, Ordering::SeqCst)
    {
        return false;
    }

    let current = state.system.stats_api_setup_status.load();
    state
        .system
        .stats_api_setup_status
        .store(Arc::new(crate::setup::StatsApiSetupStatus {
            message: if current.message.is_empty() {
                "Checking Stats API config...".to_string()
            } else {
                "Refreshing Stats API config...".to_string()
            },
            ..(**current).clone()
        }));

    let state_clone = state.clone();
    let run = move || {
        let status = crate::setup::inspect_stats_api_setup(&rocket_league_path);
        state_clone
            .system
            .stats_api_setup_status
            .store(Arc::new(status));
        state_clone
            .system
            .stats_api_setup_refresh_running
            .store(false, Ordering::SeqCst);
    };
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn_blocking(run);
    } else {
        std::thread::spawn(run);
    }
    true
}

pub(super) fn render_support_diagnostics_section(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config_edit: &mut Config,
    changed: &mut bool,
    is_rl_running: bool,
    rl_process_detection_detail: &str,
) {
    settings_section(ui, "Support Diagnostics", |ui| {
        if ui
            .checkbox(
                &mut config_edit.debug_logging_enabled,
                "Enable debug logging",
            )
            .on_hover_text(
                "Records extra hotkey and overlay state events to the local diagnostics log.",
            )
            .changed()
        {
            *changed = true;
            state
                .debug_logging_enabled
                .store(config_edit.debug_logging_enabled, Ordering::SeqCst);
            crate::input::append_hotkey_debug_log(
                config_edit.debug_logging_enabled,
                format!(
                    "debug_logging_enabled value={}",
                    config_edit.debug_logging_enabled
                ),
            );
        }

        ui.add_space(6.0);
        let identifiable_id = ui.make_persistent_id("support_diagnostics_identifiable");
        let mut include_identifiable = ui
            .data(|data| data.get_temp::<bool>(identifiable_id))
            .unwrap_or(false);
        if ui
            .checkbox(
                &mut include_identifiable,
                "Include identifiable details",
            )
            .on_hover_text(
                "Includes local paths, player and account names, match and replay identifiers, filenames, and recent debug logs.",
            )
            .changed()
        {
            ui.data_mut(|data| data.insert_temp(identifiable_id, include_identifiable));
        }

        let privacy = if include_identifiable {
            crate::diagnostics::SupportBundlePrivacy::Identifiable
        } else {
            crate::diagnostics::SupportBundlePrivacy::Redacted
        };
        if include_identifiable {
            status_text(
                ui,
                StatusTone::Warning,
                "Review the preview carefully. This version can identify accounts and local files.",
            );
        } else {
            ui.label(helper_text(
                "Paths, names, account IDs, match/replay IDs, filenames, and recent logs are redacted by default.",
            ));
        }

        ui.add_space(6.0);
        let preview_id = ui.make_persistent_id("support_diagnostics_preview_cache");
        let refresh_requested = ui
            .horizontal(|ui| {
                ui.label(egui::RichText::new("Preview — exact clipboard contents").strong());
                ui.button("Refresh Preview").clicked()
            })
            .inner;
        ui.label(helper_text(
            "Refresh after reproducing an issue to include the latest diagnostic state.",
        ));
        let cached_preview = ui.data(|data| data.get_temp::<SupportDiagnosticsPreview>(preview_id));
        let preview =
            if support_preview_needs_refresh(cached_preview.as_ref(), privacy, refresh_requested) {
                let preview = SupportDiagnosticsPreview {
                    privacy,
                    bundle: Arc::from(build_support_diagnostics_bundle(
                        state,
                        is_rl_running,
                        rl_process_detection_detail,
                        privacy,
                    )),
                };
                ui.data_mut(|data| data.insert_temp(preview_id, preview.clone()));
                preview
            } else {
                cached_preview.expect("a current support preview must exist")
            };
        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("support_diagnostics_preview")
                .max_height(160.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(preview.bundle.as_ref())
                                .monospace()
                                .size(12.0)
                                .color(egui::Color32::from_gray(190)),
                        )
                        .selectable(true),
                    );
                });
        });

        ui.add_space(6.0);
        let copy_label = if include_identifiable {
            "Copy Identifiable Diagnostics"
        } else {
            "Copy Redacted Diagnostics"
        };
        if ui
            .add_sized([210.0, 24.0], egui::Button::new(copy_label))
            .clicked()
        {
            ui.ctx().copy_text(preview.bundle.to_string());
            ui.data_mut(|data| {
                data.insert_temp(ui.make_persistent_id("support_diagnostics_copied"), privacy)
            });
            crate::input::append_hotkey_debug_log(
                state.debug_logging_enabled.load(Ordering::SeqCst),
                format!("support_diagnostics_copied privacy={}", privacy.label()),
            );
        }

        ui.add_space(4.0);
        ui.label(helper_text(
            "Use this when reporting connection, hotkey, player parsing, or session detection issues.",
        ));

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Report a Game API Issue").strong());
        ui.label(helper_text(
            "If the detected game mode, teams, or match state looks wrong, save the recent API log and attach the file to your report.",
        ));
        status_text(
            ui,
            StatusTone::Warning,
            "The API log is identifiable and can contain player names, account IDs, and match IDs.",
        );

        let export_status = state.diagnostics.api_log_export_status.load();
        if ui
            .add_enabled(
                !export_status.running,
                egui::Button::new(if export_status.running {
                    "Saving Recent Game API Log..."
                } else {
                    "Save Recent Game API Log"
                }),
            )
            .clicked()
        {
            start_recent_api_log_export(state.clone());
        }
        if !export_status.message.is_empty() {
            status_text(ui, StatusTone::Success, &export_status.message);
        }
        if !export_status.error.is_empty() {
            status_text(ui, StatusTone::Error, &export_status.error);
        }
        if !export_status.last_output_path.is_empty() {
            debug_status_row(ui, "API Log File", &export_status.last_output_path);
            ui.label(helper_text("Attach this file to the issue report."));
        }

        ui.add_space(6.0);
        debug_status_row(
            ui,
            "Hotkey Log",
            &crate::input::hotkey_debug_log_path().display().to_string(),
        );
        if ui.button("Copy Log Path").clicked() {
            ui.ctx()
                .copy_text(crate::input::hotkey_debug_log_path().display().to_string());
            ui.data_mut(|data| data.insert_temp(ui.id().with("log_path_copied"), true));
        }
        if ui.data(|data| {
            data.get_temp::<bool>(ui.id().with("log_path_copied"))
                .unwrap_or(false)
        }) {
            ui.label("Log path copied.");
        }

        if let Some(copied_privacy) = ui.data(|data| {
            data.get_temp::<crate::diagnostics::SupportBundlePrivacy>(
                ui.make_persistent_id("support_diagnostics_copied"),
            )
        }) {
            status_text(
                ui,
                StatusTone::Success,
                format!(
                    "{} diagnostics copied to clipboard.",
                    copied_privacy.label()
                ),
            );
        }
    });
}

#[derive(Clone)]
struct SupportDiagnosticsPreview {
    privacy: crate::diagnostics::SupportBundlePrivacy,
    bundle: Arc<str>,
}

fn support_preview_needs_refresh(
    preview: Option<&SupportDiagnosticsPreview>,
    privacy: crate::diagnostics::SupportBundlePrivacy,
    refresh_requested: bool,
) -> bool {
    refresh_requested || preview.is_none_or(|preview| preview.privacy != privacy)
}

fn build_support_diagnostics_bundle(
    state: &AppState,
    is_rl_running: bool,
    rl_process_detection_detail: &str,
    privacy: crate::diagnostics::SupportBundlePrivacy,
) -> String {
    let is_launched = state.flags.is_launched.load(Ordering::SeqCst);
    if privacy == crate::diagnostics::SupportBundlePrivacy::Identifiable {
        crate::diagnostics::support_diagnostics_bundle_with_privacy(
            state,
            is_launched,
            is_rl_running,
            rl_process_detection_detail,
            privacy,
        )
    } else {
        crate::diagnostics::support_diagnostics_bundle(
            state,
            is_launched,
            is_rl_running,
            rl_process_detection_detail,
        )
    }
}

fn start_recent_api_log_export(state: Arc<AppState>) {
    let snapshot = match state.diagnostics.recent_stats_api_log.lock() {
        Ok(mut recent_log) => recent_log.snapshot(crate::stats_api::now_ms()),
        Err(_) => {
            state
                .diagnostics
                .api_log_export_status
                .store(Arc::new(ApiLogExportStatus {
                    error: "Could not read the recent API log.".to_string(),
                    ..Default::default()
                }));
            return;
        }
    };
    if snapshot.is_empty() {
        state
            .diagnostics
            .api_log_export_status
            .store(Arc::new(ApiLogExportStatus {
                error: "No game API events are available yet. Keep Rocket League open and try again after the issue occurs.".to_string(),
                ..Default::default()
            }));
        return;
    }
    let session = state.game.session.load();
    let snapshot = snapshot.with_detected_mode(
        session.active_mode.label(),
        session.active_mode_source.label(),
    );

    let output = crate::stats_api::default_recent_log_path(&state.paths.config_dir);
    state
        .diagnostics
        .api_log_export_status
        .store(Arc::new(ApiLogExportStatus {
            running: true,
            last_output_path: output.display().to_string(),
            ..Default::default()
        }));

    std::thread::spawn(move || {
        let event_count = snapshot.len();
        let result = crate::stats_api::save_recent_stats_api_snapshot(&snapshot, &output);
        let status = match result {
            Ok(()) => ApiLogExportStatus {
                last_output_path: output.display().to_string(),
                message: format!("Saved {event_count} recent API events."),
                ..Default::default()
            },
            Err(error) => ApiLogExportStatus {
                last_output_path: output.display().to_string(),
                error: format!("Could not save the API log: {error}"),
                ..Default::default()
            },
        };
        state
            .diagnostics
            .api_log_export_status
            .store(Arc::new(status));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn support_preview_refreshes_only_when_requested_or_privacy_changes() {
        let preview = SupportDiagnosticsPreview {
            privacy: crate::diagnostics::SupportBundlePrivacy::Redacted,
            bundle: Arc::from("cached"),
        };

        assert!(support_preview_needs_refresh(
            None,
            crate::diagnostics::SupportBundlePrivacy::Redacted,
            false
        ));
        assert!(!support_preview_needs_refresh(
            Some(&preview),
            crate::diagnostics::SupportBundlePrivacy::Redacted,
            false
        ));
        assert!(support_preview_needs_refresh(
            Some(&preview),
            crate::diagnostics::SupportBundlePrivacy::Identifiable,
            false
        ));
        assert!(support_preview_needs_refresh(
            Some(&preview),
            crate::diagnostics::SupportBundlePrivacy::Redacted,
            true
        ));
    }

    #[test]
    fn setup_refresh_request_dedupes_while_running() {
        let state = AppState::new();
        state
            .system
            .stats_api_setup_refresh_running
            .store(true, Ordering::SeqCst);

        assert!(!request_stats_api_setup_refresh(
            &state,
            "/tmp/rocket-league".to_string(),
            false
        ));
    }

    #[test]
    fn readiness_requires_recent_packets_for_live_data() {
        let setup = crate::setup::StatsApiSetupStatus {
            installation_found: true,
            configured: true,
            packet_send_rate: Some(30),
            ..Default::default()
        };
        let result = crate::setup::StatsApiSetupResult::default();

        let recent =
            setup_readiness_items(&setup, &result, true, true, "UpdateState", 95_000, 100_000);
        let stale =
            setup_readiness_items(&setup, &result, true, true, "UpdateState", 80_000, 100_000);

        assert_eq!(recent[4].state, ReadinessState::Complete);
        assert_eq!(stale[4].state, ReadinessState::Waiting);
    }

    #[test]
    fn readiness_surfaces_restart_as_an_action() {
        let setup = crate::setup::StatsApiSetupStatus {
            installation_found: true,
            configured: true,
            packet_send_rate: Some(15),
            ..Default::default()
        };
        let result = crate::setup::StatsApiSetupResult {
            restart_required: true,
            ..Default::default()
        };

        let items = setup_readiness_items(&setup, &result, true, false, "", 0, 100_000);

        assert_eq!(items[2].state, ReadinessState::ActionNeeded);
        assert!(items[2].detail.contains("Restart Rocket League"));
    }
}
