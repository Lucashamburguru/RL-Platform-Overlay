# Performance Diagnostics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement asynchronous background polling of CPU, RAM, and Disk I/O metrics for Rocket League and EAC, tying them to existing stutter detection for advanced diagnostics.

**Architecture:** We will extend the existing `src/diagnostics.rs` module. A new `ResourcePoller` struct will manage a background thread that wakes every 250ms, uses the `sysinfo` crate to gather process metrics, and stores them in a fixed-size circular buffer. When the `FrameTimeTracker` detects a stutter, or when the user saves the diagnostics log, these historical resource snapshots will be appended to the log. The background polling will be toggleable from the `src/ui/debug.rs` menu.

**Tech Stack:** Rust, `sysinfo`, `std::sync::Mutex`, `std::thread`.

---

### Task 1: Define the Data Structures

**Files:**
- Modify: `src/diagnostics.rs`

- [ ] **Step 1: Write the failing test for data structures**

```rust
// Add to `mod tests` in src/diagnostics.rs
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_resource_snapshot_creation`
Expected: FAIL due to missing `ResourceSnapshot`.

- [ ] **Step 3: Write minimal implementation**

```rust
// Add to src/diagnostics.rs
#[derive(Clone, Debug)]
pub struct ResourceSnapshot {
    pub timestamp_ms: u128,
    pub rl_cpu_usage: f32,
    pub rl_memory_mb: u64,
    pub eac_cpu_usage: f32,
    pub eac_memory_mb: u64,
    pub system_cpu_usage: f32,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test test_resource_snapshot_creation`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/diagnostics.rs
git commit -m "feat(diagnostics): add ResourceSnapshot struct"
```

---

### Task 2: Implement the Circular Buffer and Tracker

**Files:**
- Modify: `src/diagnostics.rs`

- [ ] **Step 1: Write the failing test for `ResourceTracker`**

```rust
// Add to `mod tests` in src/diagnostics.rs
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_resource_tracker_buffer`
Expected: FAIL due to missing `ResourceTracker`.

- [ ] **Step 3: Write minimal implementation**

```rust
// Add to src/diagnostics.rs
use std::collections::VecDeque;

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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test test_resource_tracker_buffer`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/diagnostics.rs
git commit -m "feat(diagnostics): add ResourceTracker circular buffer"
```

---

### Task 3: Implement Background Polling Logic

**Files:**
- Modify: `src/diagnostics.rs`

- [ ] **Step 1: Write the failing test for the poller lifecycle**

```rust
// Add to `mod tests` in src/diagnostics.rs
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_poller_lifecycle`
Expected: FAIL due to missing `ResourcePoller`.

- [ ] **Step 3: Write minimal implementation**

