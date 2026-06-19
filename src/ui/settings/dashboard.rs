use crate::state::{AppState, Config};
use crate::ui::common::{helper_text, setting_row, settings_section, settings_two_column};
use eframe::egui;
use std::sync::Arc;

use crate::state::DashboardPlayerLayout;

pub(crate) fn render_dashboard_settings_tab(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &Arc<AppState>,
    config_edit: &mut Config,
    changed: &mut bool,
) {
    settings_section(ui, "Second Screen Dashboard", |ui| {
        settings_two_column(ui, |left, right| {
            setting_row(left, "Dashboard", |ui| {
                if ui
                    .checkbox(&mut config_edit.dashboard_enabled, "Enable Dashboard")
                    .changed()
                {
                    *changed = true;
                }
            });

            left.add_space(8.0);
            setting_row(left, "Launch", |ui| {
                if ui
                    .checkbox(
                        &mut config_edit.dashboard_open_with_overlay,
                        "Open with Overlay",
                    )
                    .changed()
                {
                    *changed = true;
                }
            });

            left.add_space(8.0);
            setting_row(left, "Overlay", |ui| {
                if ui
                    .checkbox(
                        &mut config_edit.dashboard_keep_overlay_enabled,
                        "Keep Overlay Enabled",
                    )
                    .changed()
                {
                    *changed = true;
                }
            });

            left.add_space(8.0);
            setting_row(left, "Display", |ui| {
                ui.vertical(|ui| {
                    if ui
                        .checkbox(&mut config_edit.dashboard_show_boost, "Show Boost")
                        .changed()
                    {
                        *changed = true;
                    }
                    if ui
                        .checkbox(&mut config_edit.dashboard_show_ranks, "Show Ranks")
                        .changed()
                    {
                        *changed = true;
                    }
                    if ui
                        .checkbox(
                            &mut config_edit.dashboard_show_team_comparison,
                            "Team Comparison",
                        )
                        .changed()
                    {
                        *changed = true;
                    }
                    if ui
                        .checkbox(&mut config_edit.dashboard_show_event_feed, "Event Feed")
                        .changed()
                    {
                        *changed = true;
                    }
                    if ui
                        .checkbox(
                            &mut config_edit.dashboard_show_replay_upload,
                            "Replay Upload",
                        )
                        .changed()
                    {
                        *changed = true;
                    }
                });
            });

            left.add_space(8.0);
            setting_row(left, "Player Layout", |ui| {
                if ui
                    .radio_value(
                        &mut config_edit.dashboard_player_layout,
                        DashboardPlayerLayout::Cards,
                        "Cards",
                    )
                    .changed()
                {
                    *changed = true;
                }
                if ui
                    .radio_value(
                        &mut config_edit.dashboard_player_layout,
                        DashboardPlayerLayout::Table,
                        "Table",
                    )
                    .changed()
                {
                    *changed = true;
                }
            });

            setting_row(right, "Window", |ui| {
                if ui
                    .checkbox(&mut config_edit.dashboard_fullscreen, "Fullscreen")
                    .changed()
                {
                    *changed = true;
                }
            });

            right.add_space(8.0);
            let monitors = crate::ui::monitor::available_monitors(ctx);
            setting_row(right, "Monitor", |ui| {
                if monitors.is_empty() {
                    if ui
                        .add(
                            egui::DragValue::new(&mut config_edit.dashboard_monitor_index)
                                .range(0..=16),
                        )
                        .changed()
                    {
                        *changed = true;
                    }
                } else {
                    egui::ComboBox::new("dashboard_monitor_index", "")
                        .selected_text(config_edit.dashboard_monitor_index.to_string())
                        .show_ui(ui, |ui| {
                            for monitor in &monitors {
                                if ui
                                    .selectable_value(
                                        &mut config_edit.dashboard_monitor_index,
                                        monitor.index,
                                        monitor.index.to_string(),
                                    )
                                    .changed()
                                {
                                    *changed = true;
                                }
                            }
                        });
                }
            });

            right.add_space(4.0);
            right.label(helper_text(crate::ui::monitor::monitor_summary(
                &monitors,
                config_edit.dashboard_monitor_index,
            )));
            right.label(helper_text(if cfg!(target_os = "windows") {
                "Monitor indices are enumerated from Windows display geometry."
            } else {
                "Monitor index targeting is best-effort on this platform."
            }));
        });
    });

    ui.add_space(10.0);
    settings_section(ui, "Status", |ui| {
        let is_launched = state
            .flags
            .is_launched
            .load(std::sync::atomic::Ordering::SeqCst);
        ui.horizontal_wrapped(|ui| {
            ui.label("Overlay:");
            ui.label(if is_launched { "running" } else { "stopped" });
            ui.separator();
            ui.label("Dashboard:");
            ui.label(if config_edit.dashboard_enabled {
                "enabled"
            } else {
                "off"
            });
        });
    });
}
