use crate::state::{AppState, Config};
use crate::ui::common::{StatusTone, helper_text, setting_row, settings_section, status_text};
use eframe::egui;
use std::sync::Arc;

pub(crate) fn render_replays_settings_tab(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config_edit: &mut Config,
    changed: &mut bool,
    confirm_modal: &mut Option<crate::ui::app::ConfirmAction>,
) {
    settings_section(ui, "Ballchasing.com Replay Uploader", |ui| {
        if ui
            .checkbox(&mut config_edit.ballchasing_enabled, "Enable Auto-Upload")
            .changed()
        {
            *changed = true;
        }

        ui.add_space(6.0);

        // API Key Section
        setting_row(ui, "API Key", |ui| {
            let show_key_id = ui.make_persistent_id("show_bc_api_key");
            let mut show_key = ui.data(|d| d.get_temp::<bool>(show_key_id).unwrap_or(false));

            let input_width = (ui.available_width() - 58.0).max(160.0);
            let response = if show_key {
                ui.add_sized(
                    [input_width, 22.0],
                    egui::TextEdit::singleline(&mut config_edit.ballchasing_api_key),
                )
            } else {
                ui.add_sized(
                    [input_width, 22.0],
                    egui::TextEdit::singleline(&mut config_edit.ballchasing_api_key).password(true),
                )
            };

            if response.changed() {
                *changed = true;
            }

            if ui.checkbox(&mut show_key, "Show").changed() {
                ui.data_mut(|d| d.insert_temp(show_key_id, show_key));
            }
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(helper_text("Get your API key at:"));
            ui.hyperlink_to("ballchasing.com/upload", "https://ballchasing.com/upload");
        });

        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label(helper_text(
                "Free tier quotas: 20 uploads/day, 70/week. To get higher limits, support them on:",
            ));
            ui.hyperlink_to("Patreon", "https://www.patreon.com/ballchasing");
        });

        ui.add_space(8.0);

        // Verify key button
        let verify_status_id = ui.make_persistent_id("bc_verify_status");
        let verify_status = ui.data(|d| {
            d.get_temp::<String>(verify_status_id)
                .unwrap_or_else(|| "".to_string())
        });

        ui.horizontal(|ui| {
            if ui.button("Verify Token").clicked() {
                let api_key = config_edit.ballchasing_api_key.trim().to_string();
                let ui_ctx = ui.ctx().clone();

                ui.data_mut(|d| d.insert_temp(verify_status_id, "Checking...".to_string()));

                let client = state.system.http_client.clone();
                tokio::spawn(async move {
                    let result = crate::replays::verify_token(&client, &api_key).await;
                    let msg = match result {
                        Ok(()) => "✔ Token Valid".to_string(),
                        Err(e) => format!("❌ Invalid: {}", e),
                    };
                    ui_ctx.data_mut(|d| d.insert_temp(verify_status_id, msg));
                });
            }

            if !verify_status.is_empty() {
                let color = if verify_status.starts_with("✔") {
                    egui::Color32::from_rgb(100, 220, 100)
                } else if verify_status.starts_with("Checking") {
                    egui::Color32::from_gray(160)
                } else {
                    egui::Color32::from_rgb(230, 80, 80)
                };
                ui.colored_label(color, &verify_status);
            }
        });

        ui.add_space(10.0);

        // Visibility Preference
        setting_row(ui, "Replay Visibility", |ui| {
            egui::ComboBox::new("bc_visibility", "")
                .selected_text(match config_edit.ballchasing_visibility.as_str() {
                    "public" => "Public",
                    "unlisted" => "Unlisted",
                    "private" => "Private",
                    _ => "Public",
                })
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_value(
                            &mut config_edit.ballchasing_visibility,
                            "public".to_string(),
                            "Public",
                        )
                        .clicked()
                    {
                        *changed = true;
                    }
                    if ui
                        .selectable_value(
                            &mut config_edit.ballchasing_visibility,
                            "unlisted".to_string(),
                            "Unlisted",
                        )
                        .clicked()
                    {
                        *changed = true;
                    }
                    if ui
                        .selectable_value(
                            &mut config_edit.ballchasing_visibility,
                            "private".to_string(),
                            "Private",
                        )
                        .clicked()
                    {
                        *changed = true;
                    }
                });
        });

        // Replays Directory
        setting_row(ui, "Replay Folder", |ui| {
            ui.horizontal(|ui| {
                let input_width = (ui.available_width() - 96.0).max(160.0);
                if ui
                    .add_sized(
                        [input_width, 22.0],
                        egui::TextEdit::singleline(&mut config_edit.replays_folder),
                    )
                    .changed()
                {
                    *changed = true;
                }
                let auto_detect_btn = ui.button("Auto-detect");
                if auto_detect_btn.clicked() {
                    if let Some(detected) = crate::state::detect_replays_path() {
                        config_edit.replays_folder = detected;
                        *changed = true;
                        ui.data_mut(|d| {
                            d.insert_temp(
                                ui.make_persistent_id("replay_path_autodetect_failed"),
                                false,
                            )
                        });
                    } else {
                        ui.data_mut(|d| {
                            d.insert_temp(
                                ui.make_persistent_id("replay_path_autodetect_failed"),
                                true,
                            )
                        });
                    }
                }
            });
        });

        // Folder Path Validation
        let path_valid = if config_edit.replays_folder.trim().is_empty() {
            None
        } else {
            let path = std::path::Path::new(&config_edit.replays_folder);
            Some(path.exists() && path.is_dir())
        };

        match path_valid {
            Some(true) => {
                status_text(ui, StatusTone::Success, "✔ Valid replay directory.");
            }
            Some(false) => {
                status_text(ui, StatusTone::Error, "❌ Directory not found.");
            }
            None => {
                status_text(
                    ui,
                    StatusTone::Warning,
                    "⚠ Path unconfigured. Click Auto-detect.",
                );
            }
        }

        if ui.data(|d| {
            d.get_temp::<bool>(ui.make_persistent_id("replay_path_autodetect_failed"))
                .unwrap_or(false)
        }) {
            status_text(
                ui,
                StatusTone::Error,
                "❌ Auto-detection failed. Could not locate Rocket League replays folder. Please specify it manually.",
            );
        }

        ui.add_space(6.0);
        status_text(
            ui,
            StatusTone::Warning,
            "⚠ Note: Bulk uploading waits 30s between files to respect Ballchasing.com limits.",
        );
        ui.add_space(6.0);

        // Sync and Upload buttons
        ui.horizontal(|ui| {
            let api_key_empty = config_edit.ballchasing_api_key.trim().is_empty();
            let path_invalid = path_valid != Some(true);
            let progress = state.replays.upload_progress.load();
            let bulk_running = progress.running;
            let bulk_paused = progress.paused;

            // Upload Existing
            let upload_btn = ui.add_enabled(
                !api_key_empty && !path_invalid && !bulk_running,
                egui::Button::new("Upload Existing Replays"),
            );
            if upload_btn.clicked() {
                crate::replays::start_bulk_upload_task(state.clone());
            }

            if bulk_running {
                let pause_label = if bulk_paused { "Resume" } else { "Pause" };
                if ui.button(pause_label).clicked() {
                    crate::replays::set_bulk_upload_paused(state, !bulk_paused);
                }
                if ui.button("Stop").clicked() {
                    crate::replays::stop_bulk_upload(state);
                }
            }

            // Sync Cache
            let sync_btn = ui.add_enabled(
                !api_key_empty && !bulk_running,
                egui::Button::new("Sync Uploaded Cache"),
            );
            if sync_btn.clicked() {
                crate::replays::start_sync_replays_task(state.clone());
            }

            // Clear Cache
            let clear_btn = ui.button("Clear Upload Cache");
            if clear_btn.clicked() {
                *confirm_modal = Some(crate::ui::app::ConfirmAction::ClearUploadCache);
            }
        });

        ui.add_space(8.0);

        render_upload_progress(ui, state);

        // Display Cloud Count & Local Cache
        let cloud_count = state
            .replays
            .ballchasing_cloud_count
            .load(std::sync::atomic::Ordering::SeqCst);
        if cloud_count > 0 {
            ui.label(format!("Replays on Ballchasing.com: {}", cloud_count));
            ui.add_space(4.0);
        }

        let cached_count = config_edit.uploaded_replays.len();
        if cached_count > 0 {
            egui::CollapsingHeader::new(format!("Locally Cached Uploads ({} files)", cached_count))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(100.0)
                        .show(ui, |ui| {
                            let mut sorted_replays: Vec<&String> =
                                config_edit.uploaded_replays.iter().collect();
                            sorted_replays.sort();
                            for filename in sorted_replays {
                                ui.label(
                                    egui::RichText::new(filename)
                                        .font(egui::FontId::monospace(9.0))
                                        .color(egui::Color32::from_gray(160)),
                                );
                            }
                        });
                });
            ui.add_space(4.0);
        }

        ui.separator();
        ui.add_space(6.0);

        // Status Indicator
        let current_status = if let Ok(status) = state.replays.ballchasing_status.lock() {
            status.clone()
        } else {
            "Idle".to_string()
        };

        setting_row(ui, "Uploader Status", |ui| {
            let tone = if current_status.starts_with("Success") {
                StatusTone::Success
            } else if current_status.starts_with("Error") {
                StatusTone::Error
            } else if current_status.contains("Uploading") || current_status.contains("Checking") {
                StatusTone::Warning
            } else {
                StatusTone::Neutral
            };
            status_text(ui, tone, &current_status);
        });
    });

    ui.add_space(10.0);

    settings_section(ui, "Hoops Replay Fixer", |ui| {
        ui.label("Fixes legacy/broken Rocket League Hoops replays in your folder by patching old mutator, stadium, and goal volume tags. Backups (.replay.bak) are automatically saved before patching.");

        ui.add_space(8.0);

        // Path validation feedback
        let folder_str = config_edit.replays_folder.trim();
        let path_valid = if folder_str.is_empty() {
            None
        } else {
            let path = std::path::Path::new(folder_str);
            Some(path.exists() && path.is_dir())
        };

        ui.horizontal(|ui| {
            let scan_btn = ui.add_enabled(
                path_valid == Some(true),
                egui::Button::new("Scan & Fix Replays Folder"),
            );
            if scan_btn.clicked() {
                crate::hoops_fixer::start_folder_fix_task(state.clone());
            }

            let restore_btn = ui.add_enabled(
                path_valid == Some(true),
                egui::Button::new("Restore Backups"),
            );
            if restore_btn.clicked() {
                crate::hoops_fixer::start_restore_backups_task(state.clone());
            }

            let delete_btn = ui.add_enabled(
                path_valid == Some(true),
                egui::Button::new("Delete Backups"),
            );
            if delete_btn.clicked() {
                *confirm_modal = Some(crate::ui::app::ConfirmAction::DeleteBackups);
            }
        });

        // Status Indicator
        let fixer_status = if let Ok(status) = state.hoops_fixer.hoops_fixer_status.lock() {
            status.clone()
        } else {
            "Idle".to_string()
        };

        ui.add_space(6.0);
        setting_row(ui, "Fixer Status", |ui| {
            let tone = if fixer_status.starts_with("Success") {
                StatusTone::Success
            } else if fixer_status.starts_with("Error") {
                StatusTone::Error
            } else if fixer_status.contains("Scanning") || fixer_status.contains("Checking") {
                StatusTone::Warning
            } else {
                StatusTone::Neutral
            };
            status_text(ui, tone, &fixer_status);
        });

        // Output Logs Box
        let logs = if let Ok(l) = state.hoops_fixer.hoops_fixer_logs.lock() {
            l.clone()
        } else {
            Vec::new()
        };

        if !logs.is_empty() {
            ui.add_space(8.0);
            ui.label("Fixer Logs:");
            egui::ScrollArea::vertical()
                .max_height(120.0)
                .show(ui, |ui| {
                    for log_line in &logs {
                        ui.label(
                            egui::RichText::new(log_line)
                                .font(egui::FontId::monospace(10.0))
                                .color(if log_line.starts_with("✔") {
                                    egui::Color32::from_rgb(120, 220, 120)
                                } else if log_line.contains("❌") {
                                    egui::Color32::from_rgb(220, 120, 120)
                                } else {
                                    egui::Color32::from_gray(170)
                                }),
                        );
                    }
                });
        }
    });
}

