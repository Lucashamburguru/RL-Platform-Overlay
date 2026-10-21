# RL Platform Overlay: work completed and performance review

Date: October 20, 2026 (America/Los_Angeles)

Repository: https://github.com/Lucashamburguru/RL-Platform-Overlay

Reviewed version: **v0.1.60**, commit `dee37b7c56869231da62cf674743007dede3a093`.

## Implementation follow-up — October 20, 2026

The original review below is retained as the baseline. This follow-up records the changes prepared for release v0.1.61:

- F01: Bot-only dashboard rosters terminate in a waiting panel. A rendering regression test covers both dashboard layouts.
- F02/F07: Static HUDs and dashboards now use a one-second fallback. Changed game data wakes visible HUDs; repeated identical packets do not. Live dashboard changes are coalesced with a 100 ms request. Global hotkeys, configuration, connection transitions, MMR, history, and replay upload publishers request redraws explicitly. The boost pulse requests fast frames only while rendering low-boost rows, and dragging/settings animations retain fast refreshes. Deferred dashboard callbacks load current configuration. Fullscreen monitor transitions explicitly wake the root viewport.
- F03/F04: Transport-owned JSON moves into parsing without copying the complete Data object. Game metadata is borrowed from that object. Parsed player maps are shared through `Arc`; complete rosters can be published without another deep copy. The dashboard shares the retained match roster and current session snapshot instead of rebuilding both. Unchanged rosters and dashboard snapshots reuse their previous allocations. Partial rosters still retain departed players, and live publication retains the MMR-preserving RCU merge. Regression tests cover allocation reuse, immutable retained results, owned/borrowed parser compatibility, and concurrent rank/stat updates.
- F05/F06: Live lobby and boost rendering borrow player records. Dashboard rendering borrows the selected session and roster, avoiding a redundant full player/rank copy before constructing display rows.
- F08: Replay/history filtered ordering is cached by immutable source identity, search text, source filter, and sort order. Replay title normalization runs once per sort rebuild. A regression test covers cache reuse and invalidation.
- F09: The controller listener uses `Gilrs::next_event_blocking` with a one-second shutdown timeout, replacing the application's 5 ms polling loop. Queued input and connection events wake the wait immediately. Inspection of gilrs-core 0.6.8 confirmed that Windows XInput already polls on its own thread and delivers events through a channel; its internal 10 ms polling remains unchanged. Linux waits through epoll and macOS through a channel. Physical controller latency, recording, hold behavior, and reconnection still need hardware verification.
- F10: Windows monitor enumeration is cached for up to one second, with immediate invalidation when display scale or the current monitor size changes.
- F13: Settings reuse the immutable original configuration snapshot, removing the second full configuration copy while retaining the update mutex.
- F14: Dashboard upload-status reads use a nonblocking lock and retain the last displayed status during contention.
- F15 (partial): The always-on-top viewport command is sent during window transitions instead of every root frame. Inspection of the locked egui source showed that sending a viewport command itself requests a repaint; repeating it prevented static windows from settling. Native style and monitor enforcement remain in place.

`./check.sh` passed: formatting, Clippy for normal/all-feature builds, tests for both configurations, and the Microsoft Store feature check. Library results after snapshot sharing and controller changes were 349 passed / 8 ignored normally and 323 passed / 4 ignored with all features; the integration test also passed in both configurations. Tests requiring local HTTP listeners were run outside the socket-restricted sandbox.

Headless rendering tests exercise static lobby/session HUDs, empty/high-boost panels, low-boost animation, and the actual deferred dashboard viewport callback. Settled static frames schedule their next fallback at least 900 ms away (egui subtracts a predicted frame interval from the requested one second); low-boost rows still request frames within 16 ms. Additional tests verify immediate global-hotkey wakeups, changed packet wakeups, suppression of identical packets, dashboard-only routing, batching, and weak ownership of the repaint context. These are scheduling checks, not CPU benchmarks. Representative before/after CPU measurements still require the same Rocket League match workload and native desktop rendering.

