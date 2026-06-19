# Overlay Improvement Roadmap

Date: 2026-06-03

Last updated: 2026-06-03

Scope: root Rust `rl-platform-overlay` app. `RLPeak/` is reference material only.

## Product Direction

Keep the overlay focused on live Rocket League information and lightweight quality-of-life tools:

- platform/lobby identification
- teammate boost
- ranked/MMR context
- session tracking
- safe optional file swaps
- developer diagnostics for Stats API parsing

Do not turn this into a full RLPeak-style item customization client unless that becomes an explicit product goal later.

## Current Status Snapshot

Completed:

- First-run / Setup tab for Stats API config checks and guarded enable action.
- In-app Debug tab capture, hidden behind `--debug`.
- Shared Stats API TCP JSON splitting/capture helpers.
- Basic fixture-style parser/unit coverage for TCP splitting, setup, session state, and file-swap safety.
- Session tab and session overlay for wins, losses, matches played, streak, last result, and score.
- Drag-to-position layout mode for lobby, teammate boost, and session panels.
- Alpha Boost source and safety hardening:
  - fixed GitHub Release asset URLs
  - SHA-256 verification
  - verified cache
  - original backup metadata
  - unknown target file blocking
  - dedicated restore button/status
- Settings hotkey reliability fixes:
  - duplicate key burst debounce
  - egui focused-window path plus global `rdev` fallback
  - hotkey debug logging available from the hidden Debug tab.

Partially complete:

- Connection diagnostics exist in the hidden Debug tab, but "copy debug info" and Rocket League process status are not complete.
- Typed Stats API work is only partially done through shared helpers/tests; parsing still mostly uses flexible `serde_json::Value`.
- File-swap refactor improved Alpha Boost internals, but process/path/download/backup modules are not fully split.

Remaining:

- MMR display controls, MMR cache, encounter history, tracker failure states, and session MMR delta.
- Lobby overlay themes/display modes.
- Optional per-feature hotkeys.
- More real captured Stats API fixtures.

## Near-Term Candidates

### 1. First-Run Stats API Setup

Status: complete.

Problem:

- Users must manually enable Rocket League Stats API in `DefaultStatsAPI.ini`.
- A disconnected overlay does not explain enough about what failed.

Implementation idea:

- Add a setup panel shown on first launch or when disconnected.
- Detect Rocket League install/config paths.
- Check whether `PacketSendRate` is greater than `0`.
- Offer a guarded "Enable Stats API" action.
- Explain that Rocket League must restart after config changes.

Notes:

- This is probably the highest-impact usability improvement.
- Should remain optional and transparent: show what file will be changed.

### 2. Alpha Boost Asset Source and Safety

Status: complete for Alpha Boost v1.

Implemented direction:

- Hosted one-time assets on this project's GitHub Release `alpha-boost-assets-v1`.
- App downloads from fixed GitHub Release URLs.
- App verifies hardcoded SHA-256 hashes before applying.
- App no longer depends on RLPeak API URLs at runtime.

Safety improvements:

- Verify downloaded file hashes before applying. Complete.
- Store backup metadata: original file size, hash, and timestamp. Complete.
- Refuse to overwrite an unknown modified target. Complete.
- Warn while Rocket League is running but allow user action. Complete.
- Add a clearer restore path and status history. Mostly complete.

### 3. Session Stats Overlay

Status: mostly complete.

Shape:

- Add a separate `Session` tab in the settings GUI.
- Include enable/disable controls.
- Include preview in settings.
- Include offsets/scale/anchor or future drag-position support.

Potential data:

- wins. Complete.
- losses. Complete.
- current streak. Complete.
- matches played this session. Complete.
- last match result. Complete.
- score. Complete.
- MMR before/after where available. Not started.
- MMR delta. Not started.
- tracker sync state. Not started.

Feedback:

- This should be independent from the existing lobby overlay and teammate boost HUD.
- Treat it as a third overlay layer with its own config.
- Start simple: enabled, scale, anchor, x/y offsets, opacity, compact/expanded style.
- Later, merge positioning into a general overlay layout editor.

Implementation concern:

- Need reliable match result detection from Stats API events. Capture fixtures before building too much UI.

### 4. Playlist/Mode-Aware MMR

Status: uncertain because Stats API may not disclose current mode.

Plan:

- Investigate captured payloads for playlist/mode fields.
- If mode is available, show MMR for that playlist first.
- If mode is not available, add a preferred playlist setting.

Fallback:

- Let user choose "Displayed MMR mode": `Best`, `1v1`, `2v2`, `3v3`, etc.
- Keep current "best/highest ranked playlist" behavior as one option.

### 5. Connection Diagnostics

Status: partially complete in hidden Debug tab.

Existing:

- Debug tab already shows overlay state, connection state, local player, local team, player count, version check, config status, and parsed players.

Potential improvements:

- Add Rocket League process running status.
- Add transport type: WebSocket or raw TCP.
- Add last event timestamp.
- Add last event name.
- Add last parse error.
- Add last connection error.
- Add "copy debug info" button.

Implemented:

- transport type
- last event timestamp/name
- last parse error
- last connection error
- hotkey log path/clear action

Remaining:

- Rocket League process running status in Debug
- copy debug info button

Conclusion:

- Keep this in the Debug tab rather than adding a separate diagnostics page.

### 6. Overlay Presets

Status: good future idea.

Possible presets:

