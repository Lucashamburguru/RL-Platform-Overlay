# Changelog

This file describes changes to the app in each release.

<!-- Use ASD-STE100 descriptive writing rules: short sentences, one topic per paragraph, and consistent technical terms. Keep product names and interface labels unchanged. Describe changes that affect users. Put implementation details in commits and review documents. -->

## [Unreleased]

---

## [0.1.62] - 2026-10-20

### Fixed

- **Certificate Trust**: The app uses an updated root certificate bundle that removes trust in the affected emSign certificate chain. This update resolves the dependency audit failure that blocked v0.1.61 release files.

---

## [0.1.61] - 2026-10-20

### Changed

- **Display Performance**: The overlay copies less player data during live updates and rendering. Replay and history pages reuse search and sort results until their inputs change.
- **Display Refresh**: Unchanged HUDs and dashboards refresh less often. Live data and hotkeys trigger updates. Low-boost pulses and HUD dragging keep a fast refresh rate.
- **Controller Input**: The hotkey listener waits for controller events instead of checking for them every five milliseconds.

### Fixed

- **Rank Lookups**: The app uses the Epic account ID for rank requests and Tracker links when the player name contains asterisks.
- **Dashboard**: A lobby containing only bots shows a waiting message instead of causing the app to close.
- **Windows Overlay Border**: The app disables the window shadow that can draw a thin line at the top of the overlay.

---

## [0.1.60] - 2026-10-15

### Changed

- **Replay Uploads**: The app checks only replays changed in the last 15 minutes for automatic upload. You can upload older replays with bulk upload.
- **Networking**: The app uses the current stable release of the HTTP client. The app uses this client for rank requests, downloads, and replay uploads.
- **Interface**: The overlay and dashboard use an updated GUI framework. The XML dependencies no longer need temporary exceptions for security checks.

### Fixed

- **Release Builds**: Linux and Windows checks now pass. These checks prevented the publication of v0.1.59.
- **Account Selection**: The app continues to respond when you select a different account in Setup.
- **Item Swapper**: Catalog updates cannot replace swap records. The Restore control stays available.
- **Automation**: Automatic chat and Free Play operate only on Windows. Other operating systems cannot identify the foreground application.
- **Replay Detection**: Live statistics and results are available when the app connects to a new live match after replay playback.
- **Match History**: The app saves scores from the final event before it records the match result.
- **Tracker Lookups**: API requests do not include page navigation headers. The HTTP client update added these headers, which caused Tracker to reject requests.

---

## [0.1.58] - 2026-10-06

### Added

- **Crash Logs**: The app automatically saves panic reports and runtime logs. It rotates the runtime logs. Windows exception reports help identify the cause of crashes after long periods of operation. The app also records memory use and handle counts at regular intervals. You can copy the log folder path for support.

### Fixed

- **Windows Builds**: The crash report test accepts the source path from the Windows compiler. This change corrects the failed Windows checks in v0.1.57.
- **Windows Keyboard Listener**: The Windows keyboard listener continues to process messages that arrive after the app starts.
- **System Fonts**: The app does not use invalid system fonts for replacement characters. These fonts no longer cause a panic when the app starts.

---

## [0.1.57] - 2026-10-06

### Added

- **Crash Logs**: The app automatically saves panic reports and runtime logs. It rotates the runtime logs. Windows exception reports help identify the cause of crashes after long periods of operation. The app also records memory use and handle counts at regular intervals. You can copy the log folder path for support.

### Fixed

- **Windows Keyboard Listener**: The Windows keyboard listener continues to process messages that arrive after the app starts.
- **System Fonts**: The app does not use invalid system fonts for replacement characters. These fonts no longer cause a panic when the app starts.

---

## [0.1.56] - 2026-10-04

### Changed

- **Game Mode Detection**: The app identifies game modes more accurately.

### Fixed

- **Goal Replays**: Goal replays do not prevent updates to live match results. Player statistics update when live play starts again. Saved replay files do not change live results.
- **Your Account**: Camera focus does not select your account. You can select your player in Setup if the app has no saved account.
- **Free-for-All History**: The app records Bullet Ball and Knockout players as opponents. Results stay unknown when the API does not identify the winning player.
- **Custom Teams**: Match results use the winning team name. A team can win by forfeit with a lower score.
- **Bullet Ball Detection**: The app identifies Bullet Ball from its playlist identifier. Players who leave the arena do not change the identified mode.

