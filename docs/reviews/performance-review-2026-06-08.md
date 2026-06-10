# Performance Review: RL Platform Overlay

**Date:** 2026-06-08
**Reviewer:** opencode (automated review)
**Scope:** All runtime hot paths — network event handling, UI rendering, config I/O, MMR fetching

---

## Critical

### 1. Config saved on every drag frame (~60 Hz synchronous disk I/O)

`src/ui/layout.rs:97-104` — `persist_dragged_position` clones the entire `Config` struct (~2KB), serializes it to TOML, and writes it to disk **on every frame while dragging**. This blocks the render thread with synchronous `fs::write` at 60 Hz.

```rust
let mut config = (**state.config.load()).clone();  // ~2KB clone
// ... set one field ...
state.save_config(config);  // serialize + fs::write on render thread
```

**Impact:** ~60 syscalls/sec during drag. Adds input latency and frame drops.

**Fix:** Only persist on `drag_response.drag_stopped()`. The in-memory position is already stored in egui temp data and used for rendering. Disk persistence can happen once at the end.

---

## High

### 2. Full player HashMap cloned on every UpdateState event (~30-60 Hz)

`src/network.rs:482-492` — Every `UpdateState` event (the most frequent event, fired dozens of times per second during gameplay) clones the entire `new_players` HashMap via `rcu`, which internally calls the closure. The closure clones `new_players` and iterates all players to merge MMR.

Each `PlayerInfo` clone allocates 3 `String`s (`name`, `primary_id`, `platform`) + an `Option<TrackerSnapshot>` containing a `HashMap<i32, TrackerPlaylistSnapshot>` with ~2 Strings per entry. For 6 players, that's ~18 String allocations + MMR HashMap clones per event.

**Impact:** ~18-36 String allocations × 30-60 Hz = 540-2160 allocs/sec during matches.

**Fix:** Use a diff-based approach — compare new vs. existing players and only mutate changed fields. Or use `Arc::make_mut` with copy-on-write semantics to avoid cloning when the refcount is 1.

### 3. Session state cloned + stored on every network event

`src/network.rs:273-275`, `src/network.rs:501-503` — `state.session` is loaded, the entire `SessionState` is cloned, mutated, then stored back via `ArcSwap::store`. `SessionState` contains a `BTreeMap<SessionMode, SessionModeRecord>`, an `active_match_id: String`, and several other fields.

**Impact:** Full struct clone + BTreeMap clone + String clone on every event (~30-60 Hz).

**Fix:** Use `state.session.rcu(|s| { ... })` to do the load-mutate-store atomically, avoiding the separate load + manual store.

### 4. `NetworkDiagnostics` cloned on every event via 3 separate update functions

`src/network.rs:534-558` — `update_last_event`, `update_parse_error`, and `update_transport` each load the full `NetworkDiagnostics` struct, clone it, modify one field, and store it back. On a typical event, `update_last_event` fires every time, and `update_parse_error` fires on failed parses.

The struct contains `last_event: String`, `last_parse_error: String`, `last_connection_error: String`, and other fields. Each update clones all of them.

**Impact:** 1-3 full struct clones per event, each involving 3 String clones.

**Fix:** Split `NetworkDiagnostics` into individual `ArcSwap` fields or atomics. Use `AtomicU64` for timestamps, `ArcSwap<String>` for error strings. Or batch all diagnostic updates into a single `rcu` call per event.

### 5. `decode_json_string_value` clones `Value` on every call

`src/json_utils.rs:3-8` — Called at the top of `handle_update_state` and `session.handle_update_state` on every event. When the value is not a string, it clones the entire `Value` tree. When it is a string, it parses into a new `Value`.

```rust
pub fn decode_json_string_value(value: &Value) -> Value {
    if let Some(encoded) = value.as_str() {
        serde_json::from_str::<Value>(encoded).unwrap_or_else(|_| value.clone())
    } else {
        value.clone()
    }
}
```

**Impact:** Deep `Value` tree clone on every event.

**Fix:** Return a `Cow<'_, Value>` or restructure to avoid owning the result. If the value is already a `Value::Object`, return a reference instead of cloning.

### 6. 3+ `ArcSwap::store` calls per network event

`src/network.rs` — On a single `UpdateState` event, the code stores: `local_player_name` (line 351/407/472), `players` (line 482), `session` (line 503), `local_team` (line 479), `network_diagnostics` via `update_last_event` (line 267/538). Each `ArcSwap::store` does an atomic pointer swap + drops the old `Arc`.

**Impact:** 5+ atomic operations + Arc drops per event.

