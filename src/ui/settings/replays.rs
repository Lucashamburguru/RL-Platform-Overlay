use crate::state::{AppState, Config};
use crate::ui::common::{StatusTone, helper_text, setting_row, settings_section, status_text};
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub(crate) fn render_replays_settings_tab(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config_edit: &mut Config,
    changed: &mut bool,
    confirm_modal: &mut Option<crate::ui::app::ConfirmAction>,
) {
    crate::replays::maybe_start_initial_replay_cache_sync(state);

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

                let client = state.system.ballchasing_client.clone();
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

        // Sync and Upload buttons
        ui.horizontal(|ui| {
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
                if ui.button("Stop").clicked() {
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

            // Clear Cache
            let clear_btn = ui.button("Clear Upload Cache");
            if clear_btn.clicked() {
                *confirm_modal = Some(crate::ui::app::ConfirmAction::ClearUploadCache);
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

        render_replay_cache(ui, state, config_edit, path_valid == Some(true));

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
            } else if current_status.contains("Uploading")
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

fn maybe_start_metadata_scan(state: &Arc<AppState>, folder: &str) {
    let snapshot = crate::replay_metadata::merged_metadata_snapshot(state);
    if snapshot.folder != folder {
        crate::replay_metadata::start_metadata_scan(state.clone(), folder.to_string());
    }
}

fn render_replay_cache(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    config_edit: &Config,
    path_valid: bool,
) {
    ui.separator();
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new("Replay Cache")
            .size(14.0)
            .strong()
            .color(egui::Color32::from_rgb(225, 227, 235)),
    );
    ui.add_space(4.0);

    let snapshot = crate::replay_metadata::merged_metadata_snapshot(state);
    let scan_running = state.replays.metadata_scan_running.load(Ordering::SeqCst);
    let metadata_status = state
        .replays
        .metadata_status
        .lock()
        .map(|status| status.clone())
        .unwrap_or_else(|_| "Metadata status unavailable".to_string());
    let cloud_count = state.replays.ballchasing_cloud_count.load(Ordering::SeqCst);

    ui.horizontal_wrapped(|ui| {
        ui.label(helper_text(format!(
            "{} cached uploads",
            config_edit.uploaded_replays.len()
        )));
        if cloud_count > 0 {
            ui.label(helper_text(format!("{} on Ballchasing.com", cloud_count)));
        }
        if snapshot.total_files > 0 {
            ui.label(helper_text(format!(
                "{} local metadata entries",
                snapshot.parsed
            )));
        }
        if scan_running {
            ui.add(egui::Spinner::new());
        }
        ui.label(helper_text(metadata_status));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let refresh = ui
                .add_enabled(
                    path_valid && !scan_running,
                    egui::Button::new("Refresh Metadata"),
                )
                .on_disabled_hover_text(
                    "Configure a valid replay folder before refreshing metadata.",
                );
            if refresh.clicked() {
                crate::replay_metadata::start_metadata_scan(
                    state.clone(),
                    config_edit.replays_folder.clone(),
                );
            }
        });
    });

    let cached_count = config_edit.uploaded_replays.len();
    if cached_count == 0 {
        ui.add_space(4.0);
        ui.label(helper_text(
            "No uploaded replay cache entries yet. Upload or sync replays to populate this list.",
        ));
        return;
    }

    let search_id = ui.make_persistent_id("replay_cache_search");
    let mut search = ui
        .data(|data| data.get_temp::<String>(search_id))
        .unwrap_or_default();
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(helper_text("Search"));
        if ui
            .add_sized(
                [ui.available_width().min(260.0), 22.0],
                egui::TextEdit::singleline(&mut search).hint_text("name, map, date, filename"),
            )
            .changed()
        {
            ui.data_mut(|data| data.insert_temp(search_id, search.clone()));
        }
    });

    let rows = replay_cache_rows(&config_edit.uploaded_replays, &snapshot.entries, &search);
    ui.add_space(4.0);
    egui::ScrollArea::both().max_height(210.0).show(ui, |ui| {
        egui::Grid::new("replay_cache_grid")
            .striped(true)
            .num_columns(6)
            .min_col_width(68.0)
            .show(ui, |ui| {
                ui.strong("Replay");
                ui.strong("Date");
                ui.strong("Map");
                ui.strong("Score");
                ui.strong("Players");
                ui.strong("Action");
                ui.end_row();

                for row in rows {
                    ui.label(row.primary.clone())
                        .on_hover_text(row.hover.clone());
                    ui.label(
                        egui::RichText::new(row.date.clone())
                            .size(11.0)
                            .color(egui::Color32::from_gray(178)),
                    )
                    .on_hover_text(row.hover.clone());
                    ui.label(
                        egui::RichText::new(row.map.clone())
                            .size(11.0)
                            .color(egui::Color32::from_gray(178)),
                    )
                    .on_hover_text(row.hover.clone());
                    ui.label(
                        egui::RichText::new(row.score.clone())
                            .size(11.0)
                            .color(egui::Color32::from_gray(178)),
                    )
                    .on_hover_text(row.hover.clone());
                    ui.label(
                        egui::RichText::new(row.players.clone())
                            .size(11.0)
                            .color(egui::Color32::from_gray(178)),
                    )
                    .on_hover_text(row.hover.clone());

                    let is_local = row.source_label == "Local metadata";
                    let is_failed = row.source_label == "Parse failed";
                    if is_local {
                        ui.label(
                            egui::RichText::new("Local")
                                .size(11.0)
                                .color(egui::Color32::from_rgb(100, 220, 120)),
                        )
                        .on_hover_text(row.hover.clone());
                    } else if is_failed {
                        ui.label(
                            egui::RichText::new("Error")
                                .size(11.0)
                                .color(egui::Color32::from_rgb(230, 95, 85)),
                        )
                        .on_hover_text(row.hover.clone());
                    } else {
                        let download_active = state.replays.download_active.load(Ordering::SeqCst);
                        let api_key_empty = config_edit.ballchasing_api_key.trim().is_empty();
                        let path_invalid = !path_valid;

                        let btn = ui.add_enabled(
                            !download_active && !api_key_empty && !path_invalid,
                            egui::Button::new("Download").small(),
                        );
                        if btn.clicked() {
                            let id = row.filename.trim_end_matches(".replay").to_string();
                            crate::replays::start_download_replay_task(state.clone(), id);
                        }
                    }
                    ui.end_row();
                }
            });
    });
}