- minimal
- compact
- ranked
- high contrast
- RocketStats-inspired
- caster/spectator

Defer until the core overlay layout and session stats are more stable.

### 7. Drag-To-Position Layout Mode

Status: complete for first version.

Goal:

- Let users drag overlay elements directly instead of tuning offsets with sliders.

Potential elements:

- lobby/player overlay
- teammate boost HUD
- session stats overlay

Implementation idea:

- Add "Layout Mode" in settings.
- Temporarily disable click-through.
- Render draggable frames for each enabled overlay.
- Persist normalized positions or pixel offsets per overlay element.

Notes:

- This pairs well with session overlay work.
- Keep slider fallback for precise edits.

### 8. Per-Feature Hotkeys

Status: uncertain.

Current stance:

- Do not prioritize yet.
- Existing HUD/settings hotkeys may be enough.

Possible future hotkeys:

- toggle teammate boost
- toggle session overlay
- cycle preset/theme
- enter layout mode

### 9. Lobby Overlay Themes

Status: good idea.

Current state:

- Teammate boost HUD already has multiple display styles.
- Lobby/player overlay has one primary look.

Plan:

- Add themes/display modes for the lobby overlay, similar to teammate boost.
- Keep bot/platform/MMR/stat rendering consistent across themes.

Possible modes:

- current/default
- compact
- team grouped
- MMR focused
- platform-only

### 10. In-App Debug Capture

Status: complete and hidden behind `--debug`.

Goal:

- Add a Debug tab action to capture raw Stats API output for a fixed duration.

Implementation idea:

- Button: "Capture 30s Stats API Output".
- Save raw payloads plus parsed summaries to a timestamped file.
- Show output path after capture.
- Reuse or integrate behavior from `src/bin/debug_game_output.rs`.

Notes:

- This is valuable for fixture generation and parser debugging.
- It should not clutter normal user-facing tabs.

### 11. Typed Stats API Models

Clarification:

- This is for parsing Rocket League Stats API payloads, not tracker scraping.

Current state:

- `network.rs` uses flexible `serde_json::Value` parsing.

Plan:

- Keep flexible fallback parsing for unknown payload variants.
- Add typed structs for known event/payload shapes.
- Add focused parser functions that are easy to unit test.

Benefits:

- Less fragile parsing.
- Better fixture tests.
- Easier session result detection.

### 12. MMR Cache and Encounter History

Status: not started.

MMR cache:

- Cache tracker results per platform/player ID.
- Use a TTL to avoid stale MMR and reduce rate-limit pressure.
- Persist cache across app launches if useful.

Encounter history:

- Track how often a player has appeared historically.
- Potential UI labels:
  - "seen 3 times"
  - "last seen yesterday"
  - "frequent opponent"

Privacy/locality:

- Keep this local-only.
- Provide a clear reset option if persisted.

### 13. Tracker/MMR Failure States

Status: not started.

Current issue:

- Failed/private/not-found states are not visible enough.

Plan:

- Track MMR fetch state separately from the snapshot.
- Possible states:
  - pending
  - fetching
  - ready
  - private
  - not found
  - rate limited
  - temporary failure
  - unsupported platform

UI:

- Show concise labels instead of repeated "Fetching rank...".
- Avoid retry spam for private/not-found profiles.

### 14. File-Swap Service Refactor

Status: partially complete.

Current issue:

- `assets.rs` still combines process detection, path validation, downloads, backup, restore, and operation status.
- Alpha Boost is much safer now, but the code is not fully split into dedicated modules.

Proposed modules:

- `process.rs` or `rocket_league_process.rs`
- `game_paths.rs`
- `downloads.rs`
- `backup.rs`
- `boost_swap.rs`

Benefits:

- Easier testing.
- Safer future item/file swap features.
- Cleaner UI integration.

### 15. Fixture-Based Parser Tests

Status: partially complete.

Plan:

- Save captured Stats API payloads as fixtures.
- Add parser tests for:
  - normal online match
  - bots
  - private match
  - local player detection
  - match end
  - lobby enter
  - raw TCP chunk boundaries
  - malformed/partial payloads

Notes:

- In-app debug capture should feed this test suite.
- Tests should validate both parsed players and state transitions.

## Suggested Build Order

Completed order:

1. Add in-app debug capture.
2. Add initial parser/unit tests around Stats API helpers.
3. Add first-run Stats API setup/check panel.
4. Add session stats model and basic Session tab.
5. Add session stats overlay preview and enable/disable controls.
6. Add drag-to-position layout mode.
7. Improve Alpha Boost asset source and safety.
8. Hide Debug tab behind `--debug` and add hotkey diagnostics.

Next recommended order:

1. Add MMR cache and tracker failure states.
2. Add MMR display mode setting: `Best`, `1v1`, `2v2`, `3v3`, etc.
3. Add session MMR delta once MMR cache/state is stable.
4. Add encounter history: local seen count and last-seen timestamp.
5. Add lobby overlay themes/display modes.
6. Add more captured Stats API fixtures from real sessions.
7. Continue file-swap module refactor if more swaps are planned.

Reasoning:

- MMR cache/failure states should come before MMR delta so the session overlay has reliable tracker state to compare.
- Lobby themes are user-visible but lower risk/value than making current MMR data more reliable.
- More fixtures should be added opportunistically as real payloads are captured.
- Further file-swap refactor is useful mainly if additional file-swap features are planned.
