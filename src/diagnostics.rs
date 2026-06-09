use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const MAX_SAMPLES: usize = 300;
const MAX_FOCUS_EVENTS: usize = 80;
const MAX_PROCESS_SAMPLES: usize = 120;
const FOCUS_SAMPLE_INTERVAL: Duration = Duration::from_millis(100);
const PROCESS_SAMPLE_INTERVAL: Duration = Duration::from_secs(5);
#[cfg(target_os = "windows")]
const SYSTEM_DIAGNOSTICS_CACHE_TTL: Duration = Duration::from_secs(5);

#[derive(Clone, Debug)]
pub struct FrameStats {
    pub last_frame_ms: f64,
    pub avg_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub stutter_count: usize,
    pub total_frames: u64,
    pub recent_frames: Vec<f64>,
    pub stutter_threshold_ms: f64,
}

pub struct FrameTimeTracker {
    last_frame: Instant,
    samples: VecDeque<f64>,
    stutter_threshold_ms: f64,
    stutter_count: usize,
    total_frames: u64,
    min_ms: f64,
    max_ms: f64,
}

impl FrameTimeTracker {
    pub fn new(target_fps: u32) -> Self {
        let target_frame_ms = 1000.0 / target_fps as f64;
        Self {
            last_frame: Instant::now(),
            samples: VecDeque::with_capacity(MAX_SAMPLES),
            stutter_threshold_ms: target_frame_ms * 2.5,
            stutter_count: 0,
            total_frames: 0,
            min_ms: f64::MAX,
            max_ms: 0.0,
        }
    }

    pub fn record_frame(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_frame);
        self.last_frame = now;

        let ms = elapsed.as_secs_f64() * 1000.0;

        if self.total_frames > 0 {
            if ms > self.stutter_threshold_ms {
                self.stutter_count += 1;
            }
            if ms < self.min_ms {
                self.min_ms = ms;
            }
            if ms > self.max_ms {
                self.max_ms = ms;
            }
        } else {
            self.min_ms = ms;
            self.max_ms = ms;
        }

        self.total_frames += 1;

        if self.samples.len() >= MAX_SAMPLES {
            self.samples.pop_front();
        }
        self.samples.push_back(ms);
    }

    pub fn stats(&self) -> FrameStats {
        let avg_ms = if self.samples.is_empty() {
            0.0
        } else {
            self.samples.iter().sum::<f64>() / self.samples.len() as f64
        };

        let last_frame_ms = self.samples.back().copied().unwrap_or(0.0);

        FrameStats {
            last_frame_ms,
            avg_ms,
            min_ms: if self.min_ms == f64::MAX {
                0.0
            } else {
                self.min_ms
            },
            max_ms: self.max_ms,
            stutter_count: self.stutter_count,
            total_frames: self.total_frames,
            recent_frames: self.samples.iter().copied().collect(),
            stutter_threshold_ms: self.stutter_threshold_ms,
        }
    }
}

pub struct SharedFrameTracker {
    inner: Mutex<FrameTimeTracker>,
    enabled: AtomicBool,
    target_fps: u32,
}

impl SharedFrameTracker {
    pub fn new(target_fps: u32) -> Self {
        Self {
            inner: Mutex::new(FrameTimeTracker::new(target_fps)),
            enabled: AtomicBool::new(false),
            target_fps,
        }
    }

