use super::library_table::{self, Column, cell_text};
use crate::state::{AppState, Config};
use crate::ui::common::{StatusTone, helper_text, setting_row, settings_section, status_text};
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::Ordering;

#[derive(Clone, Default)]
struct TokenVerification {
    fingerprint: u64,
    revision: u64,
    result: VerificationResult,
}

#[derive(Clone, Default)]
enum VerificationResult {
    #[default]
    Unverified,
    Checking,
    Valid,
    Failed(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ReplaysView {
    #[default]
    Uploader,
    Library,
    Tools,
}

impl TokenVerification {
    fn update_input(&mut self, key: &str) {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        let fingerprint = hasher.finish();
        if self.fingerprint != fingerprint {
            self.fingerprint = fingerprint;
            self.revision = self.revision.wrapping_add(1);
            self.result = VerificationResult::Unverified;
        }
    }

    fn complete(&mut self, revision: u64, result: Result<(), String>) {
        if self.revision == revision {
            self.result = match result {
                Ok(()) => VerificationResult::Valid,
                Err(error) => VerificationResult::Failed(error),
            };
        }
    }
}

pub(crate) fn render_replays_settings_tab(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config_edit: &mut Config,
    changed: &mut bool,
    confirm_modal: &mut Option<crate::ui::app::ConfirmAction>,
) {
    crate::replays::maybe_start_initial_replay_cache_sync(state);
    let view_id = ui.make_persistent_id("replays_view");
    let mut view = ui
        .data(|data| data.get_temp::<ReplaysView>(view_id))
        .unwrap_or_default();
    ui.horizontal(|ui| {
        ui.selectable_value(&mut view, ReplaysView::Uploader, "Uploader");
        ui.selectable_value(&mut view, ReplaysView::Library, "Replay Library");
        ui.selectable_value(&mut view, ReplaysView::Tools, "Tools & Maintenance");
    });
    ui.data_mut(|data| data.insert_temp(view_id, view));
    ui.add_space(6.0);

    let replay_path_valid = !config_edit.replays_folder.trim().is_empty()
        && std::path::Path::new(&config_edit.replays_folder).is_dir();

    if view == ReplaysView::Uploader {
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
                        egui::TextEdit::singleline(&mut config_edit.ballchasing_api_key)
                            .password(true),
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
            let mut verification = ui
                .data(|d| d.get_temp::<TokenVerification>(verify_status_id))
                .unwrap_or_default();
            verification.update_input(config_edit.ballchasing_api_key.trim());
            ui.data_mut(|d| d.insert_temp(verify_status_id, verification.clone()));

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !config_edit.ballchasing_api_key.trim().is_empty()
                            && !matches!(verification.result, VerificationResult::Checking),
                        egui::Button::new("Verify Token"),
                    )
                    .clicked()
                {
                    let api_key = config_edit.ballchasing_api_key.trim().to_string();
                    let ui_ctx = ui.ctx().clone();

                    verification.revision = verification.revision.wrapping_add(1);
                    let revision = verification.revision;
                    verification.result = VerificationResult::Checking;
                    ui.data_mut(|d| d.insert_temp(verify_status_id, verification.clone()));

                    let client = state.system.ballchasing_client.clone();
                    tokio::spawn(async move {
                        let result = crate::replays::verify_token(&client, &api_key).await;
                        ui_ctx.data_mut(|d| {
                            if let Some(mut current) =
                                d.get_temp::<TokenVerification>(verify_status_id)
                            {
                                current.complete(revision, result);
                                d.insert_temp(verify_status_id, current);
                            }
                        });
                        ui_ctx.request_repaint();
                    });
                }

                match &verification.result {
                    VerificationResult::Unverified => {
                        ui.label("Not verified");
                    }
                    VerificationResult::Checking => {
                        ui.spinner();
                        ui.label("Checking…");
                    }
                    VerificationResult::Valid => {
                        status_text(ui, StatusTone::Success, "Token valid")
                    }
                    VerificationResult::Failed(error) => status_text(ui, StatusTone::Error, error),
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
                    let input_width = (ui.available_width() - 120.0).max(80.0);
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

            if path_valid == Some(true) {
                maybe_start_metadata_scan(state, &config_edit.replays_folder);
            }

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

            ui.separator();
            ui.heading("Upload and download operations");
            // Sync and Upload buttons
            ui.horizontal_wrapped(|ui| {
                let api_key_empty = config_edit.ballchasing_api_key.trim().is_empty();
                let path_invalid = path_valid != Some(true);
                let progress = state.replays.upload_progress.load();
                let bulk_running = progress.running;
                let bulk_paused = progress.paused;
                let sync_running = state.replays.sync_running.load(Ordering::SeqCst);

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
                    if ui
                        .add_enabled(
                            !progress.stop_requested,
                            egui::Button::new(if progress.stop_requested {
                                "Stopping…"
                            } else {
                                "Stop"
                            }),
                        )
                        .clicked()
                    {
                        crate::replays::stop_bulk_upload(state);
                    }
                }

                // Sync Cache
                let sync_btn = ui.add_enabled(
                    !api_key_empty && !bulk_running && !sync_running,
                    egui::Button::new(if sync_running {
                        "Syncing Uploaded Cache..."
                    } else {
                        "Sync Uploaded Cache"
                    }),
                );
                if sync_btn.clicked() {
                    crate::replays::start_sync_replays_task(state.clone());
                }
            });

            ui.add_space(8.0);

            // Download Replay by ID
            ui.horizontal(|ui| {
                let download_id_id = ui.make_persistent_id("bc_download_id");
                let mut download_id =
                    ui.data(|d| d.get_temp::<String>(download_id_id).unwrap_or_default());

                ui.label(
                    egui::RichText::new("Download by ID:")
                        .color(egui::Color32::from_rgb(225, 227, 235)),
                );
                let response = ui.add_sized(
                    [(ui.available_width() - 96.0).max(100.0), 22.0],
                    egui::TextEdit::singleline(&mut download_id)
                        .hint_text("Enter Ballchasing Replay ID..."),
                );
                if response.changed() {
                    ui.data_mut(|d| d.insert_temp(download_id_id, download_id.clone()));
                }

                let download_active = state.replays.download_active.load(Ordering::SeqCst);
                let api_key_empty = config_edit.ballchasing_api_key.trim().is_empty();
                let path_invalid = path_valid != Some(true);

                let btn = ui.add_enabled(
                    !download_active
                        && !api_key_empty
                        && !path_invalid
                        && !download_id.trim().is_empty(),
                    egui::Button::new("Download"),
                );
                if btn.clicked() {
                    let id_clean = download_id.trim().to_string();
                    crate::replays::start_download_replay_task(state.clone(), id_clean);
                }
            });

            ui.add_space(8.0);

            render_upload_progress(ui, state);

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
                } else if current_status.starts_with("Partial failure")
                    || current_status.contains("Uploading")
                    || current_status.contains("Checking")
                    || current_status.contains("Downloading")
                {
                    StatusTone::Warning
                } else {
                    StatusTone::Neutral
                };
                status_text(ui, tone, &current_status);
            });
        });
    }

    if view == ReplaysView::Library {
        ui.heading("Replay Library");
        if replay_path_valid {
            maybe_start_metadata_scan(state, &config_edit.replays_folder);
        }
        render_replay_cache(ui, state, config_edit, replay_path_valid);
    }

    if view == ReplaysView::Tools {
        settings_section(ui, "Tools & Maintenance", |ui| {
            ui.label("Clearing upload membership keeps replay files, but future scans may upload them again.");
            let can_clear = crate::replays::can_clear_upload_ledger(state);
            if ui
                .add_enabled(can_clear, egui::Button::new("Clear Upload Cache…"))
                .clicked()
            {
                *confirm_modal = Some(crate::ui::app::ConfirmAction::ClearUploadCache);
            }
            if !can_clear {
                ui.label("Wait for uploads, downloads and sync to finish. You can stop bulk uploads above.");
            }
            crate::ui::common::maintenance_status(ui, &state.replays.clear_status.load());
            ui.separator();
            ui.heading("Hoops Replay Fixer");
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

            ui.horizontal_wrapped(|ui| {
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
}

fn maybe_start_metadata_scan(state: &Arc<AppState>, folder: &str) {
    let snapshot = state.replays.metadata_cache.load();
    if snapshot.folder != folder {
        crate::replay_metadata::start_metadata_scan(state.clone(), folder.to_string());
    }
}

fn render_replay_cache(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config: &Config,
    path_valid: bool,
) {
    let uploaded = state.replays.uploaded_replays.load_full();
    let snapshot = crate::replay_metadata::merged_metadata_snapshot(state);
    let scan_running = state.replays.metadata_scan_running.load(Ordering::SeqCst);
    let all_rows = cached_replay_cache_rows(ui, state, uploaded.as_ref(), snapshot.clone(), "");
    ui.horizontal_wrapped(|ui| {
        ui.strong(format!("{} replays", all_rows.len()));
        ui.weak(format!(
            "{} local · {} cloud",
            all_rows.iter().filter(|r| r.has_local).count(),
            all_rows.iter().filter(|r| !r.cloud_ids.is_empty()).count()
        ));
        if ui
            .add_enabled(
                path_valid && !scan_running,
                egui::Button::new("Refresh Metadata"),
            )
            .on_disabled_hover_text("Choose a replay folder, or wait for the current scan.")
            .clicked()
        {
            crate::replay_metadata::start_metadata_scan(
                state.clone(),
                config.replays_folder.clone(),
            );
        }
        if ui
            .add_enabled(
                !config.ballchasing_api_key.trim().is_empty()
                    && !state.replays.sync_running.load(Ordering::SeqCst),
                egui::Button::new("Sync Cloud"),
            )
            .clicked()
        {
            crate::replays::start_sync_replays_task(state.clone());
        }
    });
    if scan_running {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.weak("Refreshing local replays…");
        });
    } else if snapshot.failed > 0 {
        status_text(
            ui,
            StatusTone::Warning,
            format!(
                "{} local replay files could not be read. Expand their rows for details.",
                snapshot.failed
            ),
        );
    }
    if let Ok(status) = state.replays.ballchasing_status.lock()
        && !status.is_empty()
    {
        ui.add(egui::Label::new(helper_text(&*status)).truncate())
            .on_hover_text(&*status);
    }
    let search_id = ui.make_persistent_id("replay_cache_search");
    let mut search = ui
        .data(|d| d.get_temp::<String>(search_id))
        .unwrap_or_default();
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_sized(
            [(ui.available_width() - 78.0).max(40.0), 28.0],
            egui::TextEdit::singleline(&mut search)
                .hint_text("Search replays, players, maps, dates…"),
        );
        if ui.button("Clear").clicked() {
            search.clear();
        }
    });
    ui.data_mut(|d| d.insert_temp(search_id, search.clone()));
    let filter_id = ui.make_persistent_id("replay_source_filter");
    let mut filter = ui.data(|d| d.get_temp::<usize>(filter_id)).unwrap_or(0);
    ui.horizontal(|ui| {
        for (value, label) in [(0, "All"), (1, "Local"), (2, "Cloud")] {
            ui.selectable_value(&mut filter, value, label);
        }
    });
    ui.data_mut(|d| d.insert_temp(filter_id, filter));
    let query = search.trim().to_lowercase();
    let mut rows: Vec<_> = all_rows
        .iter()
        .filter(|r| {
            (query.is_empty() || row_matches_query(r, &query))
                && match filter {
                    1 => r.has_local,
                    2 => !r.cloud_ids.is_empty(),
                    _ => true,
                }
        })
        .collect();
    let sort_id = ui.make_persistent_id("replay_sort");
    let mut sort = ui
        .data(|d| d.get_temp::<(usize, bool)>(sort_id))
        .unwrap_or((1, true));
    rows.sort_by(|a, b| {
        let ordering = match sort.0 {
            0 => a.primary.to_lowercase().cmp(&b.primary.to_lowercase()),
            2 => a.map.cmp(&b.map),
            3 => a.score_value.cmp(&b.score_value),
            4 => a.source_label.cmp(b.source_label),
            _ => a.date_key.cmp(&b.date_key),
        };
        (if sort.1 { ordering.reverse() } else { ordering }).then_with(|| a.key.cmp(&b.key))
    });
    ui.weak(format!("{} shown · Click a row for details", rows.len()));
    if rows.is_empty() {
        ui.label(if all_rows.is_empty() {
            "No replays found. Choose a replay folder or sync cloud uploads."
        } else {
            "No replays match these filters."
        });
        return;
    }
    let width = (ui.available_width() - 20.0).max(1.0);
    let wide = width >= 900.0;
    let mut columns = vec![
        Column {
            label: "Replay",
            sort: 0,
            width: width - if wide { 534.0 } else { 354.0 },
            numeric: false,
        },
        Column {
            label: "Date",
            sort: 1,
            width: 142.0,
            numeric: false,
        },
    ];
    if wide {
        columns.push(Column {
            label: "Map",
            sort: 2,
            width: 180.0,
            numeric: false,
        });
    }
    columns.push(Column {
        label: "Score",
        sort: 3,
        width: 62.0,
        numeric: true,
    });
    columns.push(Column {
        label: "Availability",
        sort: 4,
        width: 150.0,
        numeric: false,
    });
    let keys: Vec<_> = rows.iter().map(|r| r.key.clone()).collect();
    let download_enabled = !state.replays.download_active.load(Ordering::SeqCst)
        && !config.ballchasing_api_key.trim().is_empty()
        && path_valid;
    library_table::show(
        ui,
        "replay_table",
        &columns,
        &keys,
        &mut sort,
        36.0,
        |ui, index, column, expanded| {
            let row = rows[index];
            match columns[column].sort {
                0 => cell_text(
                    ui,
                    format!("{} {}", if expanded { "▾" } else { "▸" }, row.primary),
                ),
                1 => cell_text(ui, &row.date),
                2 => cell_text(ui, &row.map),
                3 => cell_text(ui, &row.score),
                _ => {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(row.source_label).color(row.source_color),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&row.hover);
                }
            }
        },
        |ui, index| {
            let row = rows[index];
            render_replay_details(ui, row);
            ui.collapsing("Files & identifiers", |ui| {
                for filename in &row.filenames {
                    ui.label(filename);
                }
                ui.label(&row.key);
                for id in &row.cloud_ids {
                    ui.label(format!("Ballchasing: {id}"));
                }
                ui.collapsing("Source metadata", |ui| {
                    ui.label(&row.hover);
                });
                if ui.button("Copy filenames").clicked() {
                    ui.ctx().copy_text(row.filenames.join("\n"));
                }
            });
            for cloud_id in &row.cloud_ids {
                ui.horizontal_wrapped(|ui| {
                    ui.hyperlink_to("Open on Ballchasing",format!("https://ballchasing.com/replay/{cloud_id}"));
                    if !row.has_local && ui.add_enabled(download_enabled,egui::Button::new("Download")).on_disabled_hover_text("Choose a valid replay folder and API key, and wait for any active download.").clicked() {
                        crate::replays::start_download_replay_task(state.clone(),cloud_id.clone());
                    }
                });
            }
        },
    );
    ui.data_mut(|d| d.insert_temp(sort_id, sort));
}

