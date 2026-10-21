use eframe::egui;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

pub(crate) fn dashboard_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("second_monitor_dashboard")
}

#[derive(Default)]
pub(crate) struct UiRepaintSignal {
    // The app owns the strong handle. Keeping only a weak handle here avoids
    // a cycle through deferred viewport callbacks that capture AppState.
    context: Mutex<Weak<egui::Context>>,
}

impl UiRepaintSignal {
    pub(crate) fn bind(&self, context: &egui::Context) -> Arc<egui::Context> {
        let handle = Arc::new(context.clone());
        *self.context.lock().unwrap_or_else(|e| e.into_inner()) = Arc::downgrade(&handle);
        handle
    }

    pub(crate) fn request(&self, root_delay: Option<Duration>, dashboard_delay: Option<Duration>) {
        let context = self
            .context
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .upgrade();
        if let Some(context) = context {
            if let Some(delay) = root_delay {
                context.request_repaint_after_for(delay, egui::ViewportId::ROOT);
            }
            if let Some(delay) = dashboard_delay {
                context.request_repaint_after_for(delay, dashboard_viewport_id());
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn observe_repaints(
    context: &egui::Context,
) -> Arc<Mutex<Vec<egui::RequestRepaintInfo>>> {
    // Consume egui's initial settling frames before observing background wakeups.
    for _ in 0..3 {
        let mut output = context.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
    }
    let requests = Arc::new(Mutex::new(Vec::new()));
    let observed = requests.clone();
    context.set_request_repaint_callback(move |info| observed.lock().unwrap().push(info));
    requests
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_requests_target_viewports_and_coalesce_live_dashboard_updates() {
        let context = egui::Context::default();
        let signal = UiRepaintSignal::default();
        let handle = signal.bind(&context);
        let requests = observe_repaints(&context);
        for _ in 0..30 {
            signal.request(None, Some(Duration::from_millis(100)));
        }
        let live = requests.lock().unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].viewport_id, dashboard_viewport_id());
        assert!(live[0].delay > Duration::ZERO);
        assert!(live[0].delay <= Duration::from_millis(100));
        drop(live);
        signal.request(Some(Duration::ZERO), None);
        assert!(
            requests
                .lock()
                .unwrap()
                .iter()
                .any(|info| info.viewport_id == egui::ViewportId::ROOT
                    && info.delay == Duration::ZERO)
        );

        let requests = observe_repaints(&context);
        drop(handle);
        signal.request(Some(Duration::ZERO), Some(Duration::ZERO));
        assert!(requests.lock().unwrap().is_empty());
    }
}
