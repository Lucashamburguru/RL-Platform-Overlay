# Performance Diagnostics Enhancement Design

## Overview
Expand the existing diagnostics system to correlate Rocket League and Easy Anti-Cheat (EAC) resource usage with frame stutters. The goal is to identify if CPU spikes, disk I/O, or GPU bottlenecks coincide with dropped frames, without adding overhead to the overlay itself.

## Constraints
- **Zero Overhead:** The diagnostics must not cause stutters.
- **Opt-In:** Must be strictly optional and toggled via the Debug UI.

## Architecture

### 1. Asynchronous Background Poller
To prevent blocking the main UI or network threads, system metrics will be gathered on a dedicated, low-priority background thread.
- **Polling Rate:** Every 250ms.
- **Storage:** A fixed-size circular buffer (e.g., retaining the last 5-10 seconds of data).
- **Libraries:** Will utilize the existing `sysinfo` crate for cross-platform process metrics, falling back to minimal Windows API calls if specific GPU/IO data is missing.

### 2. Monitored Metrics
The background thread will record snapshots containing:
- **Per-Process (`RocketLeague.exe`, `EasyAntiCheat_EOS.exe`):**
  - CPU Usage (%)
  - Memory Usage (MB)
  - Disk Read/Write Bytes/sec
- **Global:**
  - Overall CPU Utilization (%)
  - Overall GPU Utilization (%) (if accessible without heavy dependencies)

### 3. The "Stutter Trigger" Mechanism
When the existing `FrameTimeTracker` records a frame duration exceeding the threshold (e.g., > 2.5x target frame time):
1. It records the timestamp of the stutter.
2. The UI/Logging system can then extract the historical metrics from the background thread's circular buffer corresponding to the time immediately *before* and *during* the stutter.
3. This "Stutter Snapshot" is appended to the `alt_tab_diagnostics.log` (or a new dedicated log file) when the user clicks "Save Diagnostics".

## UI Integration
- A toggle in the Debug menu to "Enable Advanced Resource Tracking".
- When enabled, the background polling thread is spawned.
- When disabled, the thread is terminated and memory is freed.