    pub fn set_enabled(&self, on: bool) {
        let prev = self.enabled.swap(on, Ordering::Relaxed);
        if on
            && !prev
            && let Ok(mut tracker) = self.inner.lock()
        {
            *tracker = FrameTimeTracker::new(self.target_fps);
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn record_frame(&self) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        if let Ok(mut tracker) = self.inner.lock() {
            tracker.record_frame();
        }
    }

    pub fn stats(&self) -> FrameStats {
        self.inner.lock().map(|t| t.stats()).unwrap_or(FrameStats {
            last_frame_ms: 0.0,
            avg_ms: 0.0,
            min_ms: 0.0,
            max_ms: 0.0,
            stutter_count: 0,
            total_frames: 0,
            recent_frames: Vec::new(),
            stutter_threshold_ms: 1000.0 / self.target_fps as f64 * 2.5,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FocusEvent {
    pub elapsed_ms: u128,
    pub title: String,
    pub process_name: String,
    pub rocket_league_foreground: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessDiagnosticsSample {
    pub elapsed_ms: u128,
    pub entries: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ResourceSnapshot {
    pub timestamp_ms: u128,
    pub rl_cpu_usage: f32,
    pub rl_memory_mb: u64,
    pub eac_cpu_usage: f32,
    pub eac_memory_mb: u64,
    pub system_cpu_usage: f32,
}

const MAX_RESOURCE_SNAPSHOTS: usize = 40; // 10 seconds at 250ms polling

pub struct ResourceTracker {
    pub inner: Mutex<VecDeque<ResourceSnapshot>>,
}

impl ResourceTracker {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(VecDeque::with_capacity(MAX_RESOURCE_SNAPSHOTS)),
        }
    }

    pub fn add_snapshot(&self, snapshot: ResourceSnapshot) {
        if let Ok(mut buffer) = self.inner.lock() {
            if buffer.len() >= MAX_RESOURCE_SNAPSHOTS {
                buffer.pop_front();
            }
            buffer.push_back(snapshot);
        }
    }

    pub fn get_snapshots(&self) -> Vec<ResourceSnapshot> {
        self.inner.lock().map(|b| b.iter().cloned().collect()).unwrap_or_default()
    }
}

struct ForegroundSnapshot {
    title: String,
    process_name: String,
}

pub struct ForegroundTracker {
    enabled: AtomicBool,
    inner: Mutex<ForegroundTimeline>,
}

struct ForegroundTimeline {
    started: Instant,
    last_sample: Instant,
    last_title: Option<String>,
    last_process_name: Option<String>,
    last_rocket_league_foreground: Option<bool>,
    last_process_sample: Instant,
    events: VecDeque<FocusEvent>,
    process_samples: VecDeque<ProcessDiagnosticsSample>,
}

impl ForegroundTracker {
    pub fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            inner: Mutex::new(ForegroundTimeline::new()),
        }
    }

    pub fn set_enabled(&self, on: bool) {
        let prev = self.enabled.swap(on, Ordering::Relaxed);
        if on
            && !prev
            && let Ok(mut timeline) = self.inner.lock()
        {
            *timeline = ForegroundTimeline::new();
        }
    }

    pub fn record_sample(&self) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }

        let now = Instant::now();
        if let Ok(mut timeline) = self.inner.lock() {
            if now.duration_since(timeline.last_sample) < FOCUS_SAMPLE_INTERVAL {
                return;
            }
            timeline.last_sample = now;

            let snapshot = foreground_window_snapshot();
            let rocket_league_foreground = is_rocket_league_process(&snapshot.process_name);
            timeline.record(snapshot, rocket_league_foreground, now);

            if now.duration_since(timeline.last_process_sample) >= PROCESS_SAMPLE_INTERVAL {
                timeline.last_process_sample = now;
                let entries = system_diagnostics()
                    .into_iter()
                    .map(|(label, value)| format!("{label}={value}"))
                    .collect();
                timeline.record_process_sample(entries, now);
            }
        }
    }

    pub fn events(&self) -> Vec<FocusEvent> {
        self.inner
            .lock()
            .map(|timeline| timeline.events.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn process_samples(&self) -> Vec<ProcessDiagnosticsSample> {
        self.inner
            .lock()
            .map(|timeline| timeline.process_samples.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
}

impl Default for ForegroundTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ForegroundTimeline {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            started: now,
            last_sample: now.checked_sub(FOCUS_SAMPLE_INTERVAL).unwrap_or(now),
            last_title: None,
            last_process_name: None,
            last_rocket_league_foreground: None,
            last_process_sample: now.checked_sub(PROCESS_SAMPLE_INTERVAL).unwrap_or(now),
            events: VecDeque::with_capacity(MAX_FOCUS_EVENTS),
            process_samples: VecDeque::with_capacity(MAX_PROCESS_SAMPLES),
        }
    }

    fn record(
        &mut self,
        snapshot: ForegroundSnapshot,
        rocket_league_foreground: bool,
        now: Instant,
    ) {
        if self.last_title.as_deref() == Some(snapshot.title.as_str())
            && self.last_process_name.as_deref() == Some(snapshot.process_name.as_str())
            && self.last_rocket_league_foreground == Some(rocket_league_foreground)
        {
            return;
        }

        self.last_title = Some(snapshot.title.clone());
        self.last_process_name = Some(snapshot.process_name.clone());
        self.last_rocket_league_foreground = Some(rocket_league_foreground);
        if self.events.len() >= MAX_FOCUS_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(FocusEvent {
            elapsed_ms: now.duration_since(self.started).as_millis(),
            title: snapshot.title,
            process_name: snapshot.process_name,
            rocket_league_foreground,
        });
    }

    fn record_process_sample(&mut self, entries: Vec<String>, now: Instant) {
        if self.process_samples.len() >= MAX_PROCESS_SAMPLES {
            self.process_samples.pop_front();
        }
        self.process_samples.push_back(ProcessDiagnosticsSample {
            elapsed_ms: now.duration_since(self.started).as_millis(),
            entries,
        });
    }
}

fn is_rocket_league_process(process_name: &str) -> bool {
    process_name.eq_ignore_ascii_case("RocketLeague.exe")
}

#[cfg(target_os = "windows")]
fn foreground_window_snapshot() -> ForegroundSnapshot {
    use winapi::shared::minwindef::DWORD;
    use winapi::um::winuser::{
        GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    };

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return ForegroundSnapshot {
                title: "No foreground window".to_string(),
                process_name: "Unknown".to_string(),
            };
        }

        let len = GetWindowTextLengthW(hwnd);
        let title = if len <= 0 {
            "Untitled foreground window".to_string()
        } else {
            let mut buffer = vec![0u16; len as usize + 1];
            let copied = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
            if copied <= 0 {
                "Untitled foreground window".to_string()
            } else {
                String::from_utf16_lossy(&buffer[..copied as usize])
            }
        };

        let mut process_id: DWORD = 0;
        GetWindowThreadProcessId(hwnd, &mut process_id);
        let process_name = if process_id == 0 {
            "Unknown".to_string()
        } else {
            foreground_process_name(process_id)
        };

        ForegroundSnapshot {
            title,
            process_name,
        }
    }
}

#[cfg(target_os = "windows")]
unsafe fn foreground_process_name(process_id: winapi::shared::minwindef::DWORD) -> String {
    use winapi::shared::minwindef::{DWORD, FALSE};
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::OpenProcess;
    use winapi::um::winbase::QueryFullProcessImageNameW;
    use winapi::um::winnt::PROCESS_QUERY_LIMITED_INFORMATION;

    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, process_id) };
    if process.is_null() {
        return "Unknown".to_string();
    }

    let mut buffer = vec![0u16; 32768];
    let mut size = buffer.len() as DWORD;
    let ok = unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut size) };
    unsafe {
        CloseHandle(process);
    }
    if ok == 0 || size == 0 {
        return "Unknown".to_string();
    }

    let path = String::from_utf16_lossy(&buffer[..size as usize]);
    path.rsplit(['\\', '/'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("Unknown")
        .to_string()
}

#[cfg(not(target_os = "windows"))]
fn foreground_window_snapshot() -> ForegroundSnapshot {
    ForegroundSnapshot {
        title: "Foreground window detection is Windows-only".to_string(),
        process_name: "Unsupported".to_string(),
    }
}

#[cfg(target_os = "windows")]
pub fn system_diagnostics() -> Vec<(&'static str, String)> {
    use std::sync::{Mutex, OnceLock};

    static CACHE: OnceLock<Mutex<SystemDiagnosticsCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(SystemDiagnosticsCache::default()));

    if let Ok(mut cached) = cache.lock() {
        if cached.is_fresh() {
            return cached.diagnostics.clone();
        }

        if !cached.refreshing {
            cached.refreshing = true;
            std::thread::spawn(move || {
                let diagnostics = collect_system_diagnostics();
                if let Some(cache) = CACHE.get()
                    && let Ok(mut cached) = cache.lock()
                {
                    cached.recorded_at = Some(Instant::now());
                    cached.diagnostics = diagnostics;
                    cached.refreshing = false;
                }
            });
        }

        let mut diagnostics = cached.diagnostics.clone();
        diagnostics.push(("System Diagnostics", "Refreshing...".to_string()));
        return diagnostics;
    }

    vec![("System Diagnostics", "Unavailable".to_string())]
}

