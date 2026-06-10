# Code Review — RL-Platform-Overlay

**Date:** 2026-06-05  
**Version:** 0.1.12  
**Reviewer:** Antigravity  
**Scope:** Full codebase (`src/`, `Cargo.toml`, CI, config)  
**Excluded:** `RLPeak/` (external reference project)

---

## Executive Summary

This is a well-structured Rust desktop overlay for Rocket League that provides real-time platform detection, MMR tracking, teammate boost HUDs, session tracking, and an alpha boost asset-swap feature. The codebase is clean, idiomatic Rust with good separation of concerns. The most significant areas for improvement are the **monolithic `ui.rs` file (2534 lines)**, **duplicated helper functions**, **missing error logging in several async paths**, and **a few concurrency patterns that could be tightened**. Test coverage is solid for the core logic modules but absent for the UI layer.

---

## Implementation Update — 2026-06-05

The first two remediation batches have been implemented.

### Completed

- **A1 / High Priority 1 — Split `ui.rs`:** `src/ui.rs` is now a thin module root that re-exports `MainApp`; rendering code was moved into focused `src/ui/` submodules: `app`, `settings`, `debug`, `hotkeys`, `lobby_overlay`, `boost_hud`, `session_hud`, `layout`, `mmr_panel`, and `common`. Existing UI helper tests moved with the MMR panel helpers.
- **A2 / DOC2 / Low Priority 12 — Glow vs `wgpu`:** Removed the explicit `eframe` `wgpu` feature while keeping `eframe::Renderer::Glow`. `cargo tree -e features -i wgpu` now reports no `wgpu` dependency path.
- **D1 / D2 / D4 / High Priority 3 — Helper consolidation:** Added shared `json_utils` helpers for JSON field access and double-decoded JSON handling. `network`, `session`, and `debug_game_output` use the shared helpers. `input` and `assets` now reuse `stats_api::now_ms`. `debug_game_output` reuses `TcpJsonSplitter`.
- **T1 — `save_config` store order:** `AppState::save_config` now stores the new config before publishing `config_status`.
- **E1 / E2 / E3 / E4 — Diagnostics:** TCP read errors are logged and stored in network diagnostics; background MMR HTTP client build failures are surfaced through `local_mmr.error`; gamepad init logs the actual `gilrs` error; the network task join result is monitored and logged.
- **TS1 / TS3 — Live MMR tests:** Tracker.gg-hitting tests are marked `#[ignore = "hits live tracker.gg endpoints"]`, so normal CI/test runs do not call live network endpoints.
- **CI1 / CI2 / Low Priority 13 — Lightweight CI:** Added `.github/workflows/ci.yml` for push/PR checks on `main`, running Ubuntu-only `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --all-targets`.
- **Clippy blockers found during implementation:** Fixed current `manual_contains`, `needless_borrow`, and `collapsible_if` warnings so the new clippy gate passes.

### Verification

- `cargo fmt --all -- --check` passes.
- `cargo clippy --all-targets -- -D warnings` passes.
- `cargo test --all-targets` passes: 31 passed, 2 ignored.
- `cargo tree -e features -i wgpu` reports no `wgpu` dependency path.

### Deferred

- **P1 — Repaint optimization:** Still deferred because changing repaint behavior can affect overlay responsiveness, hotkey capture, and layout dragging.
- **T4 — `boost_swap_status` ArcSwap migration:** Still deferred; current behavior remains mutex-backed.
- **A3 — Scratch files:** `test_mmr.rs` and `tests_mmr_temp.rs` remain ignored local scratch files.
- **T2 / P2 / P3 / P4 / S1 / DOC1 / TS2 / TS4:** Still open.

---

## Architecture & Project Structure

### Strengths