**Fix:** Batch updates — accumulate all changes and do a single store per field. Or use `rcu` to combine load-mutate-store.

---

## Medium

### 7. `Config::save()` called synchronously from the render thread

`src/state.rs:252-261` — `fs::write` blocks the calling thread. Called from:
- `layout.rs:104` — every drag frame (see Critical #1)
- `app.rs:719` — every settings change
- `input.rs:109,179,186,195` — on hotkey recording
- `replays.rs:111,178,384,534` — on every upload

**Impact:** Variable. Worst case: 60 Hz during drag. Typical: a few times per second during active use.

**Fix:** Debounce saves. During drag, save only on drag stop. For settings, batch and save with a 500ms delay. For replays, save after the upload batch completes, not per-file.

### 8. `debug_tracker_logs` Vec uses `remove(0)` — O(n) front removal

`src/mmr.rs:557-559` — When the log exceeds 100 entries, `logs.remove(0)` shifts all remaining elements. With 100 entries, each append costs a memmove of ~100 Strings.

```rust
if logs.len() > 100 {
    logs.remove(0);  // O(n) — shifts 99 elements
}
```

**Impact:** Called on every MMR fetch (~every 2s). Cost: ~100 String pointer shifts per call.

**Fix:** Use `VecDeque` for O(1) front removal: `logs.pop_front()`.

### 9. `uploaded_replays` Vec with `split_off` for cap enforcement

`src/replays.rs:104-109` — When the upload cache exceeds 500 entries, `split_off(skip)` creates a new `Vec` allocation, copies elements, and drops the old one. The `contains()` check is O(n) linear scan on a Vec.

```rust
config_edit.uploaded_replays.push(filename.clone());
if config_edit.uploaded_replays.len() > 500 {
    let skip = config_edit.uploaded_replays.len() - 500;
    config_edit.uploaded_replays = config_edit.uploaded_replays.split_off(skip);
}
```

**Impact:** O(n) lookup + O(n) reallocation on every upload. With 500 entries, each check scans up to 500 Strings.

**Fix:** Use a `HashSet<String>` for O(1) lookup. Keep a separate `Vec` for ordered display if needed. Use a ring buffer or `VecDeque` with `pop_front` for the cap.

### 10. `boost_operation_running` locks a Mutex every frame

`src/ui/app.rs:525-538` — `schedule_repaint` calls `boost_operation_running(state)` every frame, which locks `state.boost_swap_status` (a `std::sync::Mutex<String>`) and parses the string with 7 `starts_with` checks.

**Impact:** Mutex lock + String comparison × 7 per frame. Low cost individually, but unnecessary contention on the render thread.

**Fix:** Use an `AtomicBool` or `AtomicU8` to track whether a boost operation is running. Set it to `true` when starting, `false` when done. No Mutex needed for the render thread.

### 11. Synchronous `fs::` calls in async replay upload tasks

`src/replays.rs:56,84,94,152,256,266,311,358` — `fs::read_dir`, `fs::metadata`, `fs::read` are all synchronous and called inside `async` functions on tokio threads. These block the async runtime.

**Impact:** Background thread, not render thread. But blocks other tokio tasks on the same runtime during file I/O.

**Fix:** Use `tokio::fs::read_dir`, `tokio::fs::metadata`, `tokio::fs::read` for non-blocking I/O.

### 12. `name.to_lowercase()` in sort comparator allocates per comparison

`src/ui/lobby_overlay.rs:268` — `a.name.to_lowercase().cmp(&b.name.to_lowercase())` allocates two new `String`s on every comparison. With N players, sort does O(N log N) comparisons.

**Impact:** N is small (max ~10 players), so this is ~40 String allocs per frame. Negligible in practice.

**Fix:** Use `str::eq_ignore_ascii_case` or pre-lowercase once and cache.

### 13. Per-frame `Config` clone in settings rendering

`src/ui/settings.rs:97` — `let mut config_edit = (**state.config.load()).clone()` clones the entire ~2KB Config struct at the start of every settings tab render. This happens every frame while the settings window is visible.

**Impact:** One ~2KB clone per frame. Minor.

**Fix:** Only clone the fields needed for the current tab. Or use `RwLock<Config>` with interior mutability.

### 14. Log `Vec<String>` cloned for render

`src/ui/debug.rs:133`, `src/ui/settings.rs:1285` — `state.debug_tracker_logs.lock().clone()` and `state.hoops_fixer_logs.lock().clone()` deep-clone the entire log Vec to render it.

**Impact:** Every frame when the debug/settings tab is open. With 100 log entries at ~50 chars each, that's ~5KB of String clones per frame.

**Fix:** Hold the lock for the duration of the render loop instead of cloning. Or use `Arc<Vec<String>>` and only swap when the log is updated.

### 15. `local_player_name` stored via `ArcSwap` on every event even when unchanged

`src/network.rs:407` — `state.local_player_name.store(Arc::new(name.clone()))` creates a new `Arc<String>` on every `UpdateState` that identifies a local player, even if the name is the same as the previous frame.

**Impact:** One Arc alloc per event (~30-60 Hz).

**Fix:** Compare with the previous value before storing. `state.local_player_name.rcu(|prev| if **prev == name { prev.clone() } else { Arc::new(name.clone()) })`.

---

## Low

### 16. `include_image!` embedded images re-resolved per frame

`src/ui/lobby_overlay.rs:759-794` — `rank_icon()` and `platform_icon()` are called per-player per-frame. They do string comparisons (`rank.trim().to_lowercase()`) and return `ImageSource<'static>`. The `ImageSource` itself is cheap (just a pointer), but the string work is repeated.

**Impact:** ~6 players × string allocs per frame. Minor.

**Fix:** Cache the icon lookup in `PlayerInfo` or a side map, keyed by `tier_name`.

### 17. Preview data allocated every frame in settings/layout mode

`src/ui/lobby_overlay.rs:14-83`, `src/ui/boost_hud.rs:20-58` — `preview_lobby_players()` creates fake `PlayerInfo` entries with `TrackerSnapshot` and `HashMap` inserts on every frame when layout mode is active.

**Impact:** Only in settings/preview mode. Not during gameplay.

**Fix:** Cache and only regenerate when config changes.

### 18. Small `format!` allocations in per-player render loops

`src/ui/lobby_overlay.rs:438,580,589`, `src/ui/boost_hud.rs:305,388,405,411` — `format!("{}%", player.boost)`, `format!("{} TCH | {} BMP | {} DEM", ...)`, etc. create small String allocations per-player per-frame.

**Impact:** ~6 players × 2-3 format calls = ~15 small allocs per frame.

**Fix:** Use `itoa` for integer formatting, or pre-format values when state changes rather than per-frame.

### 19. `streak_label` allocates String every frame

`src/ui/session_hud.rs:222-230` — `format!("+{} streak", streak)` creates a new String on every render call for the session overlay.

**Impact:** One alloc per frame. Negligible.

**Fix:** Use `&'static str` lookup for common streak values, or cache.

### 20. `enforce_borderless_style` Win32 syscalls every frame

`src/ui/app.rs:600-626` — `GetWindowLongW` + conditional `SetWindowLongW` + `SetWindowPos` on every frame on Windows. The code has a guard (`style != target_style`) but the comparison itself requires a syscall to read the current style.

**Impact:** 1-3 syscalls per frame on Windows. Low overhead.

**Fix:** Cache the last-applied style in an `AtomicI32` and skip the syscall when the target hasn't changed.

### 21. `append_hotkey_debug_log` opens file on every hotkey event

`src/input.rs:20-29` — Opens the log file, creates directories, and appends on every key press and release. Each open/create is a syscall.

**Impact:** 2 syscalls per key event. Only matters during active hotkey use.

**Fix:** Keep the file handle open, or use a background writer with a channel.

---

## Summary

| Severity | Count | Top Issues |
|----------|-------|------------|
| **Critical** | 1 | Config saved on every drag frame (synchronous disk I/O at 60 Hz) |
| **High** | 5 | Player HashMap clone per event, session clone per event, diagnostics clone per event, Value clone per event, 5+ ArcSwap stores per event |
| **Medium** | 9 | Sync file I/O on render thread, Vec.remove(0), uploaded_replays O(n), Mutex in render, sync fs in async, log clones |
| **Low** | 6 | Minor format allocs, preview regen, icon recomputation, Win32 syscalls |

## Top 5 Fixes (by impact-per-effort)

1. **Debounce `persist_dragged_position`** — save only on `drag_stopped()`. One-line change, eliminates 60 Hz disk I/O.
2. **Batch `NetworkDiagnostics` updates** — combine `update_last_event` + `update_parse_error` into a single `rcu` call. Reduces 3 ArcSwap stores to 1 per event.
3. **Use `VecDeque` for `debug_tracker_logs`** — replace `Vec` + `remove(0)` with `VecDeque` + `pop_front()`. One-line type change.
4. **Replace `Mutex<String>` for boost status with `AtomicBool`** — eliminates Mutex contention on the render thread.
5. **Use `tokio::fs` in replay upload** — replace synchronous `fs::read`/`fs::metadata` with async equivalents. Blocks fewer tokio tasks.
