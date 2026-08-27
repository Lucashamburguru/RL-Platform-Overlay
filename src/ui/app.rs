use crate::state::AppState;
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use super::boost_hud::{render_teammate_boost, render_teammate_boost_position_preview};
use super::dashboard::{DashboardViewportState, render_dashboard_viewport};
use super::debug::render_debug_settings_tab;
use super::hotkeys::egui_to_rdev_key;
use super::lobby_overlay::render_overlay;
use super::session_hud::render_session_overlay;
use super::settings::{
    ArrangeHudAction, render_boost_settings_tab, render_history_settings_tab,
    render_launch_controls, render_overlay_settings_tab, render_replays_settings_tab,
    render_session_settings_tab, render_settings_tabs, render_setup_settings_tab,
    render_update_notice,
};

#[derive(Clone, Copy, Debug, PartialEq)]
struct HudPositionSnapshot {
    lobby: Option<[f32; 2]>,
    boost: Option<[f32; 2]>,
    session: Option<[f32; 2]>,
}

impl HudPositionSnapshot {
    fn from_config(config: &crate::state::Config) -> Self {
        Self {
            lobby: config.lobby_manual_position,
            boost: config.teammate_boost_manual_position,
            session: config.session_manual_position,
        }
    }

    fn restore(self, config: &mut crate::state::Config) {
        config.lobby_manual_position = self.lobby;
        config.teammate_boost_manual_position = self.boost;
        config.session_manual_position = self.session;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmAction {
    ResetConfig,
    ClearUploadCache,
    #[cfg(not(feature = "microsoft-store"))]
    AlphaBoostApply,
    #[cfg(not(feature = "microsoft-store"))]
    AlphaBoostRestore,
    DeleteBackups,
    ClearHistory,
}

pub struct MainApp {
    state: Arc<AppState>,
    settings_tab: SettingsTab,
    is_rl_running: bool,
    rl_process_detection_detail: String,
    last_rl_check: std::time::Instant,
    last_logged_show_settings: Option<bool>,
    #[allow(clippy::type_complexity)]
    last_viewport_state: Option<(bool, bool, bool, bool, Option<egui::Pos2>, [f32; 2])>,
    dashboard_viewport_state: DashboardViewportState,
    #[allow(dead_code)]
    hwnd: Option<isize>,
    rocket_league_process_watcher: crate::assets::RocketLeagueProcessWatcher,
    launched_by_layout_mode: bool,
    hud_position_snapshot: Option<HudPositionSnapshot>,
    confirm_modal: Option<ConfirmAction>,
    tos_accepted: bool,
    history_search_query: String,
}

impl MainApp {
    pub fn new(state: Arc<AppState>, hwnd: Option<isize>) -> Self {
        Self {
            state,
            settings_tab: SettingsTab::Overlay,
            is_rl_running: false,
            rl_process_detection_detail: "not checked".to_string(),
            last_rl_check: std::time::Instant::now()
                .checked_sub(std::time::Duration::from_secs(5))
                .unwrap_or_else(std::time::Instant::now),
            last_logged_show_settings: None,
            last_viewport_state: None,
            dashboard_viewport_state: DashboardViewportState::default(),
            hwnd,
            rocket_league_process_watcher: crate::assets::RocketLeagueProcessWatcher::new(),
            launched_by_layout_mode: false,
            hud_position_snapshot: None,
            confirm_modal: None,
            tos_accepted: false,
            history_search_query: String::new(),
        }
    }

    fn refresh_rocket_league_process_detection(&mut self) {
        let now = std::time::Instant::now();
        if !should_poll_rocket_league_process(now, self.last_rl_check) {
            return;
        }

        let detection = self.rocket_league_process_watcher.detect();
        if !self.is_rl_running && detection.running {
            let setup_result = self.state.system.stats_api_setup_result.load();
            if setup_result.restart_required {
                self.state.system.stats_api_setup_result.store(Arc::new(
                    crate::setup::StatsApiSetupResult {
                        restart_required: false,
                        message: "Rocket League restart detected. The Stats API setting should now be active."
                            .to_string(),
                        ..(**setup_result).clone()
                    },
                ));
            }
        }
        self.is_rl_running = detection.running;
        self.rl_process_detection_detail = detection.detail;
        self.last_rl_check = now;
    }
}

fn should_poll_rocket_league_process(
    now: std::time::Instant,
    last_check: std::time::Instant,
) -> bool {
    now.duration_since(last_check) >= std::time::Duration::from_secs(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_polling_rechecks_after_interval_even_if_running() {
        let now = std::time::Instant::now();
        assert!(should_poll_rocket_league_process(
            now,
            now - std::time::Duration::from_secs(2)
        ));
        assert!(!should_poll_rocket_league_process(
            now,
            now - std::time::Duration::from_millis(1500)
        ));
    }

    #[test]
    fn hud_position_snapshot_restores_all_movable_panels() {
        let mut config = crate::state::Config {
            lobby_manual_position: Some([0.1, 0.2]),
            teammate_boost_manual_position: None,
            session_manual_position: Some([0.3, 0.4]),
            ..Default::default()
        };
        let snapshot = HudPositionSnapshot::from_config(&config);

        config.lobby_manual_position = None;
        config.teammate_boost_manual_position = Some([0.5, 0.6]);
        config.session_manual_position = None;
        snapshot.restore(&mut config);

        assert_eq!(config.lobby_manual_position, Some([0.1, 0.2]));
        assert_eq!(config.teammate_boost_manual_position, None);
        assert_eq!(config.session_manual_position, Some([0.3, 0.4]));
    }

    #[test]
    fn dashboard_only_repaint_uses_dashboard_cadence() {
        let inputs = RepaintInputs {
            is_launched: false,
            show_settings: false,
            show_hud: false,
            show_session_overlay: false,
            show_boost_panel: false,
            show_dashboard: true,
            layout_mode: false,
        };

        assert_eq!(
            repaint_delay(&inputs, false, false),
            Duration::from_millis(100)
        );
    }

    #[test]
    fn visible_hud_keeps_interactive_repaint_cadence() {
        let inputs = RepaintInputs {
            is_launched: true,
            show_settings: false,
            show_hud: true,
            show_session_overlay: false,
            show_boost_panel: false,
            show_dashboard: false,
            layout_mode: false,
        };

        assert_eq!(
            repaint_delay(&inputs, false, false),
            Duration::from_millis(16)
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SettingsTab {
    Setup,
    Overlay,
    Dashboard,
    Session,
    Boost,
    Replays,
    History,
    Debug,
}

impl eframe::App for MainApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let is_launched = self.state.flags.is_launched.load(Ordering::SeqCst);
        if is_launched {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            [0.12, 0.12, 0.12, 1.0]
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_zoom_factor(1.0);
        self.state.diagnostics.frame_tracker.record_frame();
        self.state.diagnostics.foreground_tracker.record_sample();

        if self.state.flags.should_exit.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let is_launched = self.state.flags.is_launched.load(Ordering::SeqCst);
        let is_recording_any = self.state.hotkeys.is_recording_kb.load(Ordering::SeqCst)
            || self.state.hotkeys.is_recording_ctrl.load(Ordering::SeqCst)
            || self
                .state
                .hotkeys
                .is_recording_settings
                .load(Ordering::SeqCst)
            || self
                .state
                .hotkeys
                .is_recording_launch
                .load(Ordering::SeqCst);
        let show_settings =
            self.state.flags.is_settings_visible.load(Ordering::SeqCst) || is_recording_any;
        if self
            .state
            .system
            .stats_api_setup_attention_requested
            .swap(false, Ordering::SeqCst)
        {
            self.settings_tab = SettingsTab::Setup;
        }

        // Leaving the guided arrangement workflow without Done behaves like Cancel.
        let mut config = self.state.system.config.load();
        if (!show_settings || !is_launched) && config.layout_mode {
            let snapshot = self.hud_position_snapshot.take();
            self.state.update_config(|config| {
                if let Some(snapshot) = snapshot {
                    snapshot.restore(config);
                }
                config.layout_mode = false;
            });
            config = self.state.system.config.load();
            if self.launched_by_layout_mode {
                self.state.flags.is_launched.store(false, Ordering::SeqCst);
                self.launched_by_layout_mode = false;
            }
        }
        let is_launched =
            if config.dashboard_enabled && !config.dashboard_keep_overlay_enabled && is_launched {
                self.state.flags.is_launched.store(false, Ordering::SeqCst);
                false
            } else {
                is_launched
            };

        let lobby_hotkey_visible = self.state.flags.is_visible.load(Ordering::SeqCst);
        let show_hud = is_launched && (lobby_hotkey_visible || config.layout_mode);
        let show_session_overlay = is_launched
            && config.session_overlay_enabled
            && (!config.session_overlay_follow_lobby_hotkey
                || lobby_hotkey_visible
                || config.layout_mode);
        let show_boost_position_preview =
            (is_launched && config.show_teammate_boost && config.layout_mode)
                || (show_settings
                    && self.settings_tab == SettingsTab::Boost
                    && config.show_teammate_boost);
        let show_boost_hud =
            is_launched && config.show_teammate_boost && !show_settings && !config.layout_mode;
        let show_dashboard = config.dashboard_enabled;
        let mouse_passthrough = is_launched && !show_settings && !config.layout_mode;
        #[allow(unused_mut)]
        let mut target_size = if is_launched {
            config.window_size
        } else {
            [760.0, 820.0]
        };

        #[allow(unused_mut)]
        let mut target_pos = None;
        #[allow(unused_mut)]
        let mut fullscreen = false;

        #[cfg(target_os = "windows")]
        if is_launched && let Some(hwnd) = self.hwnd {
            use winapi::shared::windef::HWND;
            use winapi::um::winuser::{
                GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
            };
            let hwnd_val = hwnd as HWND;
            unsafe {
                let hmonitor = MonitorFromWindow(hwnd_val, MONITOR_DEFAULTTONEAREST);
                let mut info: MONITORINFO = std::mem::zeroed();
                info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
                if GetMonitorInfoW(hmonitor, &mut info as *mut MONITORINFO as *mut _) != 0 {
                    let scale = ctx.pixels_per_point();
                    let monitor_x = info.rcMonitor.left as f32 / scale;
                    let monitor_y = info.rcMonitor.top as f32 / scale;
                    let monitor_w = (info.rcMonitor.right - info.rcMonitor.left) as f32 / scale;
                    let monitor_h = (info.rcMonitor.bottom - info.rcMonitor.top) as f32 / scale;

                    target_pos = Some(egui::pos2(monitor_x, monitor_y));
                    target_size = [monitor_w, monitor_h - 1.0];
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        if is_launched {
            fullscreen = true;
        }

        // Style enforcement on Windows
        #[cfg(target_os = "windows")]
        if let Some(hwnd) = self.hwnd {
            use winapi::shared::windef::HWND;
            use winapi::um::winuser::{GWL_EXSTYLE, GetWindowLongW, WS_EX_LAYERED};
            let hwnd_val = hwnd as HWND;
            unsafe {
                let ex_style = GetWindowLongW(hwnd_val, GWL_EXSTYLE);
                let is_layered = (ex_style & WS_EX_LAYERED as i32) != 0;
                if is_launched != is_layered {
                    set_window_transparency(hwnd, is_launched);
                }
                enforce_borderless_style(hwnd);
            }
        }

        let viewport_state = (
            is_launched,
            show_settings,
            mouse_passthrough,
            fullscreen,
            target_pos,
            target_size,
        );
        if self.last_viewport_state != Some(viewport_state) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(fullscreen));
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(mouse_passthrough));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(target_size.into()));
            if let Some(pos) = target_pos {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
            }
            ctx.request_repaint();
            self.last_viewport_state = Some(viewport_state);
        }

        if is_launched {
            // 1. Unified Background (Transparent)
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 0)))
                .show(ctx, |_ui| {
                    if show_hud {
                        render_overlay(ctx, &self.state);
                    }
                    if show_session_overlay {
                        render_session_overlay(ctx, &self.state);
                    }

                    if self.last_logged_show_settings != Some(show_settings) {
                        crate::input::append_hotkey_debug_log(
                            self.state.debug_logging_enabled.load(Ordering::SeqCst),
                            format!(
                                "ui_show_settings visible={show_settings} launched={is_launched} recording_kb={} recording_ctrl={} recording_settings={} recording_launch={}",
                                self.state.hotkeys.is_recording_kb.load(Ordering::SeqCst),
                                self.state.hotkeys.is_recording_ctrl.load(Ordering::SeqCst),
                                self.state.hotkeys.is_recording_settings.load(Ordering::SeqCst),
                                self.state.hotkeys.is_recording_launch.load(Ordering::SeqCst)
                            ),
                        );
                        self.last_logged_show_settings = Some(show_settings);
                    }

                    // 2. Always-on Teammate Boost HUD
                    // Settings mode uses the Boost tab preview instead of the floating in-game HUD.
                    if config.show_teammate_boost && config.layout_mode {
                        render_teammate_boost_position_preview(ctx, &self.state, true);
                    } else if show_boost_hud {
                        render_teammate_boost(ctx, &self.state);
                    } else if show_boost_position_preview {
                        render_teammate_boost_position_preview(ctx, &self.state, false);
                    }

                    if config.layout_mode {
                        super::layout::render_arrange_hud_banner(ctx);
                    }

                    // Keep window on top every frame when launched
                    ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                        egui::WindowLevel::AlwaysOnTop,
                    ));

                    // Show gear icon ONLY if settings are hidden AND mouse is in top-left
                    if !show_settings {
                        let mouse_pos = ctx.input(|i| {
                            i.pointer
                                .interact_pos()
                                .unwrap_or(egui::Pos2::new(-100.0, -100.0))
                        });
                        let gear_rect =
                            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0));

                        if gear_rect.contains(mouse_pos) {
                            egui::Area::new("settings_toggle".into())
                                .anchor(egui::Align2::LEFT_TOP, egui::vec2(10.0, 10.0))
                                .show(ctx, |ui| {
                                    let btn = ui.add(egui::Button::new("⚙ Settings").frame(true));
                                    if btn.clicked() {
                                        crate::input::append_hotkey_debug_log(
                                            self.state.debug_logging_enabled.load(Ordering::SeqCst),
                                            "gear_settings_button_clicked visible=true",
                                        );
                                        self.state.flags.is_settings_visible.store(true, Ordering::SeqCst);
                                    }
                                });
                        }
                    }

                    let settings_hotkey = config.hotkey_settings.clone();
                    let launch_hotkey = config.hotkey_launch.clone();
                    let hud_hotkey = config.hotkey_kb.clone();
                    if !is_recording_any {
                        ctx.input(|i| {
                            for event in &i.events {
                                if let egui::Event::Key { key, pressed, .. } = event
                                    && let Some(name) = egui_to_rdev_key(*key)
                                {
                                    if *pressed && name == settings_hotkey {
                                        crate::input::append_hotkey_debug_log(
                                            self.state.debug_logging_enabled.load(Ordering::SeqCst),
                                            format!(
                                                "egui_keypress key={name} settings_match=true"
                                            ),
                                        );
                                        crate::input::toggle_settings_hotkey(&self.state, "egui");
                                    }

                                    if *pressed && name == launch_hotkey {
                                        crate::input::toggle_launch_hotkey(&self.state, "egui");
                                    }

                                    if crate::input::keyboard_hotkey_matches(&name, &hud_hotkey) {
                                        crate::input::handle_hud_hotkey_event(
                                            &self.state,
                                            *pressed,
                                            "egui",
                                        );
                                    }
                                }
                            }
                        });
                    }

                    // Float settings Window over overlay
                    if show_settings {
                        let mut settings_open = true;
                        egui::Window::new("RL Overlay Settings")
                            .collapsible(true)
                            .resizable(true)
                            .movable(true)
                            .default_pos([16.0, 16.0])
                            .default_size([760.0, 720.0])
                            .min_width(640.0)
                            .min_height(520.0)
                            .constrain_to(ctx.screen_rect().shrink(8.0))
                            .open(&mut settings_open)
                            .show(ctx, |ui| {
                                self.render_settings_content(ui, ctx, is_launched);
                            });
                        if !settings_open {
                            crate::input::append_hotkey_debug_log(
                                self.state.debug_logging_enabled.load(Ordering::SeqCst),
                                "settings_window_close_clicked visible=false",
                            );
                            self.state
                                .flags
                                .is_settings_visible
                                .store(false, Ordering::SeqCst);
                        }
                    }
                });
        } else {
            // Launcher Stopped: Render dashboard or settings directly filled
            // Custom Title Bar
            egui::TopBottomPanel::top("custom_title_bar")
                .frame(
                    egui::Frame::default()
                        .fill(egui::Color32::from_rgb(20, 20, 25))
                        .inner_margin(8.0),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("⚙  RL Platform Overlay")
                                .strong()
                                .color(egui::Color32::from_rgb(220, 220, 230))
                                .size(13.0),
                        );

                        let mut close_clicked = false;
                        let mut min_clicked = false;

                        let button_rects = ui
                            .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let button_style =
                                    |ui: &mut egui::Ui,
                                     text: &str,
                                     hover_color: egui::Color32|
                                     -> egui::Response {
                                        let (rect, response) = ui.allocate_exact_size(
                                            egui::vec2(28.0, 24.0),
                                            egui::Sense::click(),
                                        );

                                        let bg_color = if response.is_pointer_button_down_on() {
                                            hover_color.linear_multiply(0.8)
                                        } else if response.hovered() {
                                            hover_color
                                        } else {
                                            egui::Color32::TRANSPARENT
                                        };

                                        ui.painter().rect_filled(rect, 3.0, bg_color);
                                        ui.painter().text(
                                            rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            text,
                                            egui::FontId::proportional(11.0),
                                            if response.hovered() {
                                                egui::Color32::WHITE
                                            } else {
                                                egui::Color32::from_rgb(180, 180, 190)
                                            },
                                        );
                                        response
                                    };

                                let close_resp =
                                    button_style(ui, "🗙", egui::Color32::from_rgb(200, 50, 50));
                                let min_resp =
                                    button_style(ui, "🗕", egui::Color32::from_rgb(60, 60, 70));

                                (close_resp, min_resp)
                            })
                            .inner;

                        if button_rects.0.clicked() {
                            close_clicked = true;
                        }
                        if button_rects.1.clicked() {
                            min_clicked = true;
                        }

                        if close_clicked {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if min_clicked {
                            ui.ctx()
                                .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }

                        // Drag region covers everything except the buttons
                        let title_bar_rect = ui.max_rect();
                        let buttons_left_x = button_rects.1.rect.left();
                        let drag_rect = egui::Rect::from_min_max(
                            title_bar_rect.left_top(),
                            egui::pos2(buttons_left_x - 4.0, title_bar_rect.bottom()),
                        );

                        let drag_id = ui.id().with("title_bar_drag");
                        let drag_response = ui.interact(drag_rect, drag_id, egui::Sense::drag());
                        if drag_response.is_pointer_button_down_on() {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        }
                    });
                });

            if show_settings {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.inner_margin(12.0))
                    .show(ctx, |ui| {
                        self.render_settings_content(ui, ctx, is_launched);
                    });
            } else {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.inner_margin(20.0))
                    .show(ctx, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(100.0);
                            ui.heading("RL Platform Overlay");
                            ui.add_space(20.0);
                            ui.label("The overlay is currently stopped.");
                            ui.add_space(20.0);
                            if ui
                                .button(egui::RichText::new("Open Settings").heading())
                                .clicked()
                            {
                                self.state
                                    .flags
                                    .is_settings_visible
                                    .store(true, Ordering::SeqCst);
                            }
                            ui.add_space(10.0);
                            if ui
                                .button(egui::RichText::new("Launch Overlay").heading())
                                .clicked()
                            {
                                if crate::input::try_launch_overlay(
                                    &self.state,
                                    "stopped_screen_button",
                                ) {
                                    if config.dashboard_open_with_overlay
                                        && !config.dashboard_enabled
                                    {
                                        self.state.update_config(|config| {
                                            config.dashboard_enabled = true
                                        });
                                    }
                                    self.state
                                        .flags
                                        .is_settings_visible
                                        .store(false, Ordering::SeqCst);
                                } else {
                                    self.settings_tab = SettingsTab::Setup;
                                }
                            }
                            ui.add_space(20.0);
                            if ui.button("Quit").clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        });
                    });
            }

            if !is_recording_any {
                let launch_hotkey = config.hotkey_launch.clone();
                ctx.input(|i| {
                    for event in &i.events {
                        if let egui::Event::Key {
                            key, pressed: true, ..
                        } = event
                            && let Some(name) = egui_to_rdev_key(*key)
                            && name == launch_hotkey
                        {
                            crate::input::toggle_launch_hotkey(&self.state, "egui");
                        }
                    }
                });
            }
        }

        self.render_confirm_modal(ctx);

        if show_dashboard {
            render_dashboard_viewport(
                ctx,
                self.state.clone(),
                config.clone(),
                &mut self.dashboard_viewport_state,
            );
        } else {
            self.dashboard_viewport_state = DashboardViewportState::default();
        }

        schedule_repaint(
            ctx,
            &self.state,
            RepaintInputs {
                is_launched,
                show_settings,
                show_hud,
                show_session_overlay,
                show_boost_panel: show_boost_hud || show_boost_position_preview,
                show_dashboard,
                layout_mode: config.layout_mode,
            },
        );
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Err(error) = self.state.flush_config() {
            log::error!("Could not flush configuration during shutdown: {error}");
        }
    }
}

