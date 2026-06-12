use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{ProcessRefreshKind, System};

const MAX_SAMPLES: usize = 300;
const MAX_FOCUS_EVENTS: usize = 80;
const MAX_PROCESS_SAMPLES: usize = 120;
const FOCUS_SAMPLE_INTERVAL: Duration = Duration::from_millis(100);
const PROCESS_SAMPLE_INTERVAL: Duration = Duration::from_secs(5);
const POLL_INTERVAL_MS: u64 = 250;
const BYTES_PER_MB: u64 = 1_048_576;
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
    pub top_processes: Vec<(String, f32)>,
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
        self.inner
            .lock()
            .map(|b| b.iter().cloned().collect())
            .unwrap_or_default()
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

fn is_easy_anticheat_process(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "easyanticheat.exe" || lower == "easyanticheat_eos.exe"
}

fn is_eos_process(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("eosoverlayrenderer")
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
            diag.push(("DWM Memory", format!("{} MB", bytes / BYTES_PER_MB)));
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
    resource_snapshots: &[ResourceSnapshot],
    frame_stats: &FrameStats,
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
    lines.push("[resource_timeline]".to_string());
    if resource_snapshots.is_empty() {
        lines.push("no resource snapshots recorded".to_string());
    } else {
        for s in resource_snapshots {
            let top = s
                .top_processes
                .iter()
                .map(|(name, cpu)| format!("{name}={cpu:.1}%"))
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!(
                "timestamp_ms={} rl_cpu={:.1}% rl_mem={}MB eac_cpu={:.1}% eac_mem={}MB sys_cpu={:.1}% top=[{}]",
                s.timestamp_ms, s.rl_cpu_usage, s.rl_memory_mb, s.eac_cpu_usage, s.eac_memory_mb, s.system_cpu_usage, top
            ));
        }
    }
    lines.push(String::new());

    lines.push("[frame_timing]".to_string());
    lines.push(format!(
        "stutter_threshold_ms={:.1}",
        frame_stats.stutter_threshold_ms
    ));
    lines.push(format!("total_frames={}", frame_stats.total_frames));
    lines.push(format!("stutter_count={}", frame_stats.stutter_count));
    lines.push(format!("avg_frame_ms={:.2}", frame_stats.avg_ms));
    lines.push(format!("min_frame_ms={:.2}", frame_stats.min_ms));
    lines.push(format!("max_frame_ms={:.2}", frame_stats.max_ms));
    lines.push(format!("last_frame_ms={:.2}", frame_stats.last_frame_ms));
    if frame_stats.recent_frames.is_empty() {
        lines.push("no frame samples recorded".to_string());
    } else {
        lines.push("recent_frames_ms=".to_string());
        for (i, ms) in frame_stats.recent_frames.iter().enumerate() {
            let marker = if *ms > frame_stats.stutter_threshold_ms {
                " STUTTER"
            } else {
                ""
            };
            lines.push(format!("  {i}: {:.2}{marker}", ms));
        }
    }

    std::fs::write(&path, lines.join("\n"))
        .map_err(|error| format!("Could not write diagnostics log: {error}"))?;
    Ok(path)
}