#[cfg(target_os = "windows")]
#[derive(Default)]
struct SystemDiagnosticsCache {
    recorded_at: Option<Instant>,
    diagnostics: Vec<(&'static str, String)>,
    refreshing: bool,
}

#[cfg(target_os = "windows")]
impl SystemDiagnosticsCache {
    fn is_fresh(&self) -> bool {
        self.recorded_at
            .is_some_and(|recorded_at| recorded_at.elapsed() < SYSTEM_DIAGNOSTICS_CACHE_TTL)
    }
}

#[cfg(target_os = "windows")]
fn collect_system_diagnostics() -> Vec<(&'static str, String)> {
    use std::process::Command;

    let mut diag = Vec::new();

    if let Ok(output) = Command::new("powershell")
        .args(["-NoProfile", "-Command", TARGET_PROCESS_DIAGNOSTICS_PS])
        .output()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        let mut found_process = false;
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            found_process = true;
            diag.push(("Target Process", line.to_string()));
        }
        if !found_process {
            diag.push(("Target Processes", "None detected".to_string()));
        }
    }

    if let Ok(output) = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-Process dwm | Select-Object -ExpandProperty WorkingSet64",
        ])
        .output()
    {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if let Ok(bytes) = text.parse::<u64>() {
            diag.push(("DWM Memory", format!("{} MB", bytes / 1024 / 1024)));
        }
    }

    diag
}