struct RepaintInputs {
    is_launched: bool,
    show_settings: bool,
    show_hud: bool,
    show_session_overlay: bool,
    show_boost_panel: bool,
    show_dashboard: bool,
    layout_mode: bool,
}

fn schedule_repaint(ctx: &egui::Context, state: &Arc<AppState>, inputs: RepaintInputs) {
    let has_drag_input = inputs.layout_mode && ctx.input(|input| input.pointer.any_down());
    let has_spinner = state.mmr.local_mmr.load().fetching
        || state.diagnostics.debug_capture_status.load().running
        || boost_operation_running(state);
    ctx.request_repaint_after(repaint_delay(&inputs, has_drag_input, has_spinner));
}

fn repaint_delay(inputs: &RepaintInputs, has_drag_input: bool, has_spinner: bool) -> Duration {
    let needs_animation = inputs.show_hud
        || inputs.show_session_overlay
        || inputs.show_boost_panel
        || has_drag_input
        || has_spinner;

    if needs_animation {
        Duration::from_millis(16)
    } else if inputs.is_launched || inputs.show_dashboard {
        Duration::from_millis(100)
    } else if inputs.show_settings {
        Duration::from_millis(250)
    } else {
        Duration::from_millis(1000)
    }
}

fn boost_operation_running(state: &Arc<AppState>) -> bool {
    let status = state
        .boost
        .boost_swap_status
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let status = status.as_str();
    !status.is_empty()
        && status != "Idle"
        && !status.starts_with("Error")
        && !status.starts_with("Download failed")
        && !status.starts_with("Backup failed")
        && !status.starts_with("Swap failed")
        && !status.starts_with("Restore failed")
        && !status.starts_with("Failed")
        && !status.starts_with("Blocked")
        && !status.starts_with("Success")
}