fn replay_team_color(team: Option<i32>) -> egui::Color32 {
    match team {
        Some(0) => egui::Color32::from_rgb(105, 180, 255),
        Some(1) => egui::Color32::from_rgb(255, 175, 90),
        _ => egui::Color32::from_gray(218),
    }
}

fn render_replay_details(ui: &mut egui::Ui, row: &ReplayCacheRow) {
    ui.strong(&row.primary);
    ui.horizontal_wrapped(|ui| {
        ui.label(&row.map);
        ui.separator();
        ui.weak(&row.date);
        ui.colored_label(row.source_color, row.source_label);
    });
    let Some(entry) = &row.metadata else {
        ui.weak(&row.hover);
        return;
    };
    ui.add_space(6.0);
    egui::Frame::NONE
        .fill(egui::Color32::from_gray(27))
        .corner_radius(5)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(replay_team_color(Some(0)), "BLUE");
                ui.label(
                    egui::RichText::new(
                        entry
                            .team0_score
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "—".into()),
                    )
                    .size(24.0)
                    .strong()
                    .color(replay_team_color(Some(0))),
                );
                ui.weak("–");
                ui.label(
                    egui::RichText::new(
                        entry
                            .team1_score
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "—".into()),
                    )
                    .size(24.0)
                    .strong()
                    .color(replay_team_color(Some(1))),
                );
                ui.colored_label(replay_team_color(Some(1)), "ORANGE");
                if let Some(seconds) = entry.duration_seconds {
                    ui.separator();
                    ui.label(replay_time_label(seconds))
                        .on_hover_text("Match duration");
                }
                if !entry.match_type.is_empty() {
                    ui.weak(&entry.match_type);
                }
            });
        });
    ui.add_space(6.0);
    if !entry.players.is_empty() {
        ui.strong("Players");
        let mut players: Vec<_> = entry.players.iter().collect();
        players.sort_by_key(|p| match p.team {
            Some(0) => 0,
            Some(1) => 1,
            _ => 2,
        });
        if ui.available_width() >= 560.0 {
            let name_width = (ui.available_width() - 354.0).max(100.0);
            egui::Grid::new("replay_player_stats")
                .striped(true)
                .min_col_width(52.0)
                .spacing([14.0, 6.0])
                .show(ui, |ui| {
                    for label in [
                        "Player / team",
                        "Points",
                        "Goals",
                        "Assists",
                        "Saves",
                        "Shots",
                    ] {
                        ui.add(egui::Label::new(egui::RichText::new(label).weak()).truncate());
                    }
                    ui.end_row();
                    for player in players {
                        ui.allocate_ui_with_layout(
                            egui::vec2(name_width, 20.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(replay_player_name(player))
                                            .color(replay_team_color(player.team)),
                                    )
                                    .truncate(),
                                );
                            },
                        )
                        .response
                        .on_hover_text(player_stats_label(player));
                        for value in [
                            player.score,
                            player.goals,
                            player.assists,
                            player.saves,
                            player.shots,
                        ] {
                            ui.label(value.map(|v| v.to_string()).unwrap_or_else(|| "—".into()));
                        }
                        ui.end_row();
                    }
                });
        } else {
            for player in players {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(replay_player_name(player))
                            .color(replay_team_color(player.team)),
                    )
                    .truncate(),
                )
                .on_hover_text(&player.name);
                ui.horizontal_wrapped(|ui| {
                    for (value, label) in [
                        (player.score, "pts"),
                        (player.goals, "goals"),
                        (player.assists, "assists"),
                        (player.saves, "saves"),
                        (player.shots, "shots"),
                    ] {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} {label}",
                                value.map(|v| v.to_string()).unwrap_or_else(|| "—".into())
                            ))
                            .size(11.0),
                        );
                    }
                });
                ui.add_space(4.0);
            }
        }
    } else if !entry.player_names.is_empty() {
        ui.strong("Players");
        ui.label(entry.player_names.join(", "));
    }
    if !entry.goals.is_empty() {
        ui.add_space(6.0);
        ui.collapsing(format!("Goals · {}", entry.goals.len()), |ui| {
            egui::ScrollArea::vertical()
                .id_salt("replay_goals")
                .max_height(180.0)
                .show_rows(ui, 22.0, entry.goals.len(), |ui, range| {
                    for goal in &entry.goals[range] {
                        ui.horizontal(|ui| {
                            let time = goal
                                .elapsed_seconds
                                .map(replay_time_label)
                                .or_else(|| goal.frame.map(|f| format!("frame {f}")))
                                .unwrap_or_else(|| "—".into());
                            ui.add_sized(
                                [72.0, 22.0],
                                egui::Label::new(egui::RichText::new(time).monospace()),
                            );
                            let scorer = if goal.player_name.trim().is_empty() {
                                "Unknown scorer"
                            } else {
                                &goal.player_name
                            };
                            let team = match goal.team {
                                Some(0) => "Blue",
                                Some(1) => "Orange",
                                _ => "Unknown team",
                            };
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!("{scorer} · {team}"))
                                        .color(replay_team_color(goal.team)),
                                )
                                .truncate(),
                            )
                            .on_hover_text(goal_label(goal));
                        });
                    }
                });
        });
    }
    ui.add_space(6.0);
    ui.weak(format!("Upload record: {}", row.upload_status));
    if !entry.error.is_empty() {
        status_text(ui, StatusTone::Error, &entry.error);
    }
}

