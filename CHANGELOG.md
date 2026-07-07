# Changelog

All notable changes to this project will be documented in this file.

## [0.1.40] - 2026-07-07

### Fixed
- **Late-Join Mode Detection**: Uses active Stats API players when available so replacing a bot mid-match no longer makes a 2v2 session lock as 3v3.
- **Stats API Numeric Parsing**: Rejects out-of-range numeric payload fields instead of allowing lossy casts or wrapped values for teams, scores, boosts, player stats, replay cloud scores, and match winners.

### Changed
- **Dashboard Math Hardening**: Replaced fragile signed/float dashboard calculations with checked integer helpers for average MMR, comparison edges, possession, shot share, win rate, and history record percentages.
- **Time and Count Bounds**: Saturates large history timestamps and replay cloud counts at their storage limits instead of truncating.

---

## [0.1.39] - 2026-06-30

### Added
- **Replay Touch Deduplication**: Subtracts touch increments during goal replay playback, preventing inflated touch and car-touch stats on the dashboard and overlays.

### Changed
- **Settings tab migration**: Moved the "Touch Counters" (debounce duplicate touches) and "Estimate teammate bumps" options from the Overlay settings tab to the Dashboard settings tab.

### Fixed
- **Session Mode Locking**: Keeps the current match session mode locked (e.g., preventing Twos from switching to Threes) once the first goal of the match is scored, even if players temporarily join the lobby.

---

## [0.1.38] - 2026-06-30

### Added
- **Persistent Dashboard Match Snapshot**: Keeps completed match player stats visible until the next game starts instead of dropping stats when players leave the post-match lobby.
- **Touch Counter Controls**: Added optional duplicate touch debouncing for ball touches and car touches, plus an experimental estimated teammate bumps comparison stat.
- **Replay Upload Cache Sync**: Syncs replay upload cache state when the replays tab is first opened so uploaded status is available immediately.
- **Platform Styling**: Added distinct Steam platform coloring in lobby and dashboard player displays.

### Fixed
- **Dashboard Replay Filtering**: Ignores replay/background postgame `UpdateState` player stats so replay playback no longer inflates touches, car touches, or estimated bump totals after a match ends.
- **Windows Local Player Detection**: Resolves local player hints from Stats API target name or shortcut data, improving session and dashboard local-player detection on Windows.
- **Local History Display**: Prevents local players from showing their own played-with history in lobby overlay and dashboard views.
- **Dashboard Rank Consistency**: Aligns dashboard rank selection with lobby overlay behavior.
- **Dashboard Layout Stability**: Keeps scoreboard/team panels and team comparison widths aligned across fullscreen and monitor changes.

---

## [0.1.17] - 2026-06-05

### Fixed
- **Active Windows Style Enforcement**: Implemented frame-by-frame Win32 style verification to actively strip `WS_CAPTION`, `WS_SYSMENU`, and `WS_THICKFRAME` on Windows. This prevents `winit` or the OS from re-applying native window borders or caption buttons during runtime window management cycles.

---

## [0.1.16] - 2026-06-05

### Fixed
- **Windows Fullscreen Transparency**: Replaced `winit` viewport maximize/size commands with direct Win32 API monitor geometry queries, positioning the borderless window at monitor top-left with a height minus 1 pixel. This prevents Windows from enforcing native window caption decorations or disabling DWM transparency (direct flip black screen).

---

## [0.1.15] - 2026-06-05

### Fixed
- **Windows Fullscreen Transparency**: Replaced native winit fullscreen commands with maximized borderless always-on-top window commands, preventing Windows Fullscreen Optimizations from disabling DWM composition and turning the background black.
- **Custom Title Bar Dragging**: Implemented mouse-down drag detection on custom title bar to ensure immediate OS-level window drag handling.

---

## [0.1.14] - 2026-06-05

### Fixed
- **Windows Overlay Transparency**: Reapplies transparent layered-window settings and repaints on launch/settings transitions to prevent the overlay from appearing as a black fullscreen surface.
- **CI Stability**: Prevents local MMR refresh from panicking when triggered outside a Tokio runtime during synchronous unit tests.

---

## [0.1.10] - 2026-06-03

### Added
- **Flexible Platform Matching**: Broadened MMR tracking support to any human non-bot player regardless of specific platform casing or name differences returned by the client. This ensures players from both teams are processed, preventing any team from being missed.

---

## [0.1.9] - 2026-06-03

### Added
- **Multi-Platform MMR & Rank Tracking**: Added support for fetching and displaying MMR and ranks for console players (`PS4`, `PS5`, `Xbox`, `Switch`) by mapping their platforms to the corresponding tracker.gg endpoints (`psn`, `xbl`, `switch`).

---

## [0.1.8] - 2026-06-03

### Added
- **Alpha Boost (Gold Rush) Swap**: Added a local visual and audio swap capability under the **Boost** settings tab.
  - Toggling the checkbox downloads pre-patched assets from the asset server to cache.
  - Automatically creates backups of original Standard Boost assets inside the local configuration directory (`backups/Boost/`) before overwriting.
  - Restoring defaults can be performed fully offline by unchecking the box.
  - Added warnings about Terms of Service / bannability, and checks if the game is currently running (requires restart).

### Fixed
- **Linux & Windows UI Performance**: Optimized the Rocket League process detection (`is_rocket_league_running`) by caching its result for 2 seconds. This resolves severe immediate-mode rendering lag (frametime spikes) when viewing the Boost tab on Linux and Windows.

---

## [0.1.7] - 2026-06-03

### Added
- **MMR & Rank Tracking**: Integrated real-time player MMR and rank fetching via tracker.gg APIs.
- **Background Gamepad Support**: Fixed Windows gamepad input parsing so controller inputs register properly in the background while the game is focused.

---

## [0.1.6] - 2026-06-03

### Changed
- Miscellaneous cleanup and ui.rs alignment fixes.

---

## [0.1.5] - 2026-06-02

### Added
- **Teammate Boost Customization**: Added layout options for showing teammate boost (Bars, Circles, Compact, Numbers).
- **Update Checker**: Added automated version checking against GitHub releases.

---

## [0.1.4] - 2026-06-02

### Changed
- UX adjustments and documentation updates.

---

## [0.1.3] - 2026-06-01

### Added
- Teammate HUD overlays, hotkey focus fixes, full keyboard integration, and overall UX polish.