/// Configures window transparency on Windows using extended styling and Desktop Window Manager (DWM).
///
/// When `transparent` is true, this function turns the window into a layered window
/// (`WS_EX_LAYERED`) and extends the DWM glass frame margin completely into the client area (`-1`).
/// Any clear-color pixels (transparent black: `[0, 0, 0, 0]`) rendered by egui will show as
/// fully transparent, enabling the transparent overlay.
///
/// When `transparent` is false, it removes the layered window style and resets margins to `0`
/// to restore standard solid rendering.
#[cfg(target_os = "windows")]
fn set_window_transparency(hwnd: isize, transparent: bool) {
    use winapi::shared::windef::HWND;
    use winapi::um::dwmapi::DwmExtendFrameIntoClientArea;
    use winapi::um::uxtheme::MARGINS;
    use winapi::um::winuser::{
        GWL_EXSTYLE, GetWindowLongW, LWA_ALPHA, SetLayeredWindowAttributes, SetWindowLongW,
        WS_EX_LAYERED,
    };

    let hwnd = hwnd as HWND;
    unsafe {
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        if transparent {
            if (ex_style & WS_EX_LAYERED as i32) == 0 {
                SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_LAYERED as i32);
                SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
            }
            let margins = MARGINS {
                cxLeftWidth: -1,
                cxRightWidth: -1,
                cyTopHeight: -1,
                cyBottomHeight: -1,
            };
            let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        } else {
            if (ex_style & WS_EX_LAYERED as i32) != 0 {
                SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style & !(WS_EX_LAYERED as i32));
            }
            let margins = MARGINS {
                cxLeftWidth: 0,
                cxRightWidth: 0,
                cyTopHeight: 0,
                cyBottomHeight: 0,
            };
            let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        }
    }
}

