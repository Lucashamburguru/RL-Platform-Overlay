# Second-Monitor Dashboard Mode

## Summary

Add an optional second-screen dashboard as a separate `egui` viewport/window. The dashboard should run beside the existing in-game overlay instead of replacing it, so a player can keep the current transparent/click-through HUD on the Rocket League monitor while showing a richer match dashboard on another display.

The first version should focus on live match readability: roster, score, boost, ranks/MMR, session stats, local MMR, and recent encounter history. It should avoid becoming a diagnostics console or replay-management surface in the first pass.

## Goals

- Keep the current overlay behavior intact by default.
- Make the dashboard independently enableable, closable, and positionable from settings.
- Render the dashboard in a separate solid, interactive window that is not transparent, click-through, or always-on-top by default.
- Reuse the existing `AppState` data sources instead of adding new polling paths.
- Create dashboard-specific presentation helpers rather than scaling up the compact lobby overlay.
- Keep the initial data model read-only; no new database schema or persistence files should be required.
- Make monitor targeting predictable on Windows and harmless elsewhere.

## Non-Goals

- Do not replace the existing overlay HUD, session HUD, boost HUD, or hotkey behavior.
- Do not add a custom dashboard hotkey in the first version unless the implementation naturally exposes one with little extra risk.
- Do not add dashboard-specific history persistence or replay schema changes.
- Do not make the dashboard a full settings app. It can show status snippets, but settings edits should remain in the existing settings tabs.
- Do not attempt perfect multi-monitor placement on every platform in the first pass.

## Current Code Shape

- `MainApp` owns the primary app viewport lifecycle in `src/ui/app.rs`.
- The primary viewport currently switches between:
  - settings/launcher mode when the overlay is stopped;
  - transparent overlay mode when launched.
- The primary viewport state is tracked by `last_viewport_state`, which combines launch state, settings visibility, mouse passthrough, fullscreen, position, and size.
- `Config` lives in `src/state.rs`, uses `#[serde(default)]`, and is persisted through `AppState::save_config`.
- `Config::monitor_index` exists today, but it does not currently drive overlay placement. The Windows code uses `MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST)`, which means the app targets the monitor nearest the current window rather than the stored index.
- Treat monitor-index support as new/unfinished infrastructure. Do not assume the existing `monitor_index` field works.
- Settings tabs are enumerated by `SettingsTab` and rendered through `src/ui/settings/mod.rs`.
- The live player data already exists in `state.game.players`, local MMR in `state.mmr.local_mmr`, session data in `state.game.session`, and history summaries in `state.history.player_summaries`.
- The lobby overlay already has useful helper logic in `src/ui/lobby_overlay.rs`, especially `preview_lobby_players`, but the dashboard should get its own layout and row model.

## Configuration

Add fields to `Config` with serde defaults:

- `dashboard_enabled: bool`
  - Opens/closes the dashboard viewport.
  - Default: `false`.
- `dashboard_monitor_index: usize`
  - Target display.
  - Default: `0`.
  - Do not reuse the existing `monitor_index` field unless it is first made real and tested. Keeping a dashboard-specific field avoids changing overlay behavior while dashboard placement is being built.
  - A later migration can add "prefer second monitor" behavior once monitor enumeration and user selection are reliable.
- `dashboard_fullscreen: bool`
  - Default: `true`.
- `dashboard_open_with_overlay: bool`
  - When launching the overlay, also enable the dashboard.
  - Default: `false` initially. This avoids surprising existing users with a new window after update.
- `dashboard_keep_overlay_enabled: bool`
  - If true, launching the dashboard does not stop or hide the overlay.
  - Default: `true`.
- Optional later fields:
  - `dashboard_window_size: [f32; 2]`
  - `dashboard_window_position: Option<[f32; 2]>`
  - Defer unless manual window restoration is needed in the first release.

Update `config.example.toml` with commented or default dashboard values so users can discover the feature.

## Runtime State

Extend `MainApp` with dashboard-specific UI state:

- `last_dashboard_viewport_state`
  - Separate from `last_viewport_state`.
  - Tracks enabled state, fullscreen state, monitor target, computed position, and computed size.
- `dashboard_user_closed`
  - Optional in-memory guard if closing the dashboard window should not immediately reopen it while `dashboard_enabled` is still true.
  - Prefer updating `dashboard_enabled = false` when the dashboard viewport close event is detected, if `egui` exposes enough close lifecycle information.