fn replay_player_name(player: &crate::replay_metadata::ReplayPlayerMetadata) -> String {
    let team = match player.team {
        Some(0) => "Blue",
        Some(1) => "Orange",
        _ => "Unknown team",
    };
    format!(
        "{} · {team}{}",
        player.name,
        if player.is_bot == Some(true) {
            " · Bot"
        } else {
            ""
        }
    )
}

#[derive(Clone)]
struct ReplayRowsCache {
    ledger_revision: u64,
    metadata: Arc<crate::replay_metadata::ReplayMetadataSnapshot>,
    query: String,
    rows: Arc<Vec<ReplayCacheRow>>,
}

fn cached_replay_cache_rows(
    ui: &mut egui::Ui,
    state: &AppState,
    uploaded_replays: &[String],
    metadata: Arc<crate::replay_metadata::ReplayMetadataSnapshot>,
    search: &str,
) -> Arc<Vec<ReplayCacheRow>> {
    let cache_id = ui.make_persistent_id("replay_cache_rows");
    let ledger_revision = state.replays.ledger_revision.load(Ordering::SeqCst);
    let query = search.trim().to_ascii_lowercase();
    if let Some(cache) = ui.data(|data| data.get_temp::<ReplayRowsCache>(cache_id))
        && cache.ledger_revision == ledger_revision
        && Arc::ptr_eq(&cache.metadata, &metadata)
        && cache.query == query
    {
        return cache.rows;
    }

    let rows = Arc::new(replay_cache_rows(
        uploaded_replays,
        &metadata.entries,
        &query,
    ));
    ui.data_mut(|data| {
        data.insert_temp(
            cache_id,
            ReplayRowsCache {
                ledger_revision,
                metadata,
                query,
                rows: rows.clone(),
            },
        )
    });
    rows
}