- **Clean module boundaries.** Each module has a clear responsibility: `network.rs` (WebSocket/TCP parsing), `mmr.rs` (tracker.gg integration), `session.rs` (win/loss tracking), `input.rs` (keyboard/gamepad), `setup.rs` (Stats API config), `assets.rs` (boost swap), `stats_api.rs` (capture tooling), `update.rs` (version check), `state.rs` (shared state), `ui.rs` (egui rendering).
- **Lock-free shared state.** Using `ArcSwap` for hot-path reads and `AtomicBool`/`AtomicU64` for flags is a great fit for the UI→background-task communication pattern. This avoids mutex contention on the render thread.
- **Dual transport support.** WebSocket-first with automatic TCP fallback based on error sniffing (`"invalid HTTP version"`) is clever and handles both Stats API configurations seamlessly.
- **Cross-platform support.** Conditional compilation (`cfg(windows)` / `cfg(not(windows))`) is used properly for platform-specific dependencies (winapi, gilrs features) and path detection.

### Concerns

| # | Severity | File | Issue |
|---|----------|------|-------|
| A1 | **Medium** | `src/ui.rs` | At **2534 lines**, this file is doing too much. It contains settings tabs, overlay rendering, teammate boost drawing, session panels, drag positioning, hotkey recording, debug tabs, and key mapping. Consider splitting into submodules (e.g., `ui/overlay.rs`, `ui/settings.rs`, `ui/boost_hud.rs`, `ui/session_hud.rs`, `ui/hotkeys.rs`). |
| A2 | **Low** | `src/main.rs` | The renderer is set to `eframe::Renderer::Glow` but the README says "Glow renderer through eframe." Cargo.toml specifies the `wgpu` feature on eframe. These may conflict — verify the actual renderer in use. The `Glow` renderer variant may not use the `wgpu` feature at all, meaning the `wgpu` dependency is pulled in but unused. |
| A3 | **Low** | Root | `test_mmr.rs` and `tests_mmr_temp.rs` exist at the project root but are gitignored scratch files. They reference outdated function signatures (e.g., `fetch_tracker_snapshot` without a `client` parameter). Consider deleting them to avoid confusion. |

---

## Correctness

### Strengths

- **Robust JSON field access.** The `string_field` / `number_field` helpers try multiple key casing variants (`"Name"`, `"name"`, etc.), handling Stats API inconsistencies well.
- **Double-encoded JSON handling.** Both `network.rs` and `session.rs` detect and re-parse string-encoded JSON payloads — a real-world necessity for the RL Stats API.
- **Session tracking logic** is correct: results are recorded only once per match (`result_recorded_for_match`), with a fallback from winner-name to score-comparison.

### Concerns

| # | Severity | File(s) | Issue |
|---|----------|---------|-------|
| C1 | **Medium** | `network.rs:262` | `parse_platform` indexes `parts[0]` without checking that `parts` has any elements. While `id.is_empty()` is checked earlier, a string like `"|"` would produce `parts = ["", ""]` and match the `_` arm, which is fine — but a string with no `|` character produces `parts = ["whole_string"]`, which would match the `_` arm and return `(whole_string, false)`. This is technically correct but may benefit from a comment. |
| C2 | **Medium** | `network.rs:204-207` | The `if !new_players.is_empty()` block contains only a commented-out `println!`. This dead code block should be removed. |
| C3 | **Low** | `session.rs:44-49` | The double-decode logic is duplicated from `network.rs:101-106`. If the data shape changes, both need to be updated in sync. |
| C4 | **Low** | `input.rs:181-183` | The `Kp` → `Num` alias logic uses byte-offset slicing (`key_str[2..]`, `config.hotkey_kb[3..]`). This is safe for ASCII but fragile — a `debug_assert!` that the strings are ASCII would be prudent. |
| C5 | **Low** | `update.rs:70-71` | `compare_versions` returns `Option<Ordering>` and is checked with `is_some_and`. If parsing fails (returns `None`), the update is silently not shown. This is fine, but there's no log/diagnostic when a valid-looking tag fails to parse. |

---

## Concurrency & Thread Safety

### Strengths

- `ArcSwap` is used correctly everywhere — loads produce `Arc` guards, and stores atomically replace the inner value.
- `AtomicBool` with `Ordering::SeqCst` is used consistently across all flag reads/writes, which is conservative but safe.

### Concerns