- `dashboard_preview_mode`
  - Optional. The empty state can use live-preview players without persisting a setting.

Do not add new long-lived background threads for the dashboard. It should render from `AppState` snapshots that are already maintained by the stats API, MMR, session, history, and replay systems.

## Viewport Lifecycle

Use a stable child viewport id, for example:

```rust
const DASHBOARD_VIEWPORT_ID: egui::ViewportId =
    egui::ViewportId::from_hash_of("second_monitor_dashboard");
```

Render flow in `MainApp::update`:

1. Load config once near the existing config load path.
2. Compute primary overlay/settings behavior exactly as today.
3. Independently compute `show_dashboard = config.dashboard_enabled`.
4. If `show_dashboard`, call a dashboard viewport render helper after the primary viewport content is built.
5. Schedule repaint based on both overlay animation needs and dashboard live-data needs.

Recommended structure:

```rust
if config.dashboard_enabled {
    render_dashboard_viewport(ctx, &self.state, &config, &mut self.last_dashboard_viewport_state);
}
```

Keep the child viewport implementation isolated so primary window transparency and click-through commands are never sent to the dashboard viewport by accident.

Important lifecycle rules:

- The dashboard viewport should not inherit primary overlay transparency.
- The dashboard should use a solid frame and normal mouse interaction.
- The dashboard should not call `WindowLevel::AlwaysOnTop` unless a future setting explicitly requests it.
- Stopping the overlay should not automatically close the dashboard unless `dashboard_keep_overlay_enabled` is false and the user chose replacement-style behavior.
- Closing the whole app should close all viewports through the existing `should_exit`/close path.
- If `dashboard_open_with_overlay` is enabled, the launch action should set both `is_launched = true` and `dashboard_enabled = true`.

## Monitor Targeting

Create a small helper module or functions, for example `src/ui/monitor.rs`, so monitor lookup does not add more platform code to `MainApp::update`.

This should be implemented as new infrastructure. The existing `Config::monitor_index` should be considered legacy/inert until proven otherwise; it is stored but not consulted by the current overlay placement code.

Suggested model:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MonitorPlacement {
    pub position: egui::Pos2,
    pub size: [f32; 2],
    pub fullscreen: bool,
}
```

Windows behavior:

- Enumerate monitors by index with Win32 APIs.
- Resolve `dashboard_monitor_index`.
- If the index is out of range, fall back to monitor `0`.
- Do not silently jump to monitor `1` unless the UI explicitly offers a "prefer second monitor" option later; surprising monitor movement is worse than opening on the primary display.
- Convert physical pixels to `egui` points using `ctx.pixels_per_point()`.
- For fullscreen dashboard, set outer position to monitor origin and inner size to monitor size.
- For windowed dashboard, position near the monitor center with a stable size such as `1280x720`, clamped to monitor bounds.
- Expose enough monitor metadata for settings/debugging to show "index 0/1/2" plus geometry, because users need to verify which index maps to which physical display.

Non-Windows behavior:

- Keep the setting stored.
- Use `ViewportCommand::Fullscreen(true)` when `dashboard_fullscreen` is true.
- Otherwise use a normal windowed size.
- Document in the UI that monitor index targeting is best-effort outside Windows.

Test the fallback logic as pure functions. Do not unit-test Win32 API calls directly; split selection/clamping from platform enumeration.

## Settings UI

Add a "Dashboard" tab or a "Second Screen" section. A separate tab is cleaner because the existing Lobby tab is already dense.

Changes:

- Add `SettingsTab::Dashboard`.
- Add `src/ui/settings/dashboard.rs`.
- Export `render_dashboard_settings_tab`.
- Insert a `"Dashboard"` selectable tab in `render_settings_tabs`.

Controls:

- Enable Dashboard checkbox.
- Open Dashboard With Overlay checkbox.
- Keep Overlay Enabled checkbox.
- Fullscreen checkbox.
- Monitor selector:
  - Display known monitor count when available.
  - Offer numeric indices as a `ComboBox`.
  - Show the selected monitor geometry when available, for example `1920x1080 at 1920,0`.
  - If monitor enumeration is unavailable, show a numeric control and a short platform status line.
- Optional "Move Dashboard Here" or "Reset Dashboard Window" controls can wait until manual placement exists.

Launch controls:

- The existing "Launch Overlay" button should continue to mean overlay launch.
- If `dashboard_open_with_overlay` is true, launching overlay also enables dashboard.
- If dashboard is enabled while overlay is stopped, show a clear status line that the dashboard is running independently.

Avoid adding a second primary launch button that competes with the overlay launch control. The settings tab should own dashboard enablement.

## Dashboard Module

Create `src/ui/dashboard.rs`.

Public entry points:

```rust
pub(crate) fn render_dashboard_viewport(
    ctx: &egui::Context,
    state: &Arc<AppState>,
    config: &Config,
    viewport_state: &mut Option<DashboardViewportState>,
);

