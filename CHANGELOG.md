# Changelog

All notable changes to this project will be documented in this file.

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