| # | Severity | File | Issue |
|---|----------|------|-------|
| T1 | **Medium** | `state.rs:382-389` | `save_config` stores `config_status` before `config`, creating a window where status reports success but the in-memory config hasn't been updated yet. In practice this is benign since both are `ArcSwap` stores, but swapping the order would be more semantically correct — update the config first, then the status. |
| T2 | **Medium** | `mmr.rs:255-263` | The cached player path loads `state.players`, checks a name, mutates the `HashMap`, and stores it back. Between load and store, another thread could have updated `players`, and this store would overwrite that update. The same TOCTOU pattern exists at lines 277-281. This is inherent to `ArcSwap` without a CAS loop, but could lose a concurrent player update. For a UI overlay this is low-impact but worth noting. |
| T3 | **Low** | `input.rs:52` | `now_ms()` is cast from `u128` to `u64` with `as u64`, silently truncating. The current epoch in milliseconds fits in `u64` until the year 584 million, so this is safe in practice but the truncation is implicit. |
| T4 | **Low** | `assets.rs:458` | `boost_swap_status` uses `std::sync::Mutex` while every other shared field uses `ArcSwap`. This inconsistency means the UI thread takes a mutex lock on every frame to read the boost status. Consider using `ArcSwap<String>` for consistency. |

---

## Error Handling

### Strengths

- Config load/save errors are properly surfaced in the UI via `ConfigStatus`.
- The boost swap pipeline has excellent error propagation with user-visible status messages.
- Network errors are captured in `NetworkDiagnostics` and shown in the Debug tab.

### Concerns

| # | Severity | File | Issue |
|---|----------|------|-------|
| E1 | **Medium** | `network.rs:60` | TCP read errors (`Err(_) => break`) silently discard the error. The error should be logged or stored in diagnostics, similar to how connection errors are handled. |
| E2 | **Medium** | `mmr.rs:206-209` | If the `wreq::Client` fails to build, the entire MMR task exits silently after an `eprintln`. Since the UI has no way to know this happened, users may wonder why MMR never loads. Consider storing an error in `local_mmr.error` or `network_diagnostics`. |
| E3 | **Low** | `input.rs:144` | The gamepad init failure arm (`_ => { eprintln!(...) }`) uses a wildcard pattern that catches `Err(_)`. Using `Err(e)` would allow logging the actual error. |
| E4 | **Low** | `main.rs:22-24` | `network::start_network_task` is spawned but its completion (or panic) is never monitored. If it panics, the task silently disappears. Consider using `tokio::spawn` with a `JoinHandle` that's at least logged. |

---

## Performance

### Strengths

- Rocket League process detection is cached for 2 seconds, avoiding the expensive `sysinfo` scan on every frame.
- `ArcSwap` avoids mutex contention on the render hot path.
- Gamepad polling sleeps 5ms between iterations (200Hz), balancing responsiveness with CPU usage.

### Concerns

| # | Severity | File | Issue |
|---|----------|------|-------|
| P1 | **Medium** | `ui.rs:249` | `ctx.request_repaint()` is called unconditionally every frame, keeping the CPU at 100%. Consider only requesting repaint when data has changed, or using `request_repaint_after(Duration::from_millis(16))` for ~60fps. When the overlay is inactive and settings are closed, there's no reason to repaint at all. |
| P2 | **Low** | `assets.rs:96-97` | `is_rocket_league_running` creates a new `System::new_all()` every call. Even with 2-second caching, this allocates and scans all processes. Consider keeping a `System` instance alive and calling `refresh_processes` on it. |
| P3 | **Low** | `network.rs:138` | `state.local_player_name.load().trim().to_string()` allocates a new `String` every `UpdateState` event (which arrives at 30-60Hz). Consider storing the trimmed value or using `Arc<str>`. |
| P4 | **Low** | `mmr.rs:88-95` | The warmup request to tracker.gg is fire-and-forget (`let _ = ...`). This is intentional but adds ~200-500ms of latency to every MMR fetch. If the warmup is not actually needed, removing it would halve the fetch time. |

---

## Code Duplication