pub fn support_diagnostics_bundle(
    state: &crate::state::AppState,
    is_launched: bool,
    is_rl_running: bool,
    rl_process_detection_detail: &str,
) -> String {
    let config = state.system.config.load();
    let config_status = state.system.config_status.load();
    let diagnostics = state.system.network_diagnostics.load();
    let version = state.system.version_check.load();
    let local_identity = state.game.local_player_identity.load();
    let local_name = state.game.local_player_name.load();
    let local_team = state.game.local_team.load(Ordering::SeqCst);
    let players = state.game.players.load();
    let session = state.game.session.load();
    let capture = state.diagnostics.debug_capture_status.load();

    let mut lines = Vec::new();
    lines.push("RL Platform Overlay Support Diagnostics".to_string());
    lines.push(format!("generated_unix_ms={}", crate::stats_api::now_ms()));
    lines.push(format!("app_version={}", env!("CARGO_PKG_VERSION")));
    lines.push(format!("os={}", std::env::consts::OS));
    lines.push(format!("arch={}", std::env::consts::ARCH));
    lines.push(format!(
        "debug_tab_enabled={}",
        if state.debug_enabled { "true" } else { "false" }
    ));
    lines.push(format!(
        "debug_logging_enabled={}",
        if state.debug_logging_enabled.load(Ordering::SeqCst) {
            "true"
        } else {
            "false"
        }
    ));

    lines.push(String::new());
    lines.push("[config]".to_string());
    lines.push(format!("config_path={}", config_status.path));
    lines.push(format!(
        "config_status={}",
        if config_status.last_error.is_empty() {
            "OK"
        } else {
            config_status.last_error.as_str()
        }
    ));
    lines.push(format!(
        "rocket_league_path={}",
        empty_label(&config.rocket_league_path)
    ));
    lines.push(format!(
        "replays_folder={}",
        empty_label(&config.replays_folder)
    ));
    lines.push(format!(
        "ballchasing_enabled={}",
        config.ballchasing_enabled
    ));
    lines.push(format!(
        "ballchasing_api_key_present={}",
        !config.ballchasing_api_key.trim().is_empty()
    ));
    lines.push(format!("layout_mode={}", config.layout_mode));
    lines.push(format!("show_stats={}", config.show_stats));
    lines.push(format!("show_lobby_ranks={}", config.show_lobby_ranks));
    lines.push(format!(
        "show_teammate_boost={}",
        config.show_teammate_boost
    ));
    lines.push(format!(
        "session_overlay_enabled={}",
        config.session_overlay_enabled
    ));

    lines.push(String::new());
    lines.push("[runtime]".to_string());
    lines.push(format!("overlay_launched={is_launched}"));
    lines.push(format!(
        "hud_visible={}",
        state.flags.is_visible.load(Ordering::SeqCst)
    ));
    lines.push(format!(
        "settings_visible={}",
        state.flags.is_settings_visible.load(Ordering::SeqCst)
    ));
    lines.push(format!(
        "stats_api_connected={}",
        state.flags.is_connected.load(Ordering::SeqCst)
    ));
    lines.push(format!("rocket_league_running={is_rl_running}"));
    lines.push(format!(
        "rocket_league_detection_detail={}",
        empty_label(rl_process_detection_detail)
    ));

    lines.push(String::new());
    lines.push("[stats_api]".to_string());
    lines.push(format!("transport={}", diagnostics.transport.label()));
    lines.push(format!(
        "last_event={}",
        empty_label(&diagnostics.last_event)
    ));
    lines.push(format!(
        "last_event_unix_ms={}",
        diagnostics.last_event_unix_ms
    ));
    lines.push(format!(
        "last_event_rate_estimate={}",
        empty_label(&diagnostics.last_event_rate_estimate)
    ));
    lines.push(format!(
        "last_roster_signature_change_unix_ms={}",
        diagnostics.last_roster_signature_change_unix_ms
    ));
    lines.push(format!(
        "last_match_guid={}",
        empty_label(&diagnostics.last_match_guid)
    ));
    lines.push(format!(
        "last_result_signature={}",
        empty_label(&diagnostics.last_result_signature)
    ));
    lines.push(format!(
        "last_duplicate_result_suppression_reason={}",
        empty_label(&diagnostics.last_duplicate_result_suppression_reason)
    ));
    lines.push(format!(
        "last_parse_error={}",
        empty_label(&diagnostics.last_parse_error)
    ));
    lines.push(format!(
        "last_connection_error={}",
        empty_label(&diagnostics.last_connection_error)
    ));

    lines.push(String::new());
    lines.push("[local_player]".to_string());
    lines.push(format!("local_name={}", empty_label(local_name.as_str())));
    lines.push(format!(
        "identity_name={}",
        empty_label(&local_identity.name)
    ));
    lines.push(format!(
        "identity_platform={}",
        empty_label(&local_identity.platform)
    ));
    lines.push(format!(
        "identity_primary_id={}",
        empty_label(&local_identity.primary_id)
    ));
    lines.push(format!(
        "local_team={}",
        if local_team == crate::state::NO_TEAM {
            "Unknown".to_string()
        } else {
            local_team.to_string()
        }
    ));

    lines.push(String::new());
    lines.push("[session]".to_string());
    lines.push(format!(
        "active_match_id={}",
        empty_label(&session.active_match_id)
    ));
    lines.push(format!("active_mode={}", session.active_mode.label()));
    lines.push(format!("matches_played={}", session.matches_played));
    lines.push(format!("wins={}", session.wins));
    lines.push(format!("losses={}", session.losses));
    lines.push(format!("streak={}", session.streak));
    lines.push(format!("last_result={}", session.last_result.label()));
    lines.push(format!("blue_score={}", session.blue_score));
    lines.push(format!("orange_score={}", session.orange_score));
    lines.push(format!("round_started={}", session.round_started));
    if session.mode_records.is_empty() {
        lines.push("mode_records=none".to_string());
    } else {
        for (mode, record) in &session.mode_records {
            lines.push(format!(
                "mode_record={} wins={} losses={} matches={}",
                mode.label(),
                record.wins,
                record.losses,
                record.matches_played()
            ));
        }
    }

    lines.push(String::new());
    lines.push("[players]".to_string());
    lines.push(format!("count={}", players.len()));
    if players.is_empty() {
        lines.push("no players parsed".to_string());
    } else {
        let mut player_lines = players
            .values()
            .map(|player| {
                format!(
                    "name={} platform={} team={} local={} bot={} boost={} score={} goals={} saves={} touches={} demos={} mmr_loaded={}",
                    player.name,
                    player.platform,
                    player.team,
                    player.is_local,
                    player.is_bot,
                    player.boost,
                    player.score,
                    player.goals,
                    player.saves,
                    player.touches,
                    player.demos,
                    player.mmr.is_some()
                )
            })
            .collect::<Vec<_>>();
        player_lines.sort();
        lines.extend(player_lines);
    }

    lines.push(String::new());
    lines.push("[version_check]".to_string());
    lines.push(format!("checked={}", version.checked));
    lines.push(format!("update_available={}", version.update_available));
    lines.push(format!("latest_tag={}", empty_label(&version.latest_tag)));
    lines.push(format!("error={}", empty_label(&version.error)));

    lines.push(String::new());
    lines.push("[diagnostics]".to_string());
    lines.push(format!(
        "hotkey_log_path={}",
        crate::input::hotkey_debug_log_path().display()
    ));
    lines.push(format!("stats_api_capture_running={}", capture.running));
    lines.push(format!(
        "last_capture_output={}",
        empty_label(&capture.last_output_path)
    ));
    lines.push(format!(
        "last_capture_error={}",
        empty_label(&capture.error)
    ));
    lines.push(format!(
        "frame_tracker_enabled={}",
        state.diagnostics.frame_tracker.enabled()
    ));
    lines.push(format!(
        "foreground_tracker_enabled={}",
        state.diagnostics.foreground_tracker.enabled()
    ));
    let resource_poller_running = state
        .diagnostics
        .resource_poller
        .lock()
        .map(|poller| poller.is_running())
        .unwrap_or(false);
    lines.push(format!("resource_poller_running={resource_poller_running}"));
    lines.push(format!(
        "resource_snapshots={}",
        state.diagnostics.resource_tracker.get_snapshots().len()
    ));

    let upload_progress = state.replays.upload_progress.load();
    lines.push(String::new());
    lines.push("[replay_upload]".to_string());
    lines.push(format!("running={}", upload_progress.running));
    lines.push(format!("paused={}", upload_progress.paused));
    lines.push(format!("stop_requested={}", upload_progress.stop_requested));
    lines.push(format!("total={}", upload_progress.total));
    lines.push(format!("processed={}", upload_progress.processed));
    lines.push(format!("uploaded={}", upload_progress.uploaded));
    lines.push(format!("skipped={}", upload_progress.skipped));
    lines.push(format!("failed={}", upload_progress.failed));
    lines.push(format!(
        "current_file={}",
        empty_label(&upload_progress.current_file)
    ));
    lines.push(format!(
        "last_error={}",
        empty_label(&upload_progress.last_error)
    ));
    if upload_progress.recent_events.is_empty() {
        lines.push("recent_events=none".to_string());
    } else {
        for event in &upload_progress.recent_events {
            lines.push(format!("event={event}"));
        }
    }

    let system = system_diagnostics();
    lines.push(String::new());
    lines.push("[system]".to_string());
    if system.is_empty() {
        lines.push("no system diagnostics available".to_string());
    } else {
        for (label, value) in system {
            lines.push(format!("{label}={value}"));
        }
    }

    lines.push(String::new());
    lines.push("[recent_hotkey_log]".to_string());
    lines.extend(read_recent_lines(
        &crate::input::hotkey_debug_log_path(),
        40,
    ));

    lines.join("\n")
}