---

## [0.1.55] - 2026-10-04

### Added

- **Dashboard Player Details**: You can select a player name to open or close details below that player. The table shows rank, MMR, and available match counts for each mode. A different color identifies the current mode. The details show the recorded peak, season, and encounter history. The details also include a link to the player's Tracker profile.
- **App Page Tour**: The README includes an animated tour of the app pages and record details. The tour uses sample data. Setup shows a sample game folder and an active Stats API connection.

### Changed

- **README**: Screenshots are before Quick Start. Windows instructions identify the executable file that you can download and open. Setup instructions use the current button name.

### Fixed

- **Dashboard Preview**: Player details stay open when the dashboard shows sample players before a live match.
- **Missing Match Counts**: The app shows a dash when playlist match counts are not available. A recorded count of zero shows zero.

---

## [0.1.54] - 2026-10-02

### Added

- **Engine Audio**: You can copy an installed engine sound to another engine in the Engine Audio category of Item Swapper. The app makes backups. You can apply the replacement again or restore the original sound. The app identifies packages that it cannot change as unavailable.

### Changed

- **Replay Library and Player History**: The tables use all available space. You can sort columns and open details below each row. Replay filters show local copies, cloud copies, or both. Dates use the same format. Local and cloud copies of the same replay are in one row.
- **History Records**: Total wins, losses, and win rate are beside the match count. Colors identify player win rates at all window sizes. The details show teammate and opponent records with win rate bars.
- **Replay Details**: Replay details show a scoreboard with blue and orange teams and player statistics. You can open or close the goal list.

### Fixed

- **Dashboard Players**: The dashboard does not show bots. This includes bots that replace players who leave before the match ends. The dashboard keeps the recorded statistics of players who leave.
- **Shared Boost Sounds**: Standard boost and its painted variants can use the same replacement sound. If you restore one appearance, the other variants keep the replacement sound. The app restores the original sound when you restore the last replacement.
- **Sound Reapply**: If the source sound changes, the app uses the new sound when you apply a shared boost replacement again.
- **Stats API Setup**: Setup corrects the port number when necessary. The Stats API and overlay use port 49123.
- **Session Results**: If a match reset does not occur, the app clears the previous match state when it finds a new match identifier.
- **Replay Uploads**: Cloud synchronization does not identify changed local replays as previously uploaded files. You can upload the changed files.
- **Lobby Ranks**: The app gets new ratings when the previous ratings expire. This update also occurs when players stay in the lobby.

---

## [0.1.53] - 2026-09-26

### Added

- **Boost Sounds**: You can keep the original boost sound or use the sound for the new appearance. You can also select another installed boost sound or change only the sound.

### Changed

- **Gold Rush Audio**: The Gold Rush preset makes its replacement sound from installed game files. The app makes backups of the appearance and sound. The app applies and restores the appearance and sound together. The checks include sound banks that more than one boost uses.

### Fixed

- **Dev Boost**: Item Swapper can change encrypted Dev Boost packages with the current catalog key.
- **Alpha Preset Status**: The Gold Rush checkbox is selected only when the boost uses the Alpha appearance and Alpha sound. The app identifies other combinations as custom replacements.
- **Version Checks**: Version checks correctly compare release tags that have four number parts with the equivalent app versions.
- **Build Checks**: The checks include the normal app build and the Microsoft Store build. These checks find Item Swapper errors before release.

---

## [0.1.52] - 2026-09-24

### Added

- **Tracker Peak Rating**: An optional setting shows each player's recorded peak rank and MMR. The display includes the playlist and season when this data is available.

### Changed

- **Rank and MMR Sources**: Tracker Network is the primary data source. If Tracker Network is unavailable, the app uses Rocket League MMR. If peak data is unavailable, the display identifies the current rank.
- **Settings Navigation**: The Overlay tab contains the Lobby, Session, and Boost tabs. Dashboard stays in a separate tab.

---

## [0.1.51.0] - 2026-09-23

### Added

- **Item Swapper**: You can browse and search the Rocket League item catalog. You can change item appearances locally. The catalog includes bodies, wheels, boosts, trails, goal explosions, toppers, antennas, and paint finishes. The app makes replacements from installed game files. Each item has a separate backup of the original file for restoration.
- **Gold Rush Appearance**: The app makes the Alpha Boost appearance from installed UPK files and current package keys. The app adjusts to game updates and keeps verified backups.