fn render_upload_progress(ui: &mut egui::Ui, state: &Arc<AppState>) {
    let progress = state.replays.upload_progress.load();
    if !progress.running && progress.total == 0 && progress.recent_events.is_empty() {
        return;
    }

    ui.add_space(4.0);
    let fraction = if progress.total == 0 {
        0.0
    } else {
        (progress.processed as f32 / progress.total as f32).clamp(0.0, 1.0)
    };
    ui.add(egui::ProgressBar::new(fraction).text(format!(
        "{}/{} processed | {} uploaded | {} skipped | {} failed",
        progress.processed, progress.total, progress.uploaded, progress.skipped, progress.failed
    )));

    if progress.running {
        if progress.paused {
            status_text(ui, StatusTone::Warning, "Paused");
        } else if !progress.current_file.is_empty() {
            ui.label(helper_text(format!(
                "Current file: {}",
                progress.current_file
            )));
        }
    }

    if !progress.last_error.is_empty() {
        status_text(
            ui,
            StatusTone::Error,
            format!("Last error: {}", progress.last_error),
        );
    }

    if !progress.recent_events.is_empty() {
        egui::CollapsingHeader::new("Recent Upload Events")
            .default_open(progress.running)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(96.0)
                    .show(ui, |ui| {
                        for event in progress.recent_events.iter().rev() {
                            ui.label(
                                egui::RichText::new(event)
                                    .font(egui::FontId::monospace(10.0))
                                    .color(
                                        if event.starts_with("Failed")
                                            || event.starts_with("Stopped")
                                        {
                                            egui::Color32::from_rgb(230, 120, 100)
                                        } else if event.starts_with("Skipped") {
                                            egui::Color32::from_rgb(225, 190, 90)
                                        } else {
                                            egui::Color32::from_gray(180)
                                        },
                                    ),
                            );
                        }
                    });
            });
    }

    ui.add_space(4.0);
}