```rust
// Add to src/diagnostics.rs
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use sysinfo::{System, ProcessRefreshKind, UpdateKind};

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
        if self.is_running() { return; }
        self.running.store(true, Ordering::Relaxed);
        
        let running_flag = self.running.clone();
        let tracker_ref = self.tracker.clone();
        
        self.handle = Some(thread::spawn(move || {
            let mut sys = System::new_all();
            let process_refresh_kind = ProcessRefreshKind::new().with_cpu();
            
            while running_flag.load(Ordering::Relaxed) {
                sys.refresh_cpu_usage();
                sys.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, true, process_refresh_kind);
                
                let mut rl_cpu = 0.0;
                let mut rl_mem = 0;
                let mut eac_cpu = 0.0;
                let mut eac_mem = 0;
                
                for process in sys.processes().values() {
                    let name = process.name().to_string_lossy().to_lowercase();
                    if name.contains("rocketleague") {
                        rl_cpu = process.cpu_usage();
                        rl_mem = process.memory() / 1024 / 1024;
                    } else if name.contains("easyanticheat") || name.contains("eos") {
                        eac_cpu = process.cpu_usage();
                        eac_mem = process.memory() / 1024 / 1024;
                    }
                }
                
                tracker_ref.add_snapshot(ResourceSnapshot {
                    timestamp_ms: crate::stats_api::now_ms(), // Need to make sure stats_api::now_ms is available or use a local one
                    rl_cpu_usage: rl_cpu,
                    rl_memory_mb: rl_mem,
                    eac_cpu_usage: eac_cpu,
                    eac_memory_mb: eac_mem,
                    system_cpu_usage: sys.global_cpu_usage(),
                });
                
                thread::sleep(Duration::from_millis(250));
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test test_poller_lifecycle`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/diagnostics.rs
git commit -m "feat(diagnostics): implement background ResourcePoller"
```

---

### Task 4: Integrate into AppState

**Files:**
- Modify: `src/state.rs`

- [ ] **Step 1: Add to `AppState`**
```rust
// In src/state.rs inside AppState struct definition
pub resource_tracker: Arc<crate::diagnostics::ResourceTracker>,
pub resource_poller: Arc<std::sync::Mutex<crate::diagnostics::ResourcePoller>>,
```

- [ ] **Step 2: Initialize in AppState::new()**
```rust
// In src/state.rs inside AppState::new
let resource_tracker = Arc::new(crate::diagnostics::ResourceTracker::new());
let resource_poller = Arc::new(std::sync::Mutex::new(crate::diagnostics::ResourcePoller::new(resource_tracker.clone())));

// And append to the returned AppState struct:
resource_tracker,
resource_poller,
```

- [ ] **Step 3: Run project compilation to verify**
Run: `cargo check`

- [ ] **Step 4: Commit**
```bash
git add src/state.rs
git commit -m "feat(state): add resource tracker and poller to AppState"
```

---

### Task 5: Add UI Toggle and Update Log Output

**Files:**
- Modify: `src/ui/debug.rs`
- Modify: `src/diagnostics.rs`

- [ ] **Step 1: Add UI Toggle**
In `src/ui/debug.rs`, within `render_performance_diagnostics`, add a button for "Enable Advanced Resource Tracking". 
```rust
let is_polling = state.resource_poller.lock().map(|p| p.is_running()).unwrap_or(false);
if ui.button(if is_polling { "Stop Resource Polling" } else { "Start Resource Polling" }).clicked() {
    if let Ok(mut poller) = state.resource_poller.lock() {
        if is_polling { poller.stop(); } else { poller.start(); }
    }
}
```

- [ ] **Step 2: Update Log Writing**
In `src/diagnostics.rs`, modify `write_alt_tab_diagnostics_log`:
```rust
pub fn write_alt_tab_diagnostics_log(
    events: &[FocusEvent],
    system_diagnostics: &[(&'static str, String)],
    resource_snapshots: &[ResourceSnapshot], // ADDED PARAMETER
) -> Result<PathBuf, String> {
// ...
    lines.push(String::new());
    lines.push("[resource_timeline]".to_string());
    if resource_snapshots.is_empty() {
        lines.push("no resource snapshots recorded".to_string());
    } else {
        for s in resource_snapshots {
            lines.push(format!(
                "timestamp_ms={} rl_cpu={:.1}% rl_mem={}MB eac_cpu={:.1}% eac_mem={}MB sys_cpu={:.1}%",
                s.timestamp_ms, s.rl_cpu_usage, s.rl_memory_mb, s.eac_cpu_usage, s.eac_memory_mb, s.system_cpu_usage
            ));
        }
    }
```

- [ ] **Step 3: Update Log Caller**
In `src/ui/debug.rs`, update the call to `write_alt_tab_diagnostics_log`:
```rust
match crate::diagnostics::write_alt_tab_diagnostics_log(&events, &system_diagnostics, &state.resource_tracker.get_snapshots()) {
```

- [ ] **Step 4: Compile and test**
Run: `cargo check`

- [ ] **Step 5: Commit**
```bash
git add src/ui/debug.rs src/diagnostics.rs
git commit -m "feat(ui): integrate resource polling into debug menu and log output"
```

EOF