An isolated Windows GNU cross-check compiled the actual monitor module, shadow-disable API call, and the blocking controller wait with the locked Windows XInput backend successfully. Full-application Windows cross-checking remains blocked by the existing TLS dependency's cached CMake reference to a missing `/tmp/rl-overlay-windows-check.cmake` file.

Implementation note: `StatsApiEvent.players` and `DashboardMatchSnapshot.players/session` now use shared `Arc` snapshots. The duplicate `StatsApiEvent.game` field was replaced with a borrowing `game()` accessor. In-repository consumers were updated. MMR enrichment can still copy a live player map, and partial-roster merges and dashboard display-row preparation still have allocation costs.

Minimized-dashboard throttling and remaining derived-data preparation still require follow-up profiling. No CPU or RAM improvement has been measured. Windows desktop behavior still requires runtime verification, including always-on-top behavior, monitor transitions, and minimize/restore. The exported border patch continues to contain only the original three-file border fix; the v0.1.61 implementation includes additional optimizations beyond that standalone patch.

## Original review

## Executive summary

The strongest optimization opportunities are unnecessary redraws and repeated deep copies of player, rank, JSON, and match data. Small string and sorting changes should come after those. The dashboard also has a bot-only roster path that can recurse indefinitely and should be fixed before performance tuning.

Only the Windows top-border fix was implemented locally. The optimization findings remain recommendations. No performance gains have been measured, and none of the recommendations has been merged or released as part of this session.

## Work completed

| Work | Result | Validation / limitation |
| --- | --- | --- |
| Investigated the thin bar at the top of the Windows overlay | Identified a strong dependency-related explanation | Static comparison of dependency versions and upstream windowing implementation; not reproduced on Windows |
| Prepared the Windows shadow fix | Modified `src/lib.rs`, `Cargo.toml`, and `Cargo.lock` locally | Diff whitespace, TOML/lockfile consistency, and reverse patch applicability checks passed |
| Attempted to open a branch/PR | Branch creation rejected by GitHub with HTTP 403, “Resource not accessible by integration” | No branch or PR created; no remote changes made |
| Exported the fix | Created `windows-overlay-top-border.patch` | Downloadable patch supplied earlier in the conversation |
| Reviewed general performance | Inspected HUD rendering, Stats API processing, controller input, replay/history pages, and background work | Static review only |
| Reviewed dashboard performance | Inspected redraw scheduling, row construction, snapshot publication, monitor handling, and side panels | Static review only |

The original review environment had no Rust toolchain or Windows desktop. At that stage, compilation, unit tests, clippy, Windows visual verification, CPU profiling, and memory profiling were not run, and the working tree contained only the three-file border fix described above. See the implementation follow-up for current validation and scope.

## Windows top-border regression and prepared fix

The v0.1.59 dependency update changed egui/eframe from 0.31 to 0.36.2. The newer egui-winit code requests an undecorated-window shadow on Windows when decorations are disabled and the window is not fullscreen. The overlay uses a monitor-sized window on Windows rather than winit fullscreen, so it matches that condition.

Winit documents that enabling this shadow produces a thin one-pixel line at the top of the window. This fits the reported symptom. The application's existing native style stripping does not turn off winit's shadow setting.

The prepared fix:

- Adds a Windows-only direct dependency on `winit = { version = "0.30", default-features = false }`.
- Reuses the already locked `winit 0.30.13`; it does not introduce another winit version.
- Obtains the root native window through `CreationContext::winit_window()`.
- Calls `WindowExtWindows::set_undecorated_shadow(false)` during application creation.
- Updates the root package dependency list in `Cargo.lock`.

Using winit's setter also updates its internal window state, rather than merely changing native styles behind its back. `ViewportBuilder::with_has_shadow(false)` is not the Windows solution in this dependency version; it affects macOS.

The change disables the root window's shadow from startup, including the stopped launcher. It does not modify the decorated dashboard window. Future code that sends `ViewportCommand::Decorations(false)` should be reviewed because egui-winit's handler can enable the shadow again.

Upstream references:

- [egui-winit 0.36.2 implementation](https://github.com/emilk/egui/blob/0.36.2/crates/egui-winit/src/lib.rs)
- [winit Windows shadow documentation](https://docs.rs/winit/0.30.13/winit/platform/windows/trait.WindowAttributesExtWindows.html#tymethod.with_undecorated_shadow)

## Finding index

Priorities below indicate recommended engineering order, not measured CPU contribution. Findings overlap where one change can address several costs.

| ID | Priority | Finding | Primary condition |
| --- | --- | --- | --- |
| F01 | High | Bot-only dashboard roster can trigger infinite recursion | Dashboard rendering with an existing roster containing only bots |
| F02 | High | Static HUDs request redraws approximately every 16 ms | Any HUD marked visible |
| F03 | High | Stats API updates repeatedly clone JSON, players, and event data | Incoming game updates |
| F04 | Medium | Dashboard match snapshots are copied at packet rate, even when disabled | Eligible match updates |
| F05 | Medium | HUD rendering deep-clones player records | Visible lobby/boost HUD |
| F06 | Medium | Dashboard rows copy session/player/rank data every redraw | Dashboard visible |
| F07 | Medium | Dashboard and root request periodic redraws for unchanged content | Dashboard enabled, including waiting/results screens |
| F08 | Medium | Replay/history pages repeat filtering and sorting | Those settings pages open |
| F09 | Lower | Controller listener polls every 5 ms | Listener active, including idle periods |
| F10 | Lower | Windows monitor enumeration repeats during root rendering | Dashboard enabled |
| F11 | Lower | Highlights and playlist lists repeat stable preparation | Relevant dashboard panels visible |
| F12 | Lower / verify | Minimized dashboard has no application-level slow-refresh policy | Dashboard minimized |
| F13 | Lower | Settings clone configuration twice and hold an update mutex during rendering | Settings open |
| F14 | Lower | Dashboard rendering takes a blocking upload-status mutex | Replay upload panel visible |
| F15 | Lower | Root window repeats Windows management work | Overlay rendering |

## Detailed findings

### F01 — Bot-only dashboard roster can recurse indefinitely

Sources: `src/ui/dashboard.rs`, especially `render_team_columns` around line 707, `render_empty_state` around line 1638, and `build_dashboard_rows` around line 2039; `preview_lobby_players` in `src/ui/lobby_overlay.rs`.

The dashboard row builder excludes bots. If no rows remain, `render_team_columns()` calls `render_empty_state()`. That function obtains preview players and calls `render_team_columns()` again. The preview helper returns the existing roster when it is nonempty, even if every player is a bot. Filtering that roster again produces zero rows, repeating the same calls indefinitely.

Expected consequence: recursive rendering and allocation followed by stack overflow. This is a concrete static control-flow finding; the exact runtime trigger was not reproduced.

Recommendation: make empty-state rendering nonrecursive. When no eligible humans exist, either render a terminal waiting panel or generate explicit preview humans independently of the current roster. Add a focused regression test that renders the dashboard with a bot-only roster; the existing bot-filtering test does not exercise the recursive renderer.

### F02 — Static HUD visibility forces frequent redraws

Source: `src/ui/app.rs`, `schedule_repaint` and `repaint_delay`, around lines 1243–1266.

The scheduler requests a repaint after 16 ms whenever the lobby, session, or boost HUD is marked visible. Unchanged session statistics and empty boost panels therefore qualify for the same fast refresh as actual movement. A renderer that immediately returns because it has no teammates does not change the scheduler's visibility decision.

Recommendation: wake the UI when published data changes, retain fast refresh for dragging and active animations, and use slower fallback updates for clocks and connection health. Make background updates and hotkeys explicitly request redraws before reducing polling. Visibility should reflect useful content where appropriate.

This behavior existed before the dependency upgrade. It is not proof of a new dependency regression. A requested 16 ms delay is also not an exact frame-rate cap.

### F03 — Incoming Stats API data is repeatedly copied

Sources: `src/stats_api_parser.rs` around lines 90–107; `src/network.rs`, `handle_update_state` around line 222, plus match-roster and MMR-preserving publication.

The parsing pipeline owns decoded data and separately clones its nested game object. The update handler clones the parsed player map, clones it again for publication, and clones the event while replacing its player map. Roster merging adds further copies. Player copies can include strings and complete rank snapshots.

Recommendation: move owned data between stages, borrow event metadata where possible, and share immutable snapshots. Avoid cloning an entire event merely to pass adjusted players to downstream functions.

Preserve concurrency guarantees: the MMR fetcher can update player state while game updates arrive. Removing the existing merge/RCU behavior without an equivalent design could lose MMR updates.

The default configured packet rate is 30 Hz, making this a recurring ingestion cost rather than a one-time operation.

### F04 — Dashboard snapshot production costs remain when the dashboard is off

Source: `src/network.rs`, `update_dashboard_match_snapshot`, around line 433.

Every eligible update clones the previous dashboard snapshot, replaces player entries with more clones, and copies session state. The producer does not check whether the dashboard is enabled. This work happens at incoming packet frequency, independently of the dashboard's 100 ms repaint request.

Recommendation: share immutable roster/rank data and separate retained match information from frequently changing live statistics. Avoid rebuilding a complete presentation snapshot per packet.

Do not simply stop updates when the dashboard is disabled without deciding the intended behavior: the retained snapshot supports displaying match information after the live roster clears and potentially when the dashboard is opened later. Any throttling must preserve final results and relevant transitions.

### F05 — HUD renders deep-clone player records

Sources: `src/ui/lobby_overlay.rs`, `render_overlay` around line 202; `src/ui/boost_hud.rs`, `render_teammate_boost` around line 104.

The live render paths collect cloned `PlayerInfo` values into vectors. That copies nested fields even though the renderer usually only needs read access. At frequent repaint rates, these allocations repeat continuously.

Recommendation: hold the loaded player snapshot for the render and sort/filter references into it. Keep owned preview-player construction as a separate path. Sorting six players is not itself a major concern.

### F06 — Dashboard display rows are rebuilt and deep-copied every redraw

Sources: `src/ui/dashboard.rs`, `render_dashboard` around line 135 and `build_dashboard_rows` around line 2039.

Each dashboard render clones its selected session and player collection. Building rows clones rank snapshots again, including playlist maps, and copies history summaries. It also rebuilds identity keys, tracker URLs, rank labels, and other stable fields. Full rank data is prepared even when player details are closed.

Recommendation: borrow selected session/player data, separate stable identity/rank metadata from live numeric statistics, and fetch full details only for the selected player. Cache stable derived fields using relevant revisions, not a timer.

Rank information used by team-average calculations must remain available even if individual rank widgets are hidden. A display toggle is not sufficient grounds to remove all rank preparation.

### F07 — Unchanged dashboards keep requesting redraws

Sources: `src/ui/dashboard.rs` around line 126; `src/ui/app.rs`, `repaint_delay` around line 1252.

The dashboard requests another redraw after 100 ms even while waiting or showing retained match results. The root also schedules periodic redraws while the dashboard is enabled. These are separate viewport schedules; they should not be interpreted as an automatically additive or exact frame count.

Recommendation: request redraws on meaningful changes to match state, MMR, history, configuration, connection state, or upload progress. Use a slower fallback for unchanged states and preserve animation/interaction scheduling. Measure root and dashboard frame counts separately.

### F08 — Replay and history pages repeat work proportional to library size

Sources: `src/ui/settings/replays.rs` around lines 655–677 and its base-row cache around line 1004; `src/ui/settings/history.rs` around line 90; `src/ui/settings/library_table.rs`.

The replay page already caches its base rows, but rebuilds and sorts filtered references every frame. Title sorting creates lowercase strings inside the comparator. The history page similarly repeats filtering and sorting. The shared table walks all row keys to locate visible content even though it avoids rendering every row.

Recommendation: cache filtered/sorted indices by source revision, search, filter, and sort order; precompute normalized search/sort fields. Where row geometry permits, calculate visible row ranges directly, accounting for expanded details.

This primarily affects large libraries while those pages are open. It does not explain steady in-game CPU usage with the pages closed.

### F09 — Controller listener polls at approximately 200 wakeups per second

Source: `src/input.rs`, controller listener around lines 224–282.

The listener drains events, sleeps five milliseconds, and repeats even without button activity.

Recommendation: investigate backend-supported blocking event retrieval or adaptive polling. Verify XInput/backend requirements, reconnection, held-button behavior, and hotkey latency before changing the loop. Do not blindly increase the delay.

This is an idle-overhead candidate, not a demonstrated explanation for the reported CPU usage.

### F10 — Repeated Windows monitor enumeration

Sources: `src/ui/dashboard.rs` around line 58; `src/ui/monitor.rs`, `dashboard_placement` and `windows_monitors` around lines 29 and 123.

Every root render with the dashboard enabled recomputes placement through monitor enumeration. Windows calls `EnumDisplayMonitors` and `GetMonitorInfoW` and rebuilds a monitor vector. If the overlay is also active, this can run at the root's faster redraw frequency.

Recommendation: cache monitor topology/geometry and invalidate on display changes, scaling changes, selection changes, or a slow fallback refresh. Preserve existing fullscreen restoration when changing monitors and test mixed-DPI displays.

### F11 — Stable dashboard highlights and playlist preparation repeat

Sources: `src/ui/dashboard.rs`, `dashboard_events` around line 1850 and `render_local_mmr_list` around line 1984; expanded-player ranks around line 365.

Highlights format every candidate, sort them, then truncate to eight. Playlist lists are collected and sorted repeatedly despite infrequent rank changes. When a details panel is open, its last-seen date is formatted again on redraw.

Recommendation: cache these derived results by their actual inputs. If useful, choose the top highlight candidates before formatting their text. These collections are small, so implement after larger allocation/scheduling changes.

### F12 — No explicit minimized-dashboard throttling

Source: the deferred dashboard callback in `src/ui/dashboard.rs`, around lines 97–126.

The callback does not inspect minimized state before preparing rows and requesting its next redraw. This is an application-level observation; it does not prove Windows/eframe continues performing all rendering work while minimized.

Recommendation: measure minimized behavior first. If callbacks continue doing meaningful work, skip expensive preparation and slow the fallback schedule while minimized, with prompt refresh on restoration. Distinguish minimization from occlusion; platform support differs.

### F13 — Settings copy configuration twice and retain a mutex during rendering

Sources: `src/state.rs`, `begin_config_edit` around line 1611 and `ConfigEditSession` around line 1696; `src/ui/app.rs`, `render_settings_content` around line 1390.

Beginning an edit clones the configuration under the update mutex. Settings rendering clones it a second time for the original snapshot and keeps the edit guard through page rendering. This repeats even if no control changes.

Recommendation: reduce redundant copies and shorten the critical section, potentially with persistent local editing state and revision-aware commits. Preserve the protection against lost concurrent updates; simply releasing the lock and writing a stale whole configuration later is unsafe for correctness.

Relevant when settings are open, including alongside the dashboard; not the normal closed-settings baseline.

### F14 — Dashboard takes a blocking upload-status mutex

Source: `src/ui/dashboard.rs`, replay-upload side panel around line 1789.

The renderer locks the upload-status string and clones it. Most holds may be brief, but contention can block the UI thread.

Recommendation: publish an immutable status snapshot, or retain previously displayed text when a nonblocking read cannot acquire the lock. Profile contention before treating this as a significant CPU issue. Its main potential benefit is avoiding intermittent stalls.

### F15 — Repeated root-window management work

Source: `src/ui/app.rs`, Windows monitor/style handling around lines 777–824 and the launched branch's `WindowLevel(AlwaysOnTop)` command.

The overlay rechecks its monitor geometry and native styles each frame and sends an always-on-top command repeatedly. Some style calls are already conditional, and winit may avoid redundant underlying operations, so the actual cost is unmeasured.

Recommendation: send stable commands on transitions where safe, and measure the remaining native enforcement cost. Do not remove borderless enforcement outright: its comment explains that asynchronous window updates can reapply styles. Keep recovery for resize, reposition, and mode changes.

## Existing design choices worth preserving

- Immutable snapshots allow rendering without holding the network's data-processing locks.
- The replay library already caches its expensive base-row construction.
- History refreshes use a revision check and an in-flight guard rather than querying the database every frame.
- Replay metadata scans run through `spawn_blocking` and reuse file identities.
- Recent Stats API diagnostic logging samples `UpdateState` payloads once per second and has a byte limit; it does not retain every update indefinitely.
- Network reads await input and reconnect attempts have a delay; the inspected transport is not an obvious busy loop.
- Existing dashboard tests cover row behavior and layout. Performance changes need meaningful additional validation rather than tests that merely restate the implementation.

## Recommended implementation order

1. Fix F01 and add the bot-only rendering regression test.
2. Compile and visually validate the already prepared Windows shadow patch.
3. Establish reproducible CPU, frame-count, and allocation baselines.
4. Address F03/F04 ownership and snapshot production while preserving concurrent MMR and retained results.
5. Address F05/F06 borrowed rendering and stable metadata preparation.
6. Improve F02/F07 scheduling with explicit wakeups; verify hotkeys, clocks, animations, and restore behavior.
7. Address library-scale work in F08 and monitor caching in F10.
8. Pursue the remaining smaller items only where measurements justify them.

## Validation plan

Use the same release build configuration, machine, resolution, display scaling, and match workload for before/after comparisons. Keep scanning/uploads controlled so they do not contaminate the steady-state comparison.

| Scenario | Purpose |
| --- | --- |
| Overlay stopped; dashboard disabled | Background baseline |
| Session HUD only, unchanged statistics | Detect needless fast redraws |
| Boost HUD only, with and without teammates | Verify responsive updates and empty-panel behavior |
| All HUDs during a typical six-player match | Normal active overlay cost |
| Dashboard only, table and cards layouts | Isolate dashboard costs |
| Dashboard plus in-game overlay | Find combined root/secondary viewport costs |
| Waiting screen and retained match results | Confirm unchanged-state savings |
| Dashboard minimized and restored | Verify throttling and prompt restoration |
| Bot-only roster | Confirm the recursion fix |
| Player details open; MMR/history arrives | Check cache invalidation and details correctness |
| Monitor switch/fullscreen change/mixed DPI | Preserve placement behavior |
| Large replay/history library open | Measure filtering, sorting, and scrolling |
| Upload or metadata scan running | Separate transient background load from idle load |

Record total process CPU and per-thread samples, root/dashboard redraw counts, UI preparation time, allocation rate, private memory/working set, and game frame times. Report the CPU model and logical processor count because Task Manager CPU percentages depend on the machine. Check memory after a sustained run, not just startup.

Do not discard intermediate Stats API events without proving safety for touch debouncing, result recording, roster transitions, and replay detection. Reducing display refresh is different from dropping ingestion events.

## CPU/RAM estimates discussed — not benchmarks

The conversation included these rough planning guesses for a modern six-to-eight-core PC, release build, ordinary six-player match, with dashboard/settings closed:

| State | Total process CPU guess | RAM guess |
| --- | --- | --- |
| Stopped / idle | Below 1% | 100–250 MB |
| Active overlay, current code | 1–5% | 150–350 MB |
| After optimization | About 0.5–2% | 100–300 MB |

These ranges have **no measured baseline** and should not be used as acceptance criteria or promised outcomes. The reported 3.5–7% CPU is user-supplied, not independently verified. No reliable dashboard-specific budget was established. Temporary allocation reductions can improve CPU without materially lowering reported resident memory.

A speculative Python comparison of roughly 1–5% CPU and 100–400 MB with a native GUI was also discussed. No equivalent Python implementation was built or measured. Framework and redraw policy dominate such comparisons; those numbers do not justify a rewrite. Browser/Electron-based implementations generally carry additional runtime memory overhead, but no project-specific comparison was performed.

## Original handoff status

- **Implemented locally:** Windows root-window shadow disable only.
- **Exported:** `windows-overlay-top-border.patch`.
- **Not pushed or merged:** GitHub branch creation was denied by the integration.
- **Not implemented:** all performance recommendations and the bot-only recursion fix.
- **Still required:** Rust build/checks, Windows runtime verification, and profiling before claiming performance improvements.

Source links in this report should be interpreted against the reviewed commit, rather than assuming current `main` is unchanged. Repository paths and function names above identify the reviewed code.