/// Enforces a borderless style on Windows by stripping decorations, title captions, resize borders,
/// and native system menu caption buttons.
///
/// This is called on every frame update on Windows because window management updates inside `winit`'s
/// event loop (like resizing or repositioning) can asynchronously reset window styles and re-apply
/// native borders or accessibility buttons.
///
/// It checks if any of the target decoration styles (`WS_CAPTION`, `WS_SYSMENU`, `WS_THICKFRAME`,
/// `WS_MINIMIZEBOX`, `WS_MAXIMIZEBOX`) are set, strips them if present via `SetWindowLongW`,
/// and issues `SetWindowPos` with `SWP_FRAMECHANGED` to force Windows to re-evaluate the frame.
#[cfg(target_os = "windows")]
fn enforce_borderless_style(hwnd: isize) {
    use winapi::shared::windef::HWND;
    use winapi::um::winuser::{
        GWL_STYLE, GetWindowLongW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SWP_NOZORDER, SetWindowLongW, SetWindowPos, WS_CAPTION, WS_MAXIMIZEBOX, WS_MINIMIZEBOX,
        WS_SYSMENU, WS_THICKFRAME,
    };

    let hwnd = hwnd as HWND;
    unsafe {
        let style = GetWindowLongW(hwnd, GWL_STYLE);
        let target_style = style
            & !(WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX) as i32;
        if style != target_style {
            SetWindowLongW(hwnd, GWL_STYLE, target_style);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
}

impl MainApp {
    fn render_settings_content(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        is_launched: bool,
    ) {
        ui.add_space(5.0);

        self.refresh_rocket_league_process_detection();
        let mut config_session = self.state.begin_config_edit();
        let config = Arc::new(config_session.snapshot().clone());
        let config_edit = config_session.config_mut();
        let mut changed = false;
        if !self.state.debug_enabled && self.settings_tab == SettingsTab::Debug {
            self.settings_tab = SettingsTab::Setup;
        }

        render_update_notice(ui, &self.state);
        render_settings_tabs(ui, &mut self.settings_tab, self.state.debug_enabled);
        if self.settings_tab == SettingsTab::History && config_edit.history_enabled {
            crate::history::request_all_player_history_refresh(&self.state, false);
        }

        egui::ScrollArea::vertical()
            .max_height(ui.available_height() - 40.0 * ui.ctx().pixels_per_point().min(1.2))
            .show(ui, |ui| match self.settings_tab {
                SettingsTab::Setup => render_setup_settings_tab(
                    ui,
                    ctx,
                    &self.state,
                    config_edit,
                    &mut changed,
                    self.is_rl_running,
                    &self.rl_process_detection_detail,
                ),
                SettingsTab::Overlay => render_overlay_settings_tab(
                    ui,
                    ctx,
                    &self.state,
                    &config,
                    config_edit,
                    &mut changed,
                    is_launched,
                ),
                SettingsTab::Dashboard => super::settings::render_dashboard_settings_tab(
                    ui,
                    ctx,
                    &self.state,
                    config_edit,
                    &mut changed,
                ),
                SettingsTab::Session => {
                    render_session_settings_tab(ui, &self.state, config_edit, &mut changed)
                }
                SettingsTab::Boost => render_boost_settings_tab(
                    ui,
                    &self.state,
                    config_edit,
                    &mut changed,
                    self.is_rl_running,
                    &mut self.confirm_modal,
                ),
                SettingsTab::Replays => render_replays_settings_tab(
                    ui,
                    &self.state,
                    config_edit,
                    &mut changed,
                    &mut self.confirm_modal,
                ),
                SettingsTab::History => render_history_settings_tab(
                    ui,
                    &self.state,
                    config_edit,
                    &mut changed,
                    &mut self.confirm_modal,
                    &mut self.history_search_query,
                ),
                SettingsTab::Debug => render_debug_settings_tab(
                    ui,
                    &self.state,
                    is_launched,
                    self.is_rl_running,
                    &self.rl_process_detection_detail,
                ),
            });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);
        let arrange_action = render_launch_controls(
            ui,
            ctx,
            &self.state,
            is_launched,
            config_edit,
            &mut changed,
            &mut self.confirm_modal,
        );

        match arrange_action {
            Some(ArrangeHudAction::Start) => {
                self.hud_position_snapshot = Some(HudPositionSnapshot::from_config(&config));
            }
            Some(ArrangeHudAction::Done) => {
                self.hud_position_snapshot = None;
            }
            Some(ArrangeHudAction::Cancel) => {
                if let Some(snapshot) = self.hud_position_snapshot.take() {
                    snapshot.restore(config_edit);
                }
            }
            None => {}
        }

        if changed {
            // Auto-enable launch when drag positioning is turned on
            if config_edit.layout_mode && !config.layout_mode {
                if !is_launched {
                    if crate::input::try_launch_overlay_at_path(
                        &self.state,
                        &config_edit.rocket_league_path,
                        "layout_mode",
                    ) {
                        self.launched_by_layout_mode = true;
                    } else {
                        config_edit.layout_mode = false;
                        self.hud_position_snapshot = None;
                        self.settings_tab = SettingsTab::Setup;
                    }
                }
            }
            // Auto-disable launch when drag positioning is turned off, if it was launched by layout mode
            else if !config_edit.layout_mode
                && config.layout_mode
                && is_launched
                && self.launched_by_layout_mode
            {
                self.state.flags.is_launched.store(false, Ordering::SeqCst);
                self.launched_by_layout_mode = false;
            }
            let history_enabled = config_edit.history_enabled;
            let history_indicators_enabled = config_edit.lobby_history_indicators_enabled;
            config_session.commit();
            if history_enabled {
                crate::history::request_all_player_history_refresh(&self.state, true);
                crate::history::refresh_totals(&self.state);
                if history_indicators_enabled {
                    crate::history::refresh_lobby_history(&self.state);
                }
            } else {
                self.state
                    .history
                    .player_summaries
                    .store(Arc::new(Default::default()));
            }
        }
    }

    fn render_confirm_modal(&mut self, ctx: &egui::Context) {
        let Some(action) = self.confirm_modal else {
            return;
        };

        let mut open = true;
        let mut close_modal = false;
        let mut proceed = false;

        egui::Window::new("Confirm Action")
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .collapsible(false)
            .resizable(false)
            .movable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 10.0;
                self.render_confirm_modal_body(ui, action);

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let confirm_enabled = match action {
                        #[cfg(not(feature = "microsoft-store"))]
                        ConfirmAction::AlphaBoostApply => self.tos_accepted,
                        _ => true,
                    };

                    if ui
                        .add_enabled(confirm_enabled, egui::Button::new("Yes, Proceed"))
                        .clicked()
                    {
                        proceed = true;
                        close_modal = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close_modal = true;
                    }
                });
            });

        if !open || close_modal {
            self.confirm_modal = None;
            self.tos_accepted = false;
        }

        if proceed {
            self.perform_confirm_action(action);
        }
    }

    fn render_confirm_modal_body(&mut self, ui: &mut egui::Ui, action: ConfirmAction) {
        match action {
            ConfirmAction::ResetConfig => {
                ui.label("Are you sure you want to reset all settings to default?");
                ui.label(
                    "This will revert your customized hotkeys, HUD scales, opacity, and other settings.",
                );
            }
            ConfirmAction::ClearUploadCache => {
                ui.label("Are you sure you want to clear your uploaded replays cache?");
                ui.label(
                    "This will cause the auto-uploader to re-check and potentially re-upload local replays.",
                );
            }
            #[cfg(not(feature = "microsoft-store"))]
            ConfirmAction::AlphaBoostApply => {
                ui.heading("⚠️ Terms of Service Acknowledgment");
                ui.label("Applying Alpha Boost modifications edits local game files.");
                ui.label("Under Psyonix / Rocket League Terms of Service (ToS), modifying game files or cosmetics can technically be considered a violation and carries a risk of account suspension.");
                ui.add_space(4.0);
                ui.checkbox(
                    &mut self.tos_accepted,
                    "I read, understand, and accept the risks associated with this action.",
                );
            }
            #[cfg(not(feature = "microsoft-store"))]
            ConfirmAction::AlphaBoostRestore => {
                ui.label("Are you sure you want to restore original Rocket League boost files?");
                ui.label("This will revert any local file modifications made by Alpha Boost.");
            }
            ConfirmAction::DeleteBackups => {
                ui.label(
                    "Are you sure you want to permanently delete all hoops replay backup (.replay.bak) files?",
                );
                ui.colored_label(
                    egui::Color32::from_rgb(230, 80, 80),
                    "⚠️ This action is irreversible!",
                );
            }
            ConfirmAction::ClearHistory => {
                ui.label("Are you sure you want to clear local player history?");
                ui.label("This deletes the SQLite history database contents for stored matches and players.");
            }
        }
    }

    fn perform_confirm_action(&mut self, action: ConfirmAction) {
        match action {
            ConfirmAction::ResetConfig => {
                let default_config = crate::state::Config::default();
                self.state.replace_config(default_config);
            }
            ConfirmAction::ClearUploadCache => {
                self.state
                    .update_config(|config| config.uploaded_replays.clear());
                if let Ok(mut status) = self.state.replays.ballchasing_status.lock() {
                    *status = "Upload cache cleared.".to_string();
                }
            }
            #[cfg(not(feature = "microsoft-store"))]
            ConfirmAction::AlphaBoostApply => {
                let rl_path = self.state.system.config.load().rocket_league_path.clone();
                crate::assets::start_apply_alpha_boost(self.state.clone(), rl_path);
            }
            #[cfg(not(feature = "microsoft-store"))]
            ConfirmAction::AlphaBoostRestore => {
                let rl_path = self.state.system.config.load().rocket_league_path.clone();
                crate::assets::start_restore_standard_boost(self.state.clone(), rl_path);
            }
            ConfirmAction::DeleteBackups => {
                crate::hoops_fixer::start_delete_backups_task(self.state.clone());
            }
            ConfirmAction::ClearHistory => match crate::history::clear_history(&self.state) {
                Ok(()) => {
                    self.state.history.revision.fetch_add(1, Ordering::SeqCst);
                    self.state
                        .history
                        .player_summaries
                        .store(Arc::new(Default::default()));
                    self.state
                        .history
                        .all_players_snapshot
                        .store(Arc::new(Default::default()));
                    self.state
                        .history
                        .totals
                        .store(Arc::new(crate::history::HistoryTotals::default()));
                    if let Ok(mut status) = self.state.history.status.lock() {
                        *status = "History cleared.".to_string();
                    }
                }
                Err(error) => {
                    if let Ok(mut status) = self.state.history.status.lock() {
                        *status = format!("History clear failed: {error}");
                    }
                }
            },
        }
    }
}