#[derive(Clone, Debug, PartialEq)]
struct ReplayCacheRow {
    metadata: Option<crate::replay_metadata::ReplayMetadataEntry>,
    key: String,
    upload_status: &'static str,
    filename: String,
    filenames: Vec<String>,
    cloud_ids: Vec<String>,
    has_local: bool,
    primary: String,
    date: String,
    date_key: Option<[u32; 6]>,
    map: String,
    score: String,
    score_value: Option<(i32, i32)>,
    players: String,
    search_text: String,
    source_label: &'static str,
    source_color: egui::Color32,
    hover: String,
}

fn replay_cache_rows(
    uploaded: &[String],
    metadata: &std::collections::HashMap<String, crate::replay_metadata::ReplayMetadataEntry>,
    search: &str,
) -> Vec<ReplayCacheRow> {
    use crate::replay_metadata::{fill_missing_metadata, normalized_replay_id};
    use std::collections::{BTreeMap, HashMap};
    let mut groups: BTreeMap<String, Vec<&crate::replay_metadata::ReplayMetadataEntry>> =
        BTreeMap::new();
    // A cloud ID and the game's replay ID belong to different namespaces.
    let mut cloud_games: HashMap<String, std::collections::BTreeSet<String>> = HashMap::new();
    for entry in metadata.values() {
        if let (Some(cloud), Some(game)) = (
            normalized_replay_id(&entry.cloud_replay_id),
            normalized_replay_id(&entry.game_replay_id),
        ) {
            cloud_games.entry(cloud).or_default().insert(game);
        }
    }
    let cloud_to_game: HashMap<_, _> = cloud_games
        .into_iter()
        .filter_map(|(cloud, games)| {
            (games.len() == 1).then(|| (cloud, games.into_iter().next().unwrap()))
        })
        .collect();
    for entry in metadata.values() {
        let cloud = normalized_replay_id(&entry.cloud_replay_id);
        let game = normalized_replay_id(&entry.game_replay_id)
            .or_else(|| cloud.as_ref().and_then(|id| cloud_to_game.get(id).cloned()));
        let key = if let Some(id) = game {
            format!("game:{id}")
        } else if let Some(id) = cloud {
            format!("cloud:{id}")
        } else {
            format!("file:{}", entry.filename.to_ascii_lowercase())
        };
        groups.entry(key).or_default().push(entry);
    }
    let mut rows = Vec::new();
    let mut aliases = HashMap::new();
    for (key, mut entries) in groups {
        // Deterministic local-first selection; never overwrite file identity.
        entries.sort_by_key(|e| {
            (
                !e.has_metadata(),
                e.file_size == 0,
                e.filename.to_ascii_lowercase(),
            )
        });
        let mut combined = entries[0].clone();
        for entry in &entries[1..] {
            fill_missing_metadata(&mut combined, entry);
        }
        let mut row = replay_cache_row(&combined.filename, Some(&combined));
        row.key = key;
        row.has_local = entries
            .iter()
            .any(|e| e.file_size > 0 || e.cloud_replay_id.is_empty());
        row.filenames = entries.iter().map(|e| e.filename.clone()).collect();
        row.filenames.sort();
        row.filenames.dedup();
        row.cloud_ids = entries
            .iter()
            .filter_map(|e| normalized_replay_id(&e.cloud_replay_id))
            .map(|id| {
                format!(
                    "{}-{}-{}-{}-{}",
                    &id[..8],
                    &id[8..12],
                    &id[12..16],
                    &id[16..20],
                    &id[20..]
                )
            })
            .collect();
        row.cloud_ids.sort();
        row.cloud_ids.dedup();
        let errors: Vec<_> = entries
            .iter()
            .filter(|e| !e.error.is_empty())
            .map(|e| format!("{}: {}", e.filename, e.error))
            .collect();
        row.source_label = if !errors.is_empty() {
            "Parse error"
        } else if row.has_local && !row.cloud_ids.is_empty() {
            "Local + Cloud"
        } else if row.has_local {
            "Local"
        } else {
            "Cloud"
        };
        row.source_color = if !errors.is_empty() {
            egui::Color32::from_rgb(230, 95, 85)
        } else if row.has_local {
            egui::Color32::from_rgb(105, 210, 165)
        } else {
            egui::Color32::from_rgb(100, 180, 240)
        };
        for error in errors {
            row.hover.push_str(&format!("\n{error}"));
        }
        for entry in &entries {
            if entry.display_name != combined.display_name && !entry.display_name.is_empty() {
                row.hover
                    .push_str(&format!("\nAlternate title: {}", entry.display_name));
            }
            if entry.date != combined.date {
                row.hover
                    .push_str(&format!("\nSource date: {}", entry.date));
            }
        }
        for filename in &row.filenames {
            aliases.insert(filename.to_ascii_lowercase(), rows.len());
        }
        for id in &row.cloud_ids {
            aliases.insert(format!("{id}.replay"), rows.len());
            aliases.insert(format!("{}.replay", id.replace('-', "")), rows.len());
        }
        rows.push(row);
    }
    for filename in uploaded {
        let alias = filename.to_ascii_lowercase();
        if let Some(&index) = aliases.get(&alias) {
            rows[index].upload_status = "Recorded";
            if !rows[index]
                .filenames
                .iter()
                .any(|f| f.eq_ignore_ascii_case(filename))
            {
                rows[index].filenames.push(filename.clone());
            }
        } else {
            let mut row = replay_cache_row(filename, None);
            row.upload_status = "Recorded";
            aliases.insert(alias, rows.len());
            rows.push(row);
        }
    }
    let query = search.trim().to_lowercase();
    for row in &mut rows {
        row.search_text = format!(
            "{} {} {}",
            replay_row_search_text(row),
            row.filenames.join(" "),
            row.cloud_ids.join(" ")
        )
        .to_lowercase();
    }
    rows.retain(|row| query.is_empty() || row_matches_query(row, &query));
    rows.sort_by(|a, b| b.date_key.cmp(&a.date_key).then_with(|| a.key.cmp(&b.key)));
    rows
}

