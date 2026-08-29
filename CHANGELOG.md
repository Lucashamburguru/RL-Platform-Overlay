# Changelog

All notable changes to this project will be documented in this file.

<!-- Keep release notes concise and player-facing. Put implementation details in commits and review docs. -->

## [Unreleased]

### Fixed
- **Reliable Local Rank Refreshes**: Prevents overlapping refreshes and discards results from a previous account if the local player changes while a rank lookup is running.

---

## [0.1.47] - 2026-08-27

### Changed
- **More Reliable Player Tracking**: Players with identical names are now kept separate, and delayed rank lookups can no longer attach to the wrong player.

### Fixed
- **Safer Replay Handling**: Corrupt or invalid replay files are now rejected before upload, download, replacement, or Hoops repair.
- **Safer Hoops Repair**: The repair tool now changes only recognized legacy Hoops replays and verifies backups before replacing or restoring files.

---

## [0.1.46] - 2026-08-27

### Added
- **Setup Readiness Checklist**: Setup now shows whether installation, Stats API configuration, restart, connection, and live data are ready.
- **Guided Arrange HUD**: Move HUD panels with clear Done, Cancel, and Reset All controls.
- **In-App Release Notes**: The updater now shows what's new before installing an update.

### Changed
- **Smoother Dashboard and Replay Library**: Improved rendering performance, especially with large replay collections.
- **Clearer Dashboard**: Narrow windows reflow more cleanly, Event Feed is now Match Highlights, and HUD colors are more consistent.

### Fixed
- **Overlay Launch Checks**: If the Stats API is not enabled, launching the overlay now returns you to Setup with guidance.
- **Long Team Names**: Club names no longer push dashboard status details off-screen.
- **Invalid Replay Metadata**: Damaged or unreasonable replay headers are rejected safely.

---

## [0.1.45] - 2026-08-26

### Note
- **Unpublished Release**: The tag did not produce release artifacts because its CI run failed; all intended changes are included in `0.1.46`.

---

## [0.1.44] - 2026-08-25

### Added
- **Club Team Names**: Uses Stats API team names in dashboard score badges and roster panels, while retaining Blue and Orange as safe fallbacks.
- **Detailed Replay Metadata**: Adds player box scores, goal timing, match duration, and match type to replay details.

---

## [0.1.40] - 2026-07-07

### Fixed
- **Late-Join Mode Detection**: Uses active Stats API players when available so replacing a bot mid-match no longer makes a 2v2 session lock as 3v3.
- **Reliable Stats**: Invalid game data can no longer wrap or distort dashboard scores, percentages, MMR averages, or history totals.

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
- **Platform Styling**: Steam players now have distinct styling in the lobby and dashboard.

### Fixed
- **Dashboard Accuracy**: Goal replays no longer inflate touch stats, and local-player history and rank displays are more reliable.
- **Dashboard Layout**: Scoreboard and comparison panels remain aligned across fullscreen and monitor changes.

---

## [0.1.17] - 2026-06-05

### Fixed
- **Windows Overlay Borders**: Prevents native borders and caption buttons from reappearing while the overlay is running.

---

## [0.1.16] - 2026-06-05

### Fixed
- **Windows Fullscreen Transparency**: Prevents native window decorations and black backgrounds in fullscreen overlays.

---

## [0.1.15] - 2026-06-05

### Fixed
- **Windows Fullscreen Transparency**: Prevents fullscreen overlays from showing a black background.
- **Window Dragging**: The custom title bar now responds immediately when dragged.

---

## [0.1.14] - 2026-06-05

### Fixed
- **Windows Overlay Transparency**: Prevents the overlay from becoming a black fullscreen surface after launch or settings changes.

---

## [0.1.10] - 2026-06-03

### Added
- **More Reliable MMR Tracking**: Player ranks are detected more consistently across platforms and teams.

---

## [0.1.9] - 2026-06-03

### Added
- **Console MMR and Ranks**: Added rank tracking for PlayStation, Xbox, and Switch players.

---

## [0.1.8] - 2026-06-03

### Added
- **Alpha Boost Swap**: Added an optional local Gold Rush visual and audio swap with automatic backups, offline restoration, and safety warnings.

### Fixed
- **Boost Settings Performance**: Fixed severe interface lag on the Boost tab.

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