| # | Severity | Files | Issue |
|---|----------|-------|-------|
| D1 | **Medium** | `network.rs`, `session.rs`, `debug_game_output.rs` | `string_field` and `number_field` are independently defined in three places with identical implementations. Extract into a shared utility module (e.g., `json_utils.rs`). |
| D2 | **Medium** | `input.rs`, `stats_api.rs`, `assets.rs` | `now_ms()` is defined independently in three modules. Extract into a single shared function. `stats_api::now_ms` is already `pub` and used from `ui.rs` — the others should reuse it. |
| D3 | **Low** | `mmr.rs:47-53`, `mmr.rs:67-73` | `tracker_api_url` and `tracker_warmup_url` have near-identical platform-matching logic. Consider extracting the platform normalization into a shared helper. |
| D4 | **Low** | `debug_game_output.rs` | The TCP JSON splitting logic (lines 195-226) is a manual reimplementation of `TcpJsonSplitter` from `stats_api.rs`. It should use the existing `TcpJsonSplitter` instead, especially since the binary has access to the crate's code via `use`. |

---

## Security & Safety

| # | Severity | File | Issue |
|---|----------|------|-------|
| S1 | **Medium** | `mmr.rs:91-93` | Hard-coded User-Agent string impersonates Chrome. While this is common practice for scraping tracker.gg, it could be flagged or blocked. Consider using a custom UA that identifies the overlay (as `update.rs` already does with `"RL-Platform-Overlay"`). |
| S2 | **Low** | `assets.rs` | The boost swap downloads executable game assets from a GitHub release URL over HTTPS and verifies SHA-256 hashes before applying. This is a solid security pattern. ✅ |
| S3 | **Low** | `config.toml` | Config is stored as plaintext TOML in a predictable location. No sensitive data is stored (no API keys, tokens, or credentials), so this is fine. |
| S4 | **Info** | `.gitignore` | `config.toml` is gitignored, which is correct. The `docs/` directory is also gitignored — note that this review file will need to be force-added or the gitignore amended. |

---

## Testing

### Strengths

- **Good unit test coverage** for core logic:
  - `network.rs`: Platform parsing, state updates, local player detection, lobby clearing, identity persistence (5 tests).
  - `session.rs`: Win/loss recording, deduplication, score fallback, unknown results (3 tests).
  - `setup.rs`: INI creation, update, and idempotency (3 tests).
  - `assets.rs`: Path validation, hashing, metadata round-trip, unknown file blocking, state detection, corrupt backup detection (7 tests).
  - `state.rs`: Config defaults, identity case-insensitive comparison (2 tests).
  - `stats_api.rs`: TCP splitter edge cases (2 tests).
  - `update.rs`: Version parsing, comparison, tag extraction (3 tests).
  - `ui.rs`: Playlist naming, sort priority, age formatting (3 tests).
  - `mmr.rs`: Player filtering, cache key stability (2 tests, plus 2 integration tests that hit the network).
- Tests use `#[cfg(test)]` modules in each file, following Rust conventions.

### Concerns

| # | Severity | Issue |
|---|----------|-------|
| TS1 | **Medium** | **Network-dependent tests** in `mmr.rs` (`test_pengiwin_steam`, `test_alfa_psn`) hit live tracker.gg endpoints. These will fail in CI without network access and will break if the API rate-limits them. Mark them `#[ignore]` or gate behind a feature flag. |
| TS2 | **Medium** | **No integration tests.** There's no `tests/` directory. Consider adding integration tests that exercise the full data flow: JSON payload → `handle_event` → state mutations → session updates. |
| TS3 | **Low** | `mmr.rs` integration tests print results to stdout but don't assert anything meaningful — they're exploratory tests rather than regression tests. |
| TS4 | **Low** | Setup tests create temp directories using `std::env::temp_dir()` and clean up manually. Consider using the `tempfile` crate for automatic cleanup. |

---

## Dependencies

| Crate | Version | Notes |
|-------|---------|-------|
| `eframe` | 0.31.0 | Features `wgpu` enabled but renderer is set to `Glow` — verify necessity. |
| `egui` | 0.31.0 | ✅ |
| `tokio` | 1.44.2 | Full features enabled — appropriate for this use case. |
| `tokio-tungstenite` | 0.26.0 | ✅ |
| `wreq` | 5.3 | Less common HTTP client. Consider using `reqwest` for broader ecosystem support and more documentation. |
| `rdev` | 0.5.3 | Global keyboard hook — appropriate for overlay hotkeys. |
| `gilrs` | 0.11.0 | Gamepad input — appropriate. Platform-specific feature flags are correctly applied. |
| `arc-swap` | 1.7 | ✅ Excellent choice for this pattern. |
| `sysinfo` | 0.36.1 | Used only for process detection. Could be heavy for just checking if RL is running. |
| `sha2` | 0.10 | ✅ |
| `serde` / `serde_json` / `toml` | Latest 1.x | ✅ |