### Changed

- **Rank and MMR Sources**: If the primary MMR provider temporarily refuses PsyNet access, the app uses Tracker Network. A delay between requests helps keep rank data available.
- **Code Structure**: Separate modules contain network data transport, touch tracking, and Ballchasing cloud synchronization and download services. This structure makes code maintenance easier.

### Fixed

- **Release Build**: The UPK parser and encryption code pass the latest Clippy checks. The release build completes successfully.

---

## [0.1.50] - 2026-09-06

### Added

- **Window Controls**: The title bar has buttons to minimize, maximize, restore, and close the settings window. You can also double-click the title bar to maximize or restore the window.
- **Window Size**: You can change the settings window size with the control at the bottom right. You can also move a window edge.

### Fixed

- **Settings Footer**: Correct layout measurements keep the bottom controls in position. The window size does not change unexpectedly. The page content stays in view.

---

## [0.1.49] - 2026-09-06

### Added

- **Match History**: You can sort the player history table by any column in ascending or descending order. The columns include games played, win rates, and last seen.
- **Replays Tab**: The Replays tab contains Uploader, Replay Library, and Tools & Maintenance sections. The app verifies the Ballchasing API token while you use the settings.
- **International Fonts**: The app uses system fonts when the primary font cannot show a character. Player names with special characters or Asian scripts appear correctly.
- **Support Guide**: A separate guide describes Setup Readiness, connection problems, mode detection logs, and privacy.

### Changed

- **Rank Data Source**: The app uses a faster and more reliable rank provider. The app does not get MMR data from web pages. This change prevents missing or delayed MMR results.
- **Replay Database**: A separate database contains records of uploaded replays. The settings file no longer contains these records. Replay scans stay fast as the replay collection increases.
- **Settings Layout**: The settings layout adjusts to smaller windows. Sliders show percentages. Setup Readiness is at the top of the Setup tab.
- **Configuration Permissions**: On Linux and macOS, only the file owner can access configuration files. These permissions protect saved API credentials.

### Fixed

- **Replay Uploads**: Content verification prevents duplicate uploads. Files enter the upload queue only after the file write is complete.
- **History Deletion**: The app deletes match history in the background and shows progress. The user interface continues to respond.
- **MMR Errors**: If an MMR request fails, the local MMR panel shows a Retry control. You can open technical details about the error.

---

## [0.1.48] - 2026-08-29

### Added

- **Support Tab**: You can copy diagnostic data with privacy protection. After a mode or team detection problem, you can save recent Game API events. The saved events cover a maximum of two minutes. The app selects the mode and detection source.

### Changed

- **Support Data Privacy**: Support bundles remove personal paths, player details, account details, match identifiers, and replay identifiers by default. They also remove filenames and recent logs. You can examine the exact content before you copy it. An option lets you include details that identify you.

### Fixed

- **Local Rank Updates**: Rank requests do not run at the same time. If the local player changes during a request, the app does not use the previous account's result.
- **Team Assignment**: Missing or invalid game data does not assign players to Blue. The app keeps known team assignments when updates are incomplete. Players with unknown teams stay separate.

---

## [0.1.47] - 2026-08-27

### Changed

- **Player Tracking**: Players with the same name stay separate. The app does not assign delayed rank results to the wrong player.

### Fixed

- **Replay Files**: The app rejects corrupt or invalid replay files before upload, download, replacement, or Hoops repair.
- **Hoops Repair**: The repair tool changes only recognized legacy Hoops replays. The tool verifies backups before it replaces or restores files.

---

## [0.1.46] - 2026-08-27

### Added

- **Setup Readiness**: Setup shows the installation status, Stats API configuration, restart status, connection status, and live data status.
- **Arrange HUD**: You can move HUD panels. The controls include Done, Cancel, and Reset All.
- **Release Notes**: The update tool shows release notes before it installs an update.

### Changed

- **Display Performance**: The dashboard and Replay Library draw faster, especially with large replay collections.
- **Dashboard Layout**: The layout adjusts to narrow windows. Match Highlights replaces Event Feed. HUD colors are more consistent.

### Fixed

- **Overlay Start**: If the Stats API is disabled, the overlay start control opens Setup. Setup shows instructions.
- **Team Names**: Long club names do not move dashboard status details outside the window.
- **Replay Metadata**: The app rejects damaged replay headers. The app also rejects replay headers with unreasonable values.

