use crate::state::AppState;
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use super::boost_hud::{render_teammate_boost, render_teammate_boost_position_preview};
use super::debug::render_debug_settings_tab;
use super::hotkeys::egui_to_rdev_key;
use super::lobby_overlay::render_overlay;
use super::session_hud::render_session_overlay;
use super::settings::{
    render_boost_settings_tab, render_overlay_settings_tab, render_session_settings_tab,
    render_settings_tabs, render_setup_settings_tab, render_update_notice,
};

pub struct MainApp {
    state: Arc<AppState>,
    settings_tab: SettingsTab,
    is_rl_running: bool,
    rl_process_detection_detail: String,
    last_rl_check: std::time::Instant,
    last_logged_show_settings: Option<bool>,
    last_viewport_state: Option<(bool, bool, bool, bool, [f32; 2])>,
    hwnd: Option<isize>,
    rocket_league_process_watcher: crate::assets::RocketLeagueProcessWatcher,
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
            hwnd,
            rocket_league_process_watcher: crate::assets::RocketLeagueProcessWatcher::new(),
        }
    }

    fn refresh_rocket_league_process_detection(&mut self) {
        let now = std::time::Instant::now();
        if self.is_rl_running || now.duration_since(self.last_rl_check).as_secs() < 2 {
            return;
        }

        let detection = self.rocket_league_process_watcher.detect();
        self.is_rl_running = detection.running;
        self.rl_process_detection_detail = detection.detail;
        self.last_rl_check = now;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SettingsTab {
    Setup,
    Overlay,
    Session,
    Boost,
    Debug,
}

impl eframe::App for MainApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let is_launched = self.state.is_launched.load(Ordering::SeqCst);
        if is_launched {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            [0.12, 0.12, 0.12, 1.0]
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let is_launched = self.state.is_launched.load(Ordering::SeqCst);
        let config = self.state.config.load();
        let show_settings = self.state.is_settings_visible.load(Ordering::SeqCst)
            || self.state.is_recording_kb.load(Ordering::SeqCst)
            || self.state.is_recording_ctrl.load(Ordering::SeqCst)
            || self.state.is_recording_settings.load(Ordering::SeqCst);
        let show_hud =
            is_launched && (self.state.is_visible.load(Ordering::SeqCst) || config.layout_mode);
        let show_session_overlay = is_launched && config.session_overlay_enabled;
        let show_boost_position_preview =
            (is_launched && config.show_teammate_boost && config.layout_mode)
                || (show_settings
                    && self.settings_tab == SettingsTab::Boost
                    && config.show_teammate_boost);
        let show_boost_hud =
            is_launched && config.show_teammate_boost && !show_settings && !config.layout_mode;
        let mouse_passthrough = is_launched && !show_settings && !config.layout_mode;
        let maximized = is_launched;
        let window_size = if is_launched {
            config.window_size
        } else {
            [720.0, 820.0]
        };

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
            }
        }

        let viewport_state = (
            is_launched,
            show_settings,
            mouse_passthrough,
            maximized,
            window_size,
        );
        if self.last_viewport_state != Some(viewport_state) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(maximized));
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(mouse_passthrough));
            if !maximized {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(window_size.into()));
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
                        crate::input::append_hotkey_debug_log(format!(
                            "ui_show_settings visible={show_settings} launched={is_launched} recording_kb={} recording_ctrl={} recording_settings={}",
                            self.state.is_recording_kb.load(Ordering::SeqCst),
                            self.state.is_recording_ctrl.load(Ordering::SeqCst),
                            self.state.is_recording_settings.load(Ordering::SeqCst)
                        ));
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
                                            "gear_settings_button_clicked visible=true",
                                        );
                                        self.state.is_settings_visible.store(true, Ordering::SeqCst);
                                    }
                                });
                        }
                    }

                    let settings_hotkey = config.hotkey_settings.clone();
                    let hud_hotkey = config.hotkey_kb.clone();
                    let hotkey_toggle = config.hotkey_toggle;
                    ctx.input(|i| {
                        for event in &i.events {
                            if let egui::Event::Key { key, pressed, .. } = event
                                && let Some(name) = egui_to_rdev_key(*key)
                            {
                                if *pressed && name == settings_hotkey {
                                    crate::input::append_hotkey_debug_log(format!(
                                        "egui_keypress key={name} settings_match=true"
                                    ));
                                    crate::input::toggle_settings_hotkey(&self.state, "egui");
                                }

                                if show_settings && name == hud_hotkey {
                                    if hotkey_toggle {
                                        if *pressed {
                                            let curr = self.state.is_visible.load(Ordering::SeqCst);
                                            self.state.is_visible.store(!curr, Ordering::SeqCst);
                                        }
                                    } else {
                                        self.state.is_visible.store(*pressed, Ordering::SeqCst);
                                    }
                                }
                            }
                        }
                    });

                    // Float settings Window over overlay
                    if show_settings {
                        let mut settings_open = true;
                        egui::Window::new("RL Overlay Settings")
                            .collapsible(true)
                            .resizable(true)
                            .movable(true)
                            .default_pos([16.0, 16.0])
                            .default_size([450.0, 600.0])
                            .min_width(420.0)
                            .min_height(520.0)
                            .constrain_to(ctx.screen_rect().shrink(8.0))
                            .open(&mut settings_open)
                            .show(ctx, |ui| {
                                self.render_settings_content(ui, ctx, is_launched);
                            });
                        if !settings_open {
                            crate::input::append_hotkey_debug_log(
                                "settings_window_close_clicked visible=false",
                            );
                            self.state
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
                    let title_bar_rect = ui.max_rect();
                    let drag_id = ui.id().with("title_bar_drag");
                    let drag_response = ui.interact(title_bar_rect, drag_id, egui::Sense::drag());

                    if drag_response.is_pointer_button_down_on() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }

                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("⚙  RL Platform Overlay")
                                .strong()
                                .color(egui::Color32::from_rgb(220, 220, 230))
                                .size(13.0),
                        );

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let close_resp = ui.add(
                                egui::Button::new(
                                    egui::RichText::new("🗙")
                                        .color(egui::Color32::from_rgb(180, 180, 190)),
                                )
                                .frame(false),
                            );
                            if close_resp.clicked() {
                                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                            }

                            let min_resp = ui.add(
                                egui::Button::new(
                                    egui::RichText::new("🗕")
                                        .color(egui::Color32::from_rgb(180, 180, 190)),
                                )
                                .frame(false),
                            );
                            if min_resp.clicked() {
                                ui.ctx()
                                    .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                            }
                        });
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
                                self.state.is_settings_visible.store(true, Ordering::SeqCst);
                            }
                            ui.add_space(10.0);
                            if ui
                                .button(egui::RichText::new("Launch Overlay").heading())
                                .clicked()
                            {
                                self.state.is_launched.store(true, Ordering::SeqCst);
                                self.state
                                    .is_settings_visible
                                    .store(false, Ordering::SeqCst);
                            }
                            ui.add_space(20.0);
                            if ui.button("Quit").clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        });
                    });
            }
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
                layout_mode: config.layout_mode,
            },
        );
    }
}