---

## CI/CD

The `release.yml` workflow is clean:
- Builds on both `ubuntu-latest` and `windows-latest`.
- Uses `dtolnay/rust-toolchain@stable`.
- Uploads artifacts and creates a GitHub release on tag push.

### Suggestions

| # | Severity | Issue |
|---|----------|-------|
| CI1 | **Medium** | **No CI check on PR/push.** The workflow only triggers on tags (`v*`). Add a separate workflow (or expand this one) to run `cargo check`, `cargo test`, and `cargo clippy` on every push and PR. |
| CI2 | **Low** | No `cargo fmt --check` step. Consider adding it to enforce consistent formatting. |

---

## Documentation

### Strengths

- README is clear, practical, and includes setup instructions for both Windows and Linux.
- CHANGELOG follows Keep a Changelog format and is well-maintained.
- `config.example.toml` provides a complete reference for all config keys.

### Concerns

| # | Severity | Issue |
|---|----------|-------|
| DOC1 | **Low** | No inline doc comments (`///`) on any public functions or structs. Key types like `AppState`, `Config`, `PlayerInfo`, and `TrackerSnapshot` would benefit from rustdoc. |
| DOC2 | **Low** | The README lists "Glow renderer" under Tech Stack but the code uses `eframe::Renderer::Glow` while pulling in `wgpu` features. This could confuse contributors. |
| DOC3 | **Info** | `Docs/` and `docs/` both exist (case-sensitive distinction matters on Linux). The uppercase one appears to be gitignored, but its existence may cause confusion. |

---

## Summary of Recommendations (Prioritized)

### High Priority

1. **Done:** Split `ui.rs` into submodules — `src/ui.rs` is now a module root and rendering code lives under `src/ui/`.
2. **Done:** Add CI on push/PR — lightweight Ubuntu-only CI now runs fmt, clippy, and tests.
3. **Done:** Consolidate duplicated helpers — JSON helpers and timestamp usage were centralized; debug TCP splitting now reuses `TcpJsonSplitter`.

### Medium Priority

4. **Done:** Mark network-hitting MMR tests as `#[ignore]` to prevent CI failures and rate-limiting.
5. **Deferred:** Reduce unconditional repaint — `ctx.request_repaint()` every frame burns CPU unnecessarily.
6. **Done:** Log TCP read errors in `network.rs` instead of silently breaking the loop.
7. **Done:** Surface `wreq::Client` build failures in `mmr.rs` to the UI.
8. **Done:** Make `debug_game_output.rs` reuse `TcpJsonSplitter` instead of reimplementing it.
9. **Deferred:** Switch `boost_swap_status` from `Mutex<String>` to `ArcSwap<String>` for consistency.

### Low Priority

10. **Open:** Add `///` doc comments to public types and functions.
11. **Deferred:** Delete stale root-level scratch files (`test_mmr.rs`, `tests_mmr_temp.rs`).
12. **Done:** Verify eframe's `wgpu` feature is needed when using the `Glow` renderer — explicit `wgpu` feature was removed.
13. **Done:** Add `cargo fmt --check` and `cargo clippy` to CI.
14. **Open:** Consider `tempfile` crate for test cleanup.
15. **Open:** Evaluate if tracker.gg warmup request is actually needed.

---

## Conclusion

This is a solid hobby/community project with **thoughtful concurrency design**, **good error UX**, and **strong feature coverage**. The code is idiomatic Rust, the architecture is clean for a project of this size, and the test suite covers the important business logic well. The main technical debt is the `ui.rs` monolith and scattered code duplication, both of which are straightforward to address incrementally. Adding CI checks and splitting the UI module would be the highest-impact improvements.