---

## [0.1.45] - 2026-08-26

### Note

- **Unpublished Release**: The continuous integration checks failed for this tag. The tag did not produce release files. Release `0.1.46` includes all changes planned for this release.

---

## [0.1.44] - 2026-08-25

### Added

- **Club Team Names**: Dashboard score labels and player lists use team names from the Stats API. If names are unavailable, the display uses Blue and Orange.
- **Replay Metadata**: Replay details include player statistics, goal times, match duration, and match type.

---

## [0.1.40] - 2026-07-07

### Fixed

- **Mode Detection**: The app uses active Stats API players to identify the mode when this data is available. A player who replaces a bot during a 2v2 match does not lock the session mode to 3v3.
- **Statistics**: Invalid game data does not cause incorrect dashboard scores, percentages, average MMR values, or history totals.

---

## [0.1.39] - 2026-06-30

### Added

- **Replay Touch Counts**: The app subtracts touch counts that occur during goal replays. Goal replays do not increase ball touch counts or car touch counts on the dashboard or overlay.

### Changed

- **Touch Settings**: The Dashboard settings tab contains the Touch Counters and Estimate teammate bumps options. These options were previously in the Overlay settings tab. Touch Counters removes duplicate touch counts.

### Fixed

- **Session Mode**: The session mode locks after the first goal. Temporary changes to lobby players do not change the mode. For example, Twos does not change to Threes.

---

## [0.1.38] - 2026-06-30

### Added

- **Completed Match Statistics**: Player statistics stay on the dashboard until the next match starts. The statistics do not disappear when players leave the lobby after a match.
- **Touch Controls**: Optional controls remove duplicate ball touch counts and car touch counts. An experimental comparison statistic estimates teammate bumps.
- **Steam Player Display**: Steam players have a different display style in the lobby and dashboard.

### Fixed

- **Dashboard Statistics**: Goal replays do not increase touch counts. Local player history and rank displays are more reliable.
- **Dashboard Layout**: The scoreboard and comparison panels stay in alignment when the full-screen mode or monitor changes.

---

## [0.1.17] - 2026-06-05

### Fixed

- **Windows Overlay Borders**: Native window borders and title bar buttons do not return while the overlay operates.

---

## [0.1.16] - 2026-06-05

### Fixed

- **Windows Full-Screen Transparency**: Full-screen overlays do not show native window decorations or black backgrounds.

---

## [0.1.15] - 2026-06-05

### Fixed

- **Windows Full-Screen Transparency**: Full-screen overlays do not show a black background.
- **Window Movement**: The custom title bar responds immediately when you move it.

---

## [0.1.14] - 2026-06-05

### Fixed

- **Windows Overlay Transparency**: The overlay does not change to a black full-screen surface after the app starts or after a settings change.

---

## [0.1.10] - 2026-06-03

### Added

- **MMR Tracking**: The app identifies player ranks more consistently across platforms and teams.

---

## [0.1.9] - 2026-06-03

### Added

- **Console Ranks**: Rank tracking includes PlayStation, Xbox, and Switch players.

---

## [0.1.8] - 2026-06-03

### Added

- **Alpha Boost**: An optional local replacement changes the Gold Rush appearance and sound. The app makes backups automatically. You can restore the original files without an internet connection. The controls include safety warnings.

### Fixed

- **Boost Settings Performance**: The Boost tab responds without the previous long delays.

---

## [0.1.7] - 2026-06-03

### Added

- **MMR and Rank Tracking**: The app gets current player MMR and ranks through tracker.gg APIs.
- **Background Gamepad Input**: Windows gamepad input operates when the app is in the background and the game has focus.

---

## [0.1.6] - 2026-06-03

### Changed

- The ui.rs file includes general code corrections and alignment corrections.

---

## [0.1.5] - 2026-06-02

### Added

- **Teammate Boost Display**: You can select Bars, Circles, Compact, or Numbers for the teammate boost display.
- **Update Checks**: The app automatically checks GitHub releases for new versions.

---

## [0.1.4] - 2026-06-02

### Changed

- The user interface and documentation have improvements.

---

## [0.1.3] - 2026-06-01

### Added

- This release adds teammate HUD overlays and full keyboard controls. It also corrects hotkey focus and improves the user interface.