fn parsed_replay_date(value: &str) -> Option<chrono::NaiveDateTime> {
    if let Ok(date) = chrono::DateTime::parse_from_rfc3339(value.trim()) {
        return Some(date.with_timezone(&chrono::Local).naive_local());
    }
    for format in [
        "%Y-%m-%d:%H-%M-%S",
        "%Y-%m-%d:%H-%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M:%S",
    ] {
        if let Ok(date) = chrono::NaiveDateTime::parse_from_str(value.trim(), format) {
            return Some(date);
        }
    }
    chrono::NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .ok()?
        .and_hms_opt(0, 0, 0)
}

fn replay_date_key(value: &str) -> Option<[u32; 6]> {
    use chrono::{Datelike, Timelike};
    let date = parsed_replay_date(value)?;
    Some([
        u32::try_from(date.year()).ok()?,
        date.month(),
        date.day(),
        date.hour(),
        date.minute(),
        date.second(),
    ])
}

fn display_replay_date(value: &str) -> String {
    parsed_replay_date(value)
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| {
            if value.trim().is_empty() {
                "—".into()
            } else {
                value.to_owned()
            }
        })
}

fn display_arena(value: &str) -> String {
    match value {
        "Stadium_P" => "DFH Stadium",
        "EuroStadium_P" => "Mannfield",
        "EuroStadium_Rainy_P" => "Mannfield (Stormy)",
        "EuroStadium_Night_P" => "Mannfield (Night)",
        "HoopsStadium_P" => "Dunk House",
        "Underwater_P" => "AquaDome",
        "TrainStation_P" => "Urban Central",
        "Wasteland_P" => "Wasteland",
        "UtopiaStadium_P" => "Utopia Coliseum",
        _ => return display_or_dash(value),
    }
    .to_owned()
}