pub(crate) fn render_dashboard(ui: &mut egui::Ui, state: &Arc<AppState>, config: &Config);
```

Keep `render_dashboard` separately callable so it can be previewed in settings or tested at helper level without needing a child viewport.

Suggested helper data:

```rust
pub(crate) struct DashboardPlayerRow {
    pub name: String,
    pub platform: String,
    pub team: i32,
    pub is_local: bool,
    pub is_bot: bool,
    pub boost: i32,
    pub score: i32,
    pub goals: i32,
    pub saves: i32,
    pub touches: i32,
    pub demos: i32,
    pub rank_label: String,
    pub mmr: Option<i32>,
    pub history_summary: Option<PlayerHistorySummary>,
}
```

Helper responsibilities:

- Convert `PlayerInfo` plus local/session/history/MMR snapshots into dashboard rows.
- Group rows into blue/orange/unknown teams.
- Sort rows in a stable way:
  - local player first within their team;
  - then score descending;
  - then name ascending.
- Pick the active playlist/rank from session mode when available.
- Fall back to the most relevant ranked playlist if session mode is unknown.
- Hide bots by default only if existing `show_bots` is false; otherwise include them with clear bot treatment.
- Use `preview_lobby_players` only for the empty/preview state.

## Dashboard UI Layout

The dashboard should feel like a second-monitor match desk: dense, readable at a glance, and stable under live stat changes.

Structure:

- Top band:
  - connection status;
  - active match mode;
  - score line;
  - short match GUID;
  - session W/L, win rate, streak, and current delta if available.
- Main body:
  - two team columns.
  - Each column has a fixed header, team score, and player table.
  - Player rows show:
    - platform icon/text;
    - player name;
    - boost meter and number;
    - score/goals/saves/touches/demos;
    - rank label/MMR;
    - encounter summary.
- Side rail:
  - local player identity and local MMR playlists;
  - recent encounter summaries for current players;
  - replay/upload/session status snippets.
- Empty state:
  - connection/setup state;
  - "waiting for live match" style status;
  - compact preview roster using existing preview players;
  - no giant marketing-style hero.

Visual constraints:

- Solid dark neutral background with restrained accent colors for teams and status.
- Avoid transparency effects from the overlay theme.
- Use stable row heights and column widths so changing numbers do not shift the whole table.
- Make the table readable at 1080p on a second monitor.
- Do not put the entire page inside a card. Use full-window panels and only use cards for repeated player/status items.

## Data Sources

Use existing state snapshots:

- `state.flags.is_connected`
- `state.game.players`
- `state.game.session`
- `state.game.local_player_identity`
- `state.game.local_team`
- `state.mmr.local_mmr`
- `state.history.player_summaries`
- replay/upload status from `state.replays` where useful
- version/update status only if it fits the side rail without distracting from match data

History behavior:

- If `history_enabled` and lobby indicators are enabled, show current summaries.
- If history is disabled, show a muted "history off" status instead of forcing history refreshes from the dashboard.
- Do not make the dashboard trigger broad history refreshes every frame.

MMR behavior:

- Prefer player-provided `PlayerInfo.mmr` for per-row ranks.
- Use local MMR side rail for the local player's broader playlist list.
- Avoid network fetches directly from dashboard rendering.

## Repaint Policy

Extend `schedule_repaint` inputs or add dashboard-aware repaint scheduling.

Dashboard visible should repaint:

- around 16-33ms during active match if boost/live stats are changing;
- around 250ms while connected but idle;
- around 1000ms when disconnected and no spinners/status work is running.

Do not force the primary overlay to repaint faster just because the dashboard exists if child viewport repaint can be requested independently. If `egui` repaint scheduling is global, choose a conservative dashboard cadence such as 100ms unless active boost/player stats are visible.

## Staged Implementation

### Phase 1: Plumbing and Settings

- Add config fields and defaults.
- Update `config.example.toml`.
- Add `SettingsTab::Dashboard`.
- Add dashboard settings tab with enable/fullscreen/monitor/open-with-overlay controls.
- Wire overlay launch to optionally enable dashboard.
- Add focused config compatibility tests.

### Phase 2: Viewport Shell

- Add child viewport id and rendering entry point.
- Create solid dashboard window with placeholder status content.
- Add monitor enumeration/placement helper and fallback tests.
- Verify whether the existing `monitor_index` setting is unused in practice. Leave it alone unless the implementation intentionally fixes overlay monitor selection too.
- Verify primary overlay transparency, click-through, and always-on-top behavior still apply only to the primary overlay.

### Phase 3: Data Model Helpers

- Add dashboard row/team model builders.
- Add tests for:
  - empty live players uses preview rows;
  - local player sorting;
  - score/name ordering;
  - bot filtering follows `show_bots`;
  - unknown/out-of-team players remain visible.

### Phase 4: Full Dashboard UI

- Implement top band, team columns, and side rail.
- Add rank/MMR and history summary rendering.
- Add empty state.
- Tune layout at 1920x1080 and 1280x720.

### Phase 5: Polish and Validation

- Run formatting and tests.
- Manually test:
  - overlay only;
  - dashboard only;
  - overlay plus dashboard;
  - settings visible while dashboard is open;
  - closing dashboard from the window controls;
  - launching/stopping overlay with `dashboard_open_with_overlay`;
  - Windows two-monitor placement.

## Test Plan

- `cargo fmt --check`
- `cargo test --lib`
- full `cargo test`
- Existing localhost socket integration test may need the same permission/path used by the current project test workflow.

Focused unit tests:

- Config defaults deserialize when dashboard fields are absent.
- TOML with dashboard fields round-trips.
- Monitor index fallback:
  - no monitors;
  - one monitor;
  - two monitors with explicit selected index;
  - out-of-range index;
  - windowed size clamping.
- Dashboard row grouping:
  - blue/orange teams;
  - unknown teams;
  - local player priority;
  - score/name sort.
- Dashboard empty state data helper uses preview rows without mutating game state.

Manual checks:

- Primary overlay remains transparent/click-through when launched.
- Dashboard remains solid and clickable.
- Dashboard is not always-on-top unless explicitly added later.
- Settings can be opened and edited while the dashboard is visible.
- Turning dashboard off closes only the dashboard viewport.
- Closing the app exits all viewports.

## Risks and Mitigations

- Risk: Child viewport commands accidentally affect the primary overlay.
  - Mitigation: isolate dashboard viewport rendering and do not reuse primary `last_viewport_state`.
- Risk: Windows monitor placement code grows inside `MainApp::update`.
  - Mitigation: move monitor selection and placement into small helper functions with pure tests.
- Risk: The existing `monitor_index` setting gives a false sense that monitor selection already works.
  - Mitigation: document it as inert/legacy for this work, use a dashboard-specific setting, and only change overlay monitor behavior in a separate explicit fix.
- Risk: Dashboard rendering triggers expensive history or MMR refreshes.
  - Mitigation: render from existing snapshots only; do refreshes from existing settings/workflow paths.
- Risk: Closing a child viewport fights persisted `dashboard_enabled`.
  - Mitigation: decide one close behavior up front. Prefer setting `dashboard_enabled = false` when the dashboard is closed.
- Risk: New settings surprise existing users.
  - Mitigation: default dashboard off and default `dashboard_open_with_overlay` off.
- Risk: Dashboard row order shifts too much during live play.
  - Mitigation: stable sort with local-first/team grouping and fixed row dimensions.

## Open Questions

- Should closing the dashboard window persist `dashboard_enabled = false`, or should it behave like a temporary close until next app launch?
- Should `dashboard_open_with_overlay` default to true after a later release once users know the feature exists?
- Should the dashboard eventually get its own hotkey, or is settings control enough?
- Should the side rail include replay upload details in the first version, or should it stay focused on match/session/history?
- Should the existing inert `monitor_index` field be fixed or removed in a separate cleanup? The dashboard plan assumes separate fields to avoid changing overlay behavior.

## Acceptance Criteria

- Existing overlay launch, stop, hotkeys, click-through, transparency, session HUD, and boost HUD behave as they did before when dashboard settings are off.
- Enabling the dashboard opens a separate solid `egui` viewport.
- The dashboard can be visible at the same time as the overlay.
- The dashboard shows live roster/session/MMR/history data from existing `AppState` snapshots.
- Dashboard monitor fallback is deterministic and covered by tests.
- Old config files without dashboard fields load successfully.