#[derive(Clone, Debug, PartialEq)]
struct ReplayCacheRow {
    filename: String,
    primary: String,
    date: String,
    map: String,
    score: String,
    players: String,
    search_text: String,
    source_label: &'static str,
    source_color: egui::Color32,
    hover: String,
}

fn replay_cache_rows(
    uploaded_replays: &[String],
    metadata: &std::collections::HashMap<String, crate::replay_metadata::ReplayMetadataEntry>,
    search: &str,
) -> Vec<ReplayCacheRow> {
    let metadata_by_lower_filename: std::collections::HashMap<
        String,
        &crate::replay_metadata::ReplayMetadataEntry,
    > = metadata
        .iter()
        .map(|(filename, entry)| (filename.to_ascii_lowercase(), entry))
        .collect();
    let query = search.trim().to_ascii_lowercase();
    uploaded_replays
        .iter()
        .rev()
        .filter_map(|filename| {
            let entry = metadata_by_lower_filename
                .get(&filename.to_ascii_lowercase())
                .copied();
            let row = replay_cache_row(filename, entry);
            if query.is_empty() || row_matches_query(&row, &query) {
                Some(row)
            } else {
                None
            }
        })
        .collect()
}

fn replay_cache_row(
    filename: &str,
    entry: Option<&crate::replay_metadata::ReplayMetadataEntry>,
) -> ReplayCacheRow {
    let mut row = match entry {
        Some(entry) if entry.has_metadata() => {
            let primary = shorten_text(&entry.display_name, 36);
            let is_cloud = entry.file_size == 0;
            ReplayCacheRow {
                filename: filename.to_string(),
                primary,
                date: display_or_dash(&entry.date),
                map: display_or_dash(&entry.map_name),
                score: score_label(entry),
                players: players_label(entry),
                source_label: if is_cloud {
                    "Cloud metadata"
                } else {
                    "Local metadata"
                },
                source_color: if is_cloud {
                    egui::Color32::from_rgb(100, 180, 240)
                } else {
                    egui::Color32::from_rgb(100, 220, 120)
                },
                hover: metadata_hover(filename, entry),
                search_text: String::new(),
            }
        }
        Some(entry) => ReplayCacheRow {
            filename: filename.to_string(),
            primary: shorten_text(filename.trim_end_matches(".replay"), 36),
            date: "-".to_string(),
            map: "-".to_string(),
            score: "-".to_string(),
            players: "-".to_string(),
            source_label: "Parse failed",
            source_color: egui::Color32::from_rgb(230, 95, 85),
            hover: format!("{}\n{}", filename, entry.error),
            search_text: String::new(),
        },
        None => ReplayCacheRow {
            filename: filename.to_string(),
            primary: shorten_text(filename.trim_end_matches(".replay"), 36),
            date: "-".to_string(),
            map: "-".to_string(),
            score: "-".to_string(),
            players: "-".to_string(),
            source_label: "Cache only",
            source_color: egui::Color32::from_gray(165),
            hover: format!("{filename}\nNo matching local replay file for metadata."),
            search_text: String::new(),
        },
    };
    row.search_text = replay_row_search_text(&row);
    row
}

fn display_or_dash(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        "-".to_string()
    } else {
        shorten_text(value, 22)
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
    let mut lines = vec![format!("File: {filename}")];
    if !entry.replay_id.trim().is_empty() {
        lines.push(format!("Replay ID: {}", entry.replay_id));
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
        assert_eq!(row.source_label, "Local metadata");
        assert_eq!(row.map, "Stadium_P");
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
        assert_eq!(row.source_label, "Cache only");
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