fn replay_cache_row(
    filename: &str,
    entry: Option<&crate::replay_metadata::ReplayMetadataEntry>,
) -> ReplayCacheRow {
    let has_local = entry.is_some_and(|e| e.file_size > 0 || e.cloud_replay_id.is_empty());
    let primary = entry
        .filter(|e| !e.display_name.trim().is_empty())
        .map(|e| e.display_name.clone())
        .unwrap_or_else(|| filename.trim_end_matches(".replay").to_owned());
    let source_label = match entry {
        Some(e) if !e.error.is_empty() => "Parse error",
        Some(_) if has_local => "Local",
        Some(_) => "Cloud",
        None => "Upload record only",
    };
    let mut row=ReplayCacheRow {
        metadata:entry.cloned(),
        key:format!("file:{}",filename.to_ascii_lowercase()),
        upload_status:"Not recorded",
        filename:filename.to_owned(),filenames:vec![filename.to_owned()],cloud_ids:vec![],has_local,
        primary,
        date:entry.map(|e| display_replay_date(&e.date)).unwrap_or_else(|| "—".into()),
        date_key:entry.and_then(|e| replay_date_key(&e.date)),
        map:entry.map(|e| display_arena(&e.map_name)).unwrap_or_else(|| "-".into()),
        score:entry.map(score_label).unwrap_or_else(|| "-".into()),
        score_value:entry.and_then(|e| Some((e.team0_score?,e.team1_score?))),
        players:entry.map(players_label).unwrap_or_else(|| "-".into()),
        source_label,source_color:egui::Color32::from_gray(175),
        hover:entry.map(|e| metadata_hover(filename,e)).unwrap_or_else(|| "No matching local replay file or cloud metadata. Upload membership does not verify local file contents.".into()),
        search_text:String::new(),
    };
    row.search_text = replay_row_search_text(&row);
    row
}

fn display_or_dash(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        "-".to_string()
    } else {
        value.to_owned()
    }
}

fn score_label(entry: &crate::replay_metadata::ReplayMetadataEntry) -> String {
    if let (Some(team0), Some(team1)) = (entry.team0_score, entry.team1_score) {
        format!("{team0}-{team1}")
    } else {
        "-".to_string()
    }
}

fn players_label(entry: &crate::replay_metadata::ReplayMetadataEntry) -> String {
    if entry.player_names.is_empty() {
        "-".to_string()
    } else if entry.player_names.len() == 1 {
        shorten_text(&entry.player_names[0], 22)
    } else {
        shorten_text(
            &format!(
                "{} + {}",
                entry.player_names[0],
                entry.player_names.len() - 1
            ),
            22,
        )
    }
}

fn metadata_hover(filename: &str, entry: &crate::replay_metadata::ReplayMetadataEntry) -> String {
    let mut lines = vec![
        format!("Original date: {}", entry.date),
        format!("File: {filename}"),
    ];
    if !entry.game_replay_id.trim().is_empty() {
        lines.push(format!("Game replay ID: {}", entry.game_replay_id));
    }
    if !entry.cloud_replay_id.trim().is_empty() {
        lines.push(format!("Cloud replay ID: {}", entry.cloud_replay_id));
    }
    if !entry.match_type.trim().is_empty() {
        lines.push(format!("Match type: {}", entry.match_type));
    }
    if let Some(seconds) = entry.duration_seconds {
        lines.push(format!("Duration: {}", replay_time_label(seconds)));
    }
    if !entry.players.is_empty() {
        lines.push("Players:".to_string());
        lines.extend(entry.players.iter().map(player_stats_label));
    } else if !entry.player_names.is_empty() {
        lines.push(format!("Players: {}", entry.player_names.join(", ")));
    }
    if !entry.goals.is_empty() {
        lines.push("Goals:".to_string());
        lines.extend(entry.goals.iter().map(goal_label));
    }
    lines.join("\n")
}

fn player_stats_label(player: &crate::replay_metadata::ReplayPlayerMetadata) -> String {
    let team = match player.team {
        Some(0) => "Blue".to_string(),
        Some(1) => "Orange".to_string(),
        Some(team) => format!("Team {team}"),
        None => "Unknown team".to_string(),
    };
    let mut stats = Vec::new();
    if let Some(score) = player.score {
        stats.push(format!("{score} pts"));
    }
    for (value, label) in [
        (player.goals, "G"),
        (player.assists, "A"),
        (player.saves, "S"),
        (player.shots, "Sh"),
    ] {
        if let Some(value) = value {
            stats.push(format!("{value} {label}"));
        }
    }
    let bot = if player.is_bot == Some(true) {
        " · Bot"
    } else {
        ""
    };
    if stats.is_empty() {
        format!("  {} · {team}{bot}", player.name)
    } else {
        format!("  {} · {team}{bot} · {}", player.name, stats.join(" · "))
    }
}

fn goal_label(goal: &crate::replay_metadata::ReplayGoalMetadata) -> String {
    let scorer = if goal.player_name.trim().is_empty() {
        "Unknown scorer"
    } else {
        goal.player_name.as_str()
    };
    let time = goal
        .elapsed_seconds
        .map(replay_time_label)
        .or_else(|| goal.frame.map(|frame| format!("frame {frame}")))
        .unwrap_or_else(|| "unknown time".to_string());
    let team = match goal.team {
        Some(0) => " · Blue",
        Some(1) => " · Orange",
        Some(_) => " · Other team",
        None => "",
    };
    format!("  {time} · {scorer}{team}")
}

