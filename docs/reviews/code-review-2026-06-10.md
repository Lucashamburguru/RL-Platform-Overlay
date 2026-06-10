# Code Review — RL-Platform-Overlay

**Date:** 2026-06-10  
**Reviewer:** Antigravity (AI-assisted)  
**Scope:** Full codebase (`src/` — ~13 000 LOC across 26 Rust source files)  
**Version:** 0.1.27 (Cargo.toml)

---

## Executive Summary

The project is a well-structured Rocket League real-time overlay built in Rust with `eframe`/`egui`. It connects to the in-game Stats API (WebSocket or TCP), fetches MMR from tracker.gg, tracks session win/loss, uploads replays to ballchasing.com, and renders multiple HUD elements as a transparent overlay.

Overall code quality is **solid for an indie tool** — clean module boundaries, good test coverage on core logic, and thoughtful handling of platform quirks. The main areas for improvement are around **state management ergonomics**, **error surface hardening**, and **reducing duplicated patterns**.

---

## Table of Contents

1. [Architecture & Module Design](#1-architecture--module-design)
2. [Concurrency & Thread Safety](#2-concurrency--thread-safety)
3. [Error Handling](#3-error-handling)
4. [Code Duplication & DRY](#4-code-duplication--dry)
5. [Testing](#5-testing)
6. [Security & Privacy](#6-security--privacy)
7. [Performance](#7-performance)
8. [Maintainability & Style](#8-maintainability--style)
9. [Specific File Notes](#9-specific-file-notes)
10. [Prioritized Recommendations](#10-prioritized-recommendations)

---

## 1. Architecture & Module Design

### Strengths
- Clean separation of concerns: `network`, `mmr`, `session`, `replays`, `input`, `ui`, `diagnostics`, `setup` are each focused modules.
- The `AppState` struct acts as a central shared state with `ArcSwap` for lock-free reads — a good pattern for a UI app with background tasks.
- Platform-specific code is properly gated behind `#[cfg(target_os = "windows")]`.

### Concerns

> [!IMPORTANT]
> **God-object state.** `AppState` has **40+ fields** spanning config, diagnostics, network, session, boost, replays, and debug. Consider grouping related fields into sub-structs (e.g. `DiagnosticsState`, `ReplayState`, `InputState`) to improve discoverability and make it clearer which subsystems own which state.

- `lib.rs` `run()` spawns multiple fire-and-forget tasks (`network`, `mmr`, `input`, `update`, `replays`) with no centralized shutdown mechanism. If the UI closes, background tasks are orphaned until the process exits. For now this works fine, but a `CancellationToken` pattern would be cleaner.

- Both `ui.rs` (the module re-export file) and `ui/app.rs` exist. The `ui.rs` file is just `mod app; mod boost_hud; ...` — this is fine, but the module is by far the largest subsystem (~185K across 10 files) with `settings.rs` alone at 46 KB. Some splitting of `settings.rs` by tab would help.

---

## 2. Concurrency & Thread Safety

### Strengths
- `ArcSwap` is used correctly throughout for snapshotting reads in the UI and atomic stores from background tasks.
- Atomics for simple flags (`is_visible`, `is_connected`, etc.) are appropriate.
- Mutex usage is limited to truly-mutable shared structures (logs, status strings, caches).

### Concerns

> [!WARNING]
> **Clone-modify-store pattern creates TOCTOU windows.** Multiple places do:
> ```rust
> let mut players_map = (**state.players.load()).clone();
> // modify...
> state.players.store(Arc::new(players_map));
> ```
> If two tasks do this concurrently, one's changes are silently lost. In practice this is unlikely since only `handle_update_state` and the MMR fetch task write to `players`, but it's fragile. Consider using `ArcSwap::rcu()` for read-copy-update semantics, or documenting the single-writer invariant.

- `std::sync::Mutex` is used (not `tokio::sync::Mutex`) in async contexts via `state.ballchasing_status.lock()`, `boost_swap_status.lock()`, etc. These locks are held briefly so it's fine, but if any lock body ever becomes async or long-running, it could block the tokio runtime.

- The `input.rs` keyboard/controller listeners run on OS threads (`std::thread::spawn`), which is correct since `rdev::listen` blocks. Good.

---

## 3. Error Handling

### Strengths
- Config load/save errors are captured in `ConfigStatus` and surfaced to the user — good UX pattern.
- Network reconnect loop with 5-second backoff is reasonable.
- MMR fetch has cooldown logic (60s for 429/403, 5s for transient errors, separate `failed_fetches` map).

### Concerns

> [!NOTE]
> Several `unwrap()` calls on Mutex locks (e.g. `boost_swap_status.lock().unwrap()` in `boost_operation_running`). A poisoned Mutex (from a panicking thread) would crash the whole app. Consider using `.lock().unwrap_or_else(|e| e.into_inner())` or `.lock().ok()` for non-critical status reads.

- `replays.rs`: `run_replay_upload` and `run_bulk_upload` return `Ok(())` even on error conditions (e.g., folder doesn't exist, API key empty). They set status strings but the `Result` return is misleading — the caller only prints errors from `Err`. Either propagate real errors or change the return type.

- `network.rs` line 562: `parts[0]` in `parse_platform()` — if `id` is non-empty but has no `|` separator, this panics. Should use `.first()` or check `parts.len()`.

---

## 4. Code Duplication & DRY

> [!TIP]
> **Replay upload cache trimming** is copy-pasted in 4 locations (`run_replay_upload` ×2, `run_bulk_upload`, `run_sync_replays`). Extract a helper like:
> ```rust
> fn mark_replay_uploaded(state: &AppState, filename: String) {
>     let config_current = state.config.load();
>     let mut config_edit = (**config_current).clone();
>     if !config_edit.uploaded_replays.contains(&filename) {
>         config_edit.uploaded_replays.push(filename);
>         if config_edit.uploaded_replays.len() > 500 {
>             let skip = config_edit.uploaded_replays.len() - 500;
>             config_edit.uploaded_replays = config_edit.uploaded_replays.split_off(skip);
>         }
>         state.save_config(config_edit);
>     }
> }
> ```

- The `update_*` helper functions in `network.rs` (`update_transport`, `update_last_event`, `update_parse_error`, `update_connection_error`) all follow the same clone-modify-store pattern. A generic `update_diagnostics(state, |d| { ... })` helper would reduce boilerplate.

- HTTP client construction appears in `replays.rs` (`verify_token`, `upload_file_to_ballchasing`) and `update.rs` (`check_latest_release`, `download_and_apply_update`). Reusing a shared `wreq::Client` (like `mmr_client` is for MMR) would reduce overhead and centralize timeout configuration.

- Key-matching logic in `input.rs` (`KeyPress` and `KeyRelease` handlers) has duplicated Kp→Num aliasing logic. Extract to a helper.

---

## 5. Testing

### Strengths
- Core logic modules have good test coverage:
  - `state.rs`: Config defaults, identity comparison, round-trip persistence
  - `network.rs`: Platform parsing, auto-GG key sequences, player state handling, lobby events, early leave
  - `session.rs`: Win/loss recording, streaks, edge cases (tied scores, unknown results, mode records)
  - `mmr.rs`: Cache key generation, cooldown behavior, local MMR error paths
  - `setup.rs`: INI creation, update, and no-op detection
  - `stats_api.rs`: TCP JSON splitter edge cases
  - `update.rs`: Version comparison, checksum parsing, SHA256

### Concerns

- **No integration tests.** The `tests/` directory is empty (only root-level temp files). An integration test that stands up a mock WebSocket server and exercises `handle_event` end-to-end would catch regressions.

- **UI is untested.** The `ui/` module (~185K, 10 files) has zero tests. While UI testing is hard in egui, snapshot testing or at least testing the data transformations that feed the UI would add value.

- `network.rs` tests at line 798+ (early-leave tests) are cut off — make sure they're not partial.

---

## 6. Security & Privacy

> [!WARNING]
> **API keys in config file.** `ballchasing_api_key` is stored in plaintext in `config.toml`. This is standard for local tools, but document it clearly. Consider noting in the README that users should not share their config file.

- `mmr.rs` sends requests to tracker.gg with a hard-coded Chrome User-Agent and `Referer: https://rocketleague.tracker.network/` header. This is standard for scraping but worth noting — if tracker.gg changes their anti-bot measures, this will break.

- The auto-update system (`update.rs`) downloads an exe and verifies a SHA256 checksum, which is good. However, both the exe and checksum come from the same GitHub release — if the release is compromised, both would be. Code-signing would be the next step, but SHA256 verification is a reasonable baseline.

- `simulate_auto_key_sequence` in `network.rs` injects key events into the OS. The `rocket_league_accepts_auto_input()` guard on Windows (foreground check) is a good safety measure. On Linux it's unconditionally `true` — acceptable since rdev on Linux usually goes through X11/Wayland which scopes to the session.

---

## 7. Performance

### Strengths
- Smart repaint scheduling in `schedule_repaint()` — adaptive intervals from 16ms (animation) to 1000ms (idle). This avoids burning CPU.
- `ArcSwap` for state reads means the UI never blocks on a lock during rendering.
- MMR cache with 1-hour TTL and 2-second polling interval is respectful of tracker.gg rate limits.

### Concerns

> [!NOTE]
> **`uploaded_replays` is a `Vec<String>` checked with `.contains()`.** With up to 500 entries and linear scans on every replay file, this is O(n) per check. A `HashSet<String>` would be O(1). The serialization format would need adjusting (TOML array → set), but it's worth it for the bulk upload path.

- `diagnostics.rs` `ResourcePoller` creates a `System::new_all()` which scans all processes. At 250ms polling this is fine, but it could cause momentary CPU spikes on systems with many processes. The `ProcessRefreshKind::nothing().with_cpu()` optimization is good.

- `network.rs` TCP read buffer is 16KB fixed. For very large game state payloads this means multiple read cycles. The `TcpJsonSplitter` handles this correctly, so it's not a bug — just a note.

---

## 8. Maintainability & Style

### Strengths
- Consistent Rust 2024 edition features (let-chains, `#[derive]` usage).
- Good use of `#[serde(default)]` for config forward-compatibility.
- `Config::load()` has a nice fallback chain (config dir → local `config.toml` → defaults).

### Concerns

- **Magic numbers:** `255` as sentinel for "no team" (`local_team: AtomicU8::new(255)`) should be a named constant or use `Option<u8>` internally.

- **Commented-out code:** `network.rs:483` has `// println!(...)`. Remove or use a proper logging framework.

- **No logging framework.** The project uses `println!` and `eprintln!` throughout (~60 instances). A crate like `tracing` or `log` with configurable levels would let users get debug output without recompiling with `--debug`.

- **`settings.rs` is 46 KB** — the largest file in the project. Breaking it into per-tab modules (`settings/overlay.rs`, `settings/boost.rs`, etc.) would dramatically improve navigability.

---

## 9. Specific File Notes

| File | Lines | Notes |
|------|-------|-------|
| `state.rs` | 613 | Well-structured. Config struct is large but `#[serde(default)]` handles evolution well. `detect_replays_path` has very long path literals — consider constants. |
| `network.rs` | 1137 | Largest core module. The auto-freeplay navigation (lines 288-324) is a brittle sequence of fixed-delay key presses — document that this is inherently fragile. |
| `mmr.rs` | 779 | Good caching and rate-limit handling. The `select_next_player` function has a subtle behavior: it returns the *first* cached player OR the first pending player, but iteration order of `HashMap` is nondeterministic. Consider using insertion-order-preserving `IndexMap` if priority matters. |
| `session.rs` | 596 | Clean state machine. `record_early_leave()` defaults to Loss on tie — documented in tests, which is good. |
| `replays.rs` | 546 | Functional but repetitive (see DRY section). The 30-second inter-upload delay for bulk is hardcoded; could be configurable. |
| `input.rs` | 304 | Dual keyboard+controller input handling is well-done. The debounce approach (200ms) is practical. |
| `diagnostics.rs` | 898 | Comprehensive debug tooling. Windows-only foreground detection is properly gated. The `ResourcePoller` start/stop lifecycle is clean. |
| `setup.rs` | 293 | INI parser/updater is well-tested. Line-ending preservation (`\r\n` vs `\n`) is a nice touch. |
| `update.rs` | 381 | Auto-update with SHA256 verification is well-implemented. The PowerShell script for Windows self-update is creative. |
| `stats_api.rs` | 253 | The `TcpJsonSplitter` brace-matching parser is correct and well-tested. |
| `json_utils.rs` | 26 | Tiny but essential. `number_field` only returns `u64` — consider adding a signed variant for fields that could be negative. |
| `ui/app.rs` | 723 | Complex but well-organized. The viewport state diffing (`last_viewport_state`) to avoid redundant commands is clever. |

---

## 10. Prioritized Recommendations

### 🔴 High Priority (Bugs / Correctness)

1. **Fix potential panic in `parse_platform()`** — `parts[0]` can panic if `id` contains no `|`. Use `parts.first()` with a fallback.

2. **Guard Mutex unwraps** — Replace `lock().unwrap()` with `lock().unwrap_or_else(|e| e.into_inner())` for non-critical reads like `boost_operation_running()`.

### 🟡 Medium Priority (Quality / Robustness)

3. **Extract replay upload cache helper** — Deduplicate the 4 identical upload-cache-trim blocks in `replays.rs`.

4. **Use `ArcSwap::rcu()`** — For `players` updates in `mmr.rs` and `network.rs` to prevent silent data loss under theoretical concurrent modification.

5. **Replace `Vec<String>` with `HashSet<String>`** for `uploaded_replays` in config — linear scan of 500 entries for each file is inefficient.

6. **Add a `const NO_TEAM: u8 = 255`** — Replace magic number `255` across `network.rs`, `state.rs`, and `session.rs`.

### 🟢 Low Priority (Maintainability)

7. **Split `settings.rs`** into per-tab sub-modules for better navigability.

8. **Introduce a structured logging crate** (`tracing` or `log`) to replace scattered `println!`/`eprintln!`.

9. **Group `AppState` fields** into domain sub-structs (`DiagnosticsState`, `ReplayState`, etc.).

10. **Add integration tests** with a mock Stats API WebSocket server to exercise end-to-end event handling.

---

## Summary Scorecard

| Category | Rating | Notes |
|----------|--------|-------|
| **Architecture** | ⭐⭐⭐⭐ | Clean module boundaries, could decompose `AppState` further |
| **Correctness** | ⭐⭐⭐⭐ | One panic risk in `parse_platform`, TOCTOU in state updates |
| **Testing** | ⭐⭐⭐⭐ | Strong unit tests on core logic, zero UI test coverage |
| **Error Handling** | ⭐⭐⭐ | Status-string pattern works but Mutex unwraps are risky |
| **Performance** | ⭐⭐⭐⭐⭐ | Smart repaint scheduling, lock-free UI reads |
| **Security** | ⭐⭐⭐⭐ | SHA256 update verification, plaintext API keys (acceptable for local) |
| **Maintainability** | ⭐⭐⭐ | Large files (settings.rs), no logging framework, some duplication |
| **Overall** | ⭐⭐⭐⭐ | **Solid indie project. Well above average for a personal tool.** |