#[cfg(target_os = "windows")]
const TARGET_PROCESS_DIAGNOSTICS_PS: &str = r#"
$names = @(
    'RocketLeague',
    'EasyAntiCheat',
    'EasyAntiCheat_EOS',
    'EOSOverlayRenderer-Win64-Shipping',
    'EOSOverlayRenderer-Win32-Shipping'
)
Get-Process |
    Where-Object {
        $names -contains $_.ProcessName -or
        $_.ProcessName -like '*EasyAntiCheat*' -or
        $_.ProcessName -like '*EOS*'
    } |
    Sort-Object ProcessName, Id |
    ForEach-Object {
        $priority = 'Unavailable'
        $affinity = 'Unavailable'
        try { $priority = [string]$_.PriorityClass } catch {}
        try { $affinity = [string]$_.ProcessorAffinity } catch {}
        "$($_.ProcessName).exe pid=$($_.Id) priority=$priority affinity=$affinity"
    }
"#;

#[cfg(not(target_os = "windows"))]
pub fn system_diagnostics() -> Vec<(&'static str, String)> {
    vec![]
}

pub fn alt_tab_diagnostics_log_path() -> PathBuf {
    crate::state::config_dir()
        .map(|dir| dir.join("alt_tab_diagnostics.log"))
        .unwrap_or_else(|| PathBuf::from("alt_tab_diagnostics.log"))
}