fn replay_time_label(seconds: u32) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn row_matches_query(row: &ReplayCacheRow, query: &str) -> bool {
    row.search_text.contains(query)
}

fn replay_row_search_text(row: &ReplayCacheRow) -> String {
    [
        row.primary.as_str(),
        row.date.as_str(),
        row.map.as_str(),
        row.score.as_str(),
        row.players.as_str(),
        row.hover.as_str(),
        row.source_label,
    ]
    .join("\n")
    .to_ascii_lowercase()
}

fn shorten_text(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    format!("{}...", trimmed.chars().take(keep).collect::<String>())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_groups_game_identity_preserving_files_cloud_actions_and_search() {
        let game = "ECAF212F4E9154C5F5C2F681C5D891EE";
        let remote = "969c37e6-f5c7-41bf-a0c0-21922df74469";
        let remote2 = "969c37e6-f5c7-41bf-a0c0-21922df74468";
        let mut local = metadata_entry("local-name.replay", "My saved title");
        local.game_replay_id = game.into();
        let mut cloud = metadata_entry(&format!("{remote}.replay"), "Cloud title");
        cloud.file_size = 0;
        cloud.cloud_replay_id = remote.into();
        cloud.game_replay_id = "ecaf212f-4e91-54c5-f5c2-f681c5d891ee".into();
        let mut cloud2 = cloud.clone();
        cloud2.cloud_replay_id = remote2.into();
        cloud2.filename = format!("{remote2}.replay");
        let metadata = HashMap::from([
            (local.filename.clone(), local),
            (cloud.filename.clone(), cloud),
            (cloud2.filename.clone(), cloud2),
        ]);
        let uploaded = vec![format!("{}.replay", remote.replace('-', "")).to_uppercase()];
        let rows = replay_cache_rows(&uploaded, &metadata, "");
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.primary, "My saved title");
        assert_eq!(row.source_label, "Local + Cloud");
        assert_eq!(row.cloud_ids, vec![remote2.to_string(), remote.to_string()]);
        assert!(row.filenames.contains(&"local-name.replay".to_string()));
        assert_eq!(row.upload_status, "Recorded");
        assert_eq!(
            replay_cache_rows(&uploaded, &metadata, "local-name").len(),
            1
        );
        assert_eq!(replay_cache_rows(&uploaded, &metadata, remote2).len(), 1);
        assert!(
            replay_cache_rows(&[], &metadata, "")
                .iter()
                .all(|r| r.upload_status == "Not recorded")
        );
    }

    #[test]
    fn library_does_not_group_similar_titles_dates_or_invalid_ids() {
        let mut a = metadata_entry("a.replay", "Same title");
        a.game_replay_id = "invalid".into();
        let mut b = a.clone();
        b.filename = "b.replay".into();
        let metadata = HashMap::from([(a.filename.clone(), a), (b.filename.clone(), b)]);
        assert_eq!(replay_cache_rows(&[], &metadata, "").len(), 2);
        assert_eq!(
            replay_cache_rows(&[], &metadata, "nothing matches").len(),
            0
        );
    }

    #[test]
    fn replay_dates_validate_and_normalize_without_assuming_naive_timezone() {
        assert_eq!(
            display_replay_date("2026-07-12:03-42-37"),
            "2026-07-12 03:42"
        );
        assert_eq!(
            replay_date_key("2026-07-12T03:42:37+02:00"),
            replay_date_key("2026-07-12T01:42:37Z")
        );
        assert!(replay_date_key("2026-02-30 12:00").is_none());
        assert_eq!(display_replay_date("unknown"), "unknown");
    }

    #[tokio::test]
    async fn replay_library_layout() {
        let state = AppState::new();
        let mut entries = HashMap::new();
        for i in 0..80 {
            let mut entry = metadata_entry(
                &format!("match-{i:03}.replay"),
                if i == 0 {
                    "A very long replay title with Unicode 漢字 and extra details"
                } else {
                    "Ranked Doubles"
                },
            );
            entry.game_replay_id = format!("{i:032x}");
            if i == 0 {
                entry.duration_seconds = Some(416);
                entry.match_type = "Offline".into();
                entry.team0_score = Some(8);
                entry.team1_score = Some(7);
                entry.players = vec![
                    crate::replay_metadata::ReplayPlayerMetadata {
                        name: "cyberPeng with a very long name 漢字".into(),
                        team: Some(0),
                        score: Some(1441),
                        goals: Some(8),
                        assists: Some(0),
                        saves: Some(2),
                        shots: Some(12),
                        is_bot: Some(false),
                    },
                    crate::replay_metadata::ReplayPlayerMetadata {
                        name: "Nexto".into(),
                        team: Some(1),
                        score: Some(1332),
                        goals: Some(7),
                        assists: Some(0),
                        saves: Some(3),
                        shots: Some(10),
                        is_bot: Some(true),
                    },
                ];
                entry.goals = (0..15)
                    .map(|g| crate::replay_metadata::ReplayGoalMetadata {
                        player_name: if g % 2 == 0 {
                            "cyberPeng".into()
                        } else {
                            "Nexto".into()
                        },
                        team: Some(g % 2),
                        elapsed_seconds: Some(g as u32 * 27),
                        ..Default::default()
                    })
                    .collect();
            }
            if i % 3 == 0 {
                entry.cloud_replay_id = format!("{i:08x}-1234-5678-abcd-123456789012");
            }
            if i % 3 == 1 {
                entry.file_size = 0;
                entry.cloud_replay_id = format!("{i:08x}-1234-5678-abcd-123456789012");
            }
            entries.insert(entry.filename.clone(), entry);
        }
        state.replays.merged_metadata_cache.store(Arc::new(
            crate::replay_metadata::ReplayMetadataSnapshot {
                entries,
                ..Default::default()
            },
        ));
        let mut config = Config {
            replays_folder: String::new(),
            ballchasing_api_key: String::new(),
            ..Default::default()
        };
        let mut changed = false;
        let mut confirm = None;
        library_table::assert_page_layout("replays", |ui, expanded| {
            let view_id = ui.make_persistent_id("replays_view");
            ui.data_mut(|d| d.insert_temp(view_id, ReplaysView::Library));
            if expanded {
                let id = ui.make_persistent_id("replay_table").with("selected");
                ui.data_mut(|d| d.insert_temp(id, format!("game:{:032x}", 0)));
            }
            render_replays_settings_tab(ui, &state, &mut config, &mut changed, &mut confirm);
        });
    }

    #[test]
    fn verification_discards_old_results_even_after_input_changes_back() {
        let mut verification = TokenVerification::default();
        verification.update_input("first");
        let old_revision = verification.revision;
        verification.complete(old_revision, Ok(()));
        assert!(matches!(verification.result, VerificationResult::Valid));
        verification.update_input("second");
        assert!(matches!(
            verification.result,
            VerificationResult::Unverified
        ));
        verification.update_input("first");
        verification.complete(old_revision, Ok(()));
        assert!(matches!(
            verification.result,
            VerificationResult::Unverified
        ));
        verification.complete(verification.revision, Err("denied".into()));
        assert!(matches!(verification.result, VerificationResult::Failed(_)));
    }

    #[test]
    fn library_includes_uncached_local_files_and_preserves_unknown_metadata() {
        let entry = crate::replay_metadata::ReplayMetadataEntry {
            filename: "local.replay".into(),
            display_name: "Local match".into(),
            file_size: 42,
            map_name: "FutureArena_P".into(),
            date: "unknown date".into(),
            ..Default::default()
        };
        let rows = replay_cache_rows(
            &[],
            &std::collections::HashMap::from([("local.replay".into(), entry)]),
            "",
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].upload_status, "Not recorded");
        assert_eq!(rows[0].map, "FutureArena_P");
        assert_eq!(rows[0].date, "unknown date");
        assert_eq!(
            display_replay_date("2026-09-05:12-48-00"),
            "2026-09-05 12:48"
        );
        assert_eq!(
            display_replay_date("2026-09-05T12:48:00Z"),
            chrono::DateTime::parse_from_rfc3339("2026-09-05T12:48:00Z")
                .unwrap()
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        );
    }
    use std::collections::HashMap;

    fn metadata_entry(
        filename: &str,
        display_name: &str,
    ) -> crate::replay_metadata::ReplayMetadataEntry {
        crate::replay_metadata::ReplayMetadataEntry {
            filename: filename.to_string(),
            display_name: display_name.to_string(),
            date: "2026-06-14:20-15".to_string(),
            map_name: "Stadium_P".to_string(),
            team0_score: Some(3),
            team1_score: Some(2),
            player_names: vec!["One".to_string(), "Two".to_string()],
            players: vec![crate::replay_metadata::ReplayPlayerMetadata {
                name: "One".to_string(),
                team: Some(0),
                score: Some(515),
                goals: Some(2),
                assists: Some(1),
                saves: Some(3),
                shots: Some(4),
                is_bot: Some(false),
            }],
            goals: vec![crate::replay_metadata::ReplayGoalMetadata {
                player_name: "One".to_string(),
                team: Some(0),
                frame: Some(900),
                elapsed_seconds: Some(30),
            }],
            duration_seconds: Some(60),
            file_size: 1024,
            ..Default::default()
        }
    }

    #[test]
    fn replay_cache_rows_use_newest_cached_first() {
        let uploaded = vec!["old.replay".to_string(), "new.replay".to_string()];
        let mut metadata = HashMap::new();
        metadata.insert(
            "old.replay".to_string(),
            metadata_entry("old.replay", "Old Match"),
        );
        metadata.insert(
            "new.replay".to_string(),
            metadata_entry("new.replay", "New Match"),
        );

        let rows = replay_cache_rows(&uploaded, &metadata, "");

        assert_eq!(rows[0].primary, "New Match");
        assert_eq!(rows[1].primary, "Old Match");
    }

    #[test]
    fn replay_cache_row_uses_local_metadata_when_available() {
        let entry = metadata_entry("match.replay", "Ranked Doubles");

        let row = replay_cache_row("match.replay", Some(&entry));

        assert_eq!(row.primary, "Ranked Doubles");
        assert_eq!(row.source_label, "Local");
        assert_eq!(row.map, "DFH Stadium");
        assert_eq!(row.score, "3-2");
        assert_eq!(row.players, "One + 1");
        assert!(row.hover.contains("Duration: 1:00"));
        assert!(row.hover.contains("515 pts · 2 G · 1 A · 3 S · 4 Sh"));
        assert!(row.hover.contains("0:30 · One"));
    }

    #[test]
    fn replay_cache_row_keeps_cache_only_entries_visible() {
        let row = replay_cache_row("abcdef.replay", None);

        assert_eq!(row.primary, "abcdef");
        assert_eq!(row.source_label, "Upload record only");
        assert_eq!(row.map, "-");
        assert!(row.hover.contains("No matching local replay"));
    }

    #[test]
    fn replay_cache_rows_filter_by_metadata_detail() {
        let uploaded = vec!["match.replay".to_string()];
        let mut metadata = HashMap::new();
        metadata.insert(
            "match.replay".to_string(),
            metadata_entry("match.replay", "Ranked Doubles"),
        );

        let rows = replay_cache_rows(&uploaded, &metadata, "stadium");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].primary, "Ranked Doubles");
    }

    #[test]
    fn replay_cache_rows_match_metadata_case_insensitively() {
        let uploaded = vec!["MATCH.REPLAY".to_string()];
        let mut metadata = HashMap::new();
        metadata.insert(
            "match.replay".to_string(),
            metadata_entry("match.replay", "Ranked Doubles"),
        );

        let rows = replay_cache_rows(&uploaded, &metadata, "");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].primary, "Ranked Doubles");
    }
}
