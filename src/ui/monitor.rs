use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MonitorInfo {
    pub index: usize,
    pub position: egui::Pos2,
    pub size: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MonitorPlacement {
    pub position: egui::Pos2,
    pub size: [f32; 2],
    pub fullscreen: bool,
}

pub(crate) fn available_monitors(ctx: &egui::Context) -> Vec<MonitorInfo> {
    #[cfg(target_os = "windows")]
    {
        windows_monitors(ctx.pixels_per_point())
    }

    #[cfg(not(target_os = "windows"))]
    {
        fallback_monitors(ctx)
    }
}

pub(crate) fn dashboard_placement(
    ctx: &egui::Context,
    monitor_index: usize,
    fullscreen: bool,
) -> MonitorPlacement {
    let monitors = available_monitors(ctx);
    placement_for_monitors(&monitors, monitor_index, fullscreen)
}

pub(crate) fn monitor_summary(monitors: &[MonitorInfo], index: usize) -> String {
    if monitors.is_empty() {
        return "Monitor enumeration unavailable".to_string();
    }

    let selected = select_monitor(monitors, index);
    format!(
        "Monitor {}: {:.0}x{:.0} at {:.0},{:.0}",
        selected.index,
        selected.size[0],
        selected.size[1],
        selected.position.x,
        selected.position.y
    )
}

fn placement_for_monitors(
    monitors: &[MonitorInfo],
    monitor_index: usize,
    fullscreen: bool,
) -> MonitorPlacement {
    let monitor = select_monitor(monitors, monitor_index);
    if fullscreen {
        return MonitorPlacement {
            position: monitor.position,
            size: monitor.size,
            fullscreen: true,
        };
    }

    let size = clamp_window_size([1280.0, 720.0], monitor.size);
    let position = egui::pos2(
        monitor.position.x + ((monitor.size[0] - size[0]) / 2.0).max(0.0),
        monitor.position.y + ((monitor.size[1] - size[1]) / 2.0).max(0.0),
    );
    MonitorPlacement {
        position,
        size,
        fullscreen: false,
    }
}

fn select_monitor(monitors: &[MonitorInfo], monitor_index: usize) -> MonitorInfo {
    monitors
        .iter()
        .find(|monitor| monitor.index == monitor_index)
        .copied()
        .or_else(|| monitors.first().copied())
        .unwrap_or(MonitorInfo {
            index: 0,
            position: egui::Pos2::ZERO,
            size: [1280.0, 720.0],
        })
}

fn clamp_window_size(target: [f32; 2], monitor_size: [f32; 2]) -> [f32; 2] {
    [
        target[0].min(monitor_size[0].max(640.0)),
        target[1].min(monitor_size[1].max(480.0)),
    ]
}

#[cfg(not(target_os = "windows"))]
fn fallback_monitors(ctx: &egui::Context) -> Vec<MonitorInfo> {
    let size = ctx.input(|input| {
        input
            .viewport()
            .monitor_size
            .map(|size| [size.x, size.y])
            .or_else(|| {
                input.screen_rect().is_positive().then(|| {
                    let rect = input.screen_rect();
                    [rect.width(), rect.height()]
                })
            })
    });

    vec![MonitorInfo {
        index: 0,
        position: egui::Pos2::ZERO,
        size: size.unwrap_or([1280.0, 720.0]),
    }]
}

#[cfg(target_os = "windows")]
fn windows_monitors(pixels_per_point: f32) -> Vec<MonitorInfo> {
    use winapi::shared::minwindef::{BOOL, LPARAM, TRUE};
    use winapi::shared::windef::{HDC, HMONITOR, LPRECT};
    use winapi::um::winuser::{EnumDisplayMonitors, GetMonitorInfoW, MONITORINFO};

    struct MonitorEnumState {
        scale: f32,
        monitors: Vec<MonitorInfo>,
    }

    unsafe extern "system" fn enum_proc(
        monitor: HMONITOR,
        _hdc: HDC,
        _rect: LPRECT,
        data: LPARAM,
    ) -> BOOL {
        let state = unsafe { &mut *(data as *mut MonitorEnumState) };
        let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if unsafe { GetMonitorInfoW(monitor, &mut info as *mut MONITORINFO) } != 0 {
            let scale = state.scale.max(0.1);
            let left = info.rcMonitor.left as f32 / scale;
            let top = info.rcMonitor.top as f32 / scale;
            let width = (info.rcMonitor.right - info.rcMonitor.left) as f32 / scale;
            let height = (info.rcMonitor.bottom - info.rcMonitor.top) as f32 / scale;
            state.monitors.push(MonitorInfo {
                index: state.monitors.len(),
                position: egui::pos2(left, top),
                size: [width.max(1.0), height.max(1.0)],
            });
        }
        TRUE
    }

    let mut state = MonitorEnumState {
        scale: pixels_per_point,
        monitors: Vec::new(),
    };
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            Some(enum_proc),
            &mut state as *mut MonitorEnumState as LPARAM,
        );
    }
    state.monitors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(index: usize, x: f32, y: f32, w: f32, h: f32) -> MonitorInfo {
        MonitorInfo {
            index,
            position: egui::pos2(x, y),
            size: [w, h],
        }
    }

    #[test]
    fn placement_falls_back_when_no_monitors_exist() {
        let placement = placement_for_monitors(&[], 3, true);
        assert_eq!(placement.position, egui::Pos2::ZERO);
        assert_eq!(placement.size, [1280.0, 720.0]);
        assert!(placement.fullscreen);
    }

    #[test]
    fn placement_uses_selected_monitor() {
        let monitors = [
            monitor(0, 0.0, 0.0, 1920.0, 1080.0),
            monitor(1, 1920.0, 0.0, 2560.0, 1440.0),
        ];
        let placement = placement_for_monitors(&monitors, 1, true);
        assert_eq!(placement.position, egui::pos2(1920.0, 0.0));
        assert_eq!(placement.size, [2560.0, 1440.0]);
        assert!(placement.fullscreen);
    }

    #[test]
    fn placement_out_of_range_uses_primary_monitor() {
        let monitors = [
            monitor(0, 0.0, 0.0, 1920.0, 1080.0),
            monitor(1, 1920.0, 0.0, 2560.0, 1440.0),
        ];
        let placement = placement_for_monitors(&monitors, 99, true);
        assert_eq!(placement.position, egui::pos2(0.0, 0.0));
        assert_eq!(placement.size, [1920.0, 1080.0]);
    }

    #[test]
    fn windowed_placement_is_centered_and_clamped() {
        let monitors = [monitor(0, 100.0, 50.0, 1000.0, 600.0)];
        let placement = placement_for_monitors(&monitors, 0, false);
        assert_eq!(placement.size, [1000.0, 600.0]);
        assert_eq!(placement.position, egui::pos2(100.0, 50.0));
        assert!(!placement.fullscreen);
    }
}