fn empty_label(value: &str) -> &str {
    if value.trim().is_empty() {
        "(empty)"
    } else {
        value
    }
}

fn read_recent_lines(path: &PathBuf, max_lines: usize) -> Vec<String> {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let mut lines = content
                .lines()
                .rev()
                .take(max_lines)
                .map(str::to_string)
                .collect::<Vec<_>>();
            lines.reverse();
            if lines.is_empty() {
                vec!["hotkey log is empty".to_string()]
            } else {
                lines
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            vec!["hotkey log file not found".to_string()]
        }
        Err(error) => vec![format!("could not read hotkey log: {error}")],
    }
}

pub struct ResourcePoller {
    tracker: Arc<ResourceTracker>,
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl ResourcePoller {
    pub fn new(tracker: Arc<ResourceTracker>) -> Self {
        Self {
            tracker,
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    pub fn start(&mut self) {
        if self.is_running() {
            return;
        }
        self.running.store(true, Ordering::Relaxed);

        let running_flag = self.running.clone();
        let tracker_ref = self.tracker.clone();

        self.handle = Some(thread::spawn(move || {
            let mut sys = System::new_all();
            let process_refresh_kind = ProcessRefreshKind::nothing().with_cpu();

            while running_flag.load(Ordering::Relaxed) {
                sys.refresh_cpu_usage();
                sys.refresh_processes_specifics(
                    sysinfo::ProcessesToUpdate::All,
                    true,
                    process_refresh_kind,
                );

                let mut rl_cpu = 0.0;
                let mut rl_mem = 0;
                let mut eac_cpu = 0.0;
                let mut eac_mem = 0;

                for process in sys.processes().values() {
                    let name = process.name().to_string_lossy();

                    if is_rocket_league_process(&name) {
                        rl_cpu += process.cpu_usage();
                        rl_mem += process.memory() / BYTES_PER_MB;
                    } else if is_easy_anticheat_process(&name) || is_eos_process(&name) {
                        eac_cpu += process.cpu_usage();
                        eac_mem += process.memory() / BYTES_PER_MB;
                    }
                }

                let mut top_processes: Vec<(String, f32)> = sys
                    .processes()
                    .values()
                    .filter(|p| p.cpu_usage() > 0.0)
                    .map(|p| (p.name().to_string_lossy().into_owned(), p.cpu_usage()))
                    .collect();
                top_processes
                    .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                top_processes.truncate(5);

                tracker_ref.add_snapshot(ResourceSnapshot {
                    timestamp_ms: crate::stats_api::now_ms(),
                    rl_cpu_usage: rl_cpu,
                    rl_memory_mb: rl_mem,
                    eac_cpu_usage: eac_cpu,
                    eac_memory_mb: eac_mem,
                    system_cpu_usage: sys.global_cpu_usage(),
                    top_processes,
                });

                thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
            }
        }));
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for ResourcePoller {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
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
            top_processes: vec![],
        };
        assert_eq!(snapshot.rl_memory_mb, 2048);
    }

    #[test]
    fn test_resource_tracker_buffer() {
        let tracker = super::ResourceTracker::new();
        tracker.add_snapshot(super::ResourceSnapshot {
            timestamp_ms: 100,
            rl_cpu_usage: 0.0,
            rl_memory_mb: 0,
            eac_cpu_usage: 0.0,
            eac_memory_mb: 0,
            system_cpu_usage: 0.0,
            top_processes: vec![],
        });
        let snapshots = tracker.get_snapshots();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].timestamp_ms, 100);
    }

    #[test]
    fn test_resource_tracker_buffer_eviction() {
        let tracker = super::ResourceTracker::new();
        for i in 0..(super::MAX_RESOURCE_SNAPSHOTS + 5) {
            tracker.add_snapshot(super::ResourceSnapshot {
                timestamp_ms: i as u128 * 100,
                rl_cpu_usage: 0.0,
                rl_memory_mb: 0,
                eac_cpu_usage: 0.0,
                eac_memory_mb: 0,
                system_cpu_usage: 0.0,
                top_processes: vec![],
            });
        }
        let snapshots = tracker.get_snapshots();
        assert_eq!(snapshots.len(), super::MAX_RESOURCE_SNAPSHOTS);
        assert_eq!(snapshots[0].timestamp_ms, 500);
        assert_eq!(
            snapshots.last().unwrap().timestamp_ms,
            (super::MAX_RESOURCE_SNAPSHOTS + 4) as u128 * 100
        );
    }

    #[test]
    fn test_poller_lifecycle() {
        let tracker = std::sync::Arc::new(super::ResourceTracker::new());
        let mut poller = super::ResourcePoller::new(tracker.clone());

        assert!(!poller.is_running());
        poller.start();
        assert!(poller.is_running());
        poller.stop();
        assert!(!poller.is_running());
    }
}