pub fn write_alt_tab_diagnostics_log(
    events: &[FocusEvent],
    process_samples: &[ProcessDiagnosticsSample],
    system_diagnostics: &[(&'static str, String)],
) -> Result<PathBuf, String> {
    let path = alt_tab_diagnostics_log_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create diagnostics log directory: {error}"))?;
    }

    let mut lines = Vec::new();
    lines.push(format!("captured_unix_ms={}", crate::stats_api::now_ms()));
    lines.push(String::new());
    lines.push("[foreground_timeline]".to_string());
    if events.is_empty() {
        lines.push("no foreground-window changes recorded".to_string());
    } else {
        for event in events {
            let status = if event.rocket_league_foreground {
                "rl_foreground"
            } else {
                "rl_unfocused"
            };
            lines.push(format!(
                "elapsed_ms={} status={} process={} title={}",
                event.elapsed_ms, status, event.process_name, event.title
            ));
        }
    }

    lines.push(String::new());
    lines.push("[process_samples]".to_string());
    if process_samples.is_empty() {
        lines.push("no process priority/affinity samples recorded".to_string());
    } else {
        for sample in process_samples {
            lines.push(format!("elapsed_ms={}", sample.elapsed_ms));
            if sample.entries.is_empty() {
                lines.push("  no process diagnostics available".to_string());
            } else {
                for entry in &sample.entries {
                    lines.push(format!("  {entry}"));
                }
            }
        }
    }

    lines.push(String::new());
    lines.push("[system]".to_string());
    if system_diagnostics.is_empty() {
        lines.push("no system diagnostics available".to_string());
    } else {
        for (label, value) in system_diagnostics {
            lines.push(format!("{label}={value}"));
        }
    }
    lines.push(String::new());

    std::fs::write(&path, lines.join("\n"))
        .map_err(|error| format!("Could not write diagnostics log: {error}"))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn frame_tracker_records_and_detects_stutters() {
        let mut tracker = FrameTimeTracker::new(60);
        // Simulate 10 normal frames
        for _ in 0..10 {
            tracker.record_frame();
            std::thread::sleep(Duration::from_millis(1));
        }
        let stats = tracker.stats();
        assert_eq!(stats.total_frames, 10);
        assert_eq!(stats.stutter_count, 0);
    }

    #[test]
    fn frame_tracker_detects_stutter() {
        let mut tracker = FrameTimeTracker::new(60);
        tracker.record_frame();
        // Simulate a long frame (>41ms = stutter at 60fps with 2.5x threshold)
        std::thread::sleep(Duration::from_millis(50));
        tracker.record_frame();
        let stats = tracker.stats();
        assert_eq!(stats.stutter_count, 1);
    }

    #[test]
    fn shared_tracker_works() {
        let tracker = SharedFrameTracker::new(60);
        tracker.set_enabled(true);
        tracker.record_frame();
        std::thread::sleep(Duration::from_millis(1));
        tracker.record_frame();
        let stats = tracker.stats();
        assert_eq!(stats.total_frames, 2);
    }

    #[test]
    fn test_resource_snapshot_creation() {
        let snapshot = super::ResourceSnapshot {
            timestamp_ms: 12345,
            rl_cpu_usage: 15.5,
            rl_memory_mb: 2048,
            eac_cpu_usage: 1.2,
            eac_memory_mb: 50,
            system_cpu_usage: 25.0,
        };
        assert_eq!(snapshot.rl_memory_mb, 2048);
    }

    #[test]
    fn test_resource_tracker_buffer() {
        let tracker = super::ResourceTracker::new();
        tracker.add_snapshot(super::ResourceSnapshot {
            timestamp_ms: 100, rl_cpu_usage: 0.0, rl_memory_mb: 0, eac_cpu_usage: 0.0, eac_memory_mb: 0, system_cpu_usage: 0.0,
        });
        let snapshots = tracker.get_snapshots();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].timestamp_ms, 100);
    }
}