struct RepaintInputs {
    is_launched: bool,
    show_settings: bool,
    show_hud: bool,
    show_session_overlay: bool,
    show_boost_panel: bool,
    layout_mode: bool,
}

fn schedule_repaint(ctx: &egui::Context, state: &Arc<AppState>, inputs: RepaintInputs) {
    let has_drag_input = inputs.layout_mode && ctx.input(|input| input.pointer.any_down());
    let has_spinner = state.local_mmr.load().fetching
        || state.debug_capture_status.load().running
        || boost_operation_running(state);
    let needs_animation = inputs.show_hud
        || inputs.show_session_overlay
        || inputs.show_boost_panel
        || has_drag_input
        || has_spinner;

    let delay = if needs_animation {
        Duration::from_millis(16)
    } else if inputs.is_launched {
        Duration::from_millis(100)
    } else if inputs.show_settings {
        Duration::from_millis(250)
    } else {
        Duration::from_millis(1000)
    };
    ctx.request_repaint_after(delay);
}

fn boost_operation_running(state: &Arc<AppState>) -> bool {
    let status = state.boost_swap_status.lock().unwrap();
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

impl MainApp {
    fn render_settings_content(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        is_launched: bool,
    ) {
        ui.add_space(5.0);

        let config = self.state.config.load();
        let mut config_edit = (**config).clone();
        let mut changed = false;
        if !self.state.debug_enabled && self.settings_tab == SettingsTab::Debug {
            self.settings_tab = SettingsTab::Setup;
        }

        render_update_notice(ui, &self.state);
        render_settings_tabs(ui, &mut self.settings_tab, self.state.debug_enabled);
        self.refresh_rocket_league_process_detection();

        egui::ScrollArea::vertical().show(ui, |ui| match self.settings_tab {
            SettingsTab::Setup => render_setup_settings_tab(
                ui,
                &self.state,
                &mut config_edit,
                &mut changed,
                self.is_rl_running,
            ),
            SettingsTab::Overlay => render_overlay_settings_tab(
                ui,
                ctx,
                &self.state,
                &config,
                &mut config_edit,
                &mut changed,
                is_launched,
            ),
            SettingsTab::Session => {
                render_session_settings_tab(ui, &self.state, &mut config_edit, &mut changed)
            }
            SettingsTab::Boost => render_boost_settings_tab(
                ui,
                &self.state,
                &mut config_edit,
                &mut changed,
                self.is_rl_running,
            ),
            SettingsTab::Debug => render_debug_settings_tab(
                ui,
                &self.state,
                is_launched,
                self.is_rl_running,
                &self.rl_process_detection_detail,
            ),
        });

        if changed {
            self.state.save_config(config_edit);
        }
    }
}
