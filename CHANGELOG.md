# Changelog

This file describes changes to the app.

<!-- Use short sentences and consistent technical terms. Keep release notes about changes that affect users. Put implementation details in commits and review documents. -->

## [Unreleased]

### Added

- **Crash Logs**: The app automatically saves panic reports and rotating runtime logs. Windows exception reports and periodic memory and handle counts help investigate crashes after extended use. Support can copy the log folder path.

### Fixed

- **Windows Builds**: The crash-report test accepts the source path emitted by the Windows compiler. This fixes the failed Windows checks in v0.1.57.
- **Windows Keyboard Listener**: Corrected the message loop so messages arriving after startup do not cause it to fail or stop listening.
- **System Fonts**: Invalid installed fallback fonts are skipped instead of causing a startup panic.

---

## [0.1.57] - 2026-10-06

### Added

- **Crash Logs**: The app automatically saves panic reports and rotating runtime logs. Windows exception reports and periodic memory and handle counts help investigate crashes after extended use. Support can copy the log folder path.

### Fixed

- **Windows Keyboard Listener**: Corrected the message loop so messages arriving after startup do not cause it to fail or stop listening.
- **System Fonts**: Invalid installed fallback fonts are skipped instead of causing a startup panic.

---

## [0.1.56] - 2026-10-04

### Changed

- **Game Mode Detection**: Improved game mode detection.

### Fixed

- **Goal Replays**: Goal replays no longer block live match results. Player statistics update when live play resumes. Saved replay files do not change live results.
- **Your Account**: Camera focus no longer selects your account. Select your player in Setup if the app has no saved account.
- **Free-for-All History**: Bullet Ball and Knockout players count as opponents. Results stay unknown when the API does not identify the winning player.
- **Custom Teams**: Match results use the winning team name. A team can win by forfeit when its score is lower.
- **Bullet Ball Detection**: The app recognizes Bullet Ball from its playlist identifier. The mode stays correct as players leave the arena.

---

## [0.1.55] - 2026-10-04

### Added

- **Dashboard Player Details**: Select a player name to open details below the player. Select the name again to close the details. The table shows rank, MMR, and available match counts for each mode. The current mode uses a highlight color. Details include the recorded peak and season, encounter history, and a link to the player's Tracker profile.
- **App Page Tour**: The README includes an animated tour of the app pages and record details. The tour uses sample data. Setup shows a sample game folder and an active Stats API connection.

### Changed

- **README**: Screenshots appear before Quick Start. Windows instructions identify the executable file to download and open. Setup instructions use the current button name.

### Fixed

- **Dashboard Preview**: Player details stay open when the dashboard shows sample players before a live match.
- **Missing Match Counts**: Unavailable playlist match counts show a dash. A recorded count of zero still shows zero.

---

## [0.1.54] - 2026-10-02

### Added

- **Engine Audio**: You can copy an installed engine sound to another engine in the Engine Audio category of Item Swapper. The app makes backups. You can apply the replacement again or restore the original sound. Packages that the app cannot change show as unavailable.

### Changed

- **Replay Library and Player History**: The tables fill the available space. You can sort the columns and open details below each row. Replay filters show local copies, cloud copies, or both. Dates use the same format. Local and cloud copies of the same replay appear in one row.
- **History Records**: Total wins, losses, and win rate appear beside the match count. Player win rates use colors at all window sizes. The details show teammate and opponent records with win rate bars.
- **Replay Details**: Replay details show a blue and orange scoreboard and player statistics. You can open or close the goal list.

### Fixed

- **Dashboard Players**: The dashboard does not show bots. This includes bots that replace players who leave before the match ends. The dashboard keeps the recorded statistics of players who leave.
- **Shared Boost Sounds**: Standard boost and its painted variants can use the same replacement sound. If you restore one appearance, the other variants keep the replacement sound. The original sound returns when you restore the last replacement.
- **Sound Reapply**: If the source sound changes, the app uses the new sound when you apply a shared boost replacement again.
- **Stats API Setup**: Setup corrects the port number when necessary. The Stats API and overlay use port 49123.
- **Session Results**: If a match reset does not occur, the app clears the previous match state when it finds a new match identifier.
- **Replay Uploads**: Cloud synchronization does not identify changed local replays as files that were already uploaded. The changed files remain available for upload.
- **Lobby Ranks**: The app gets new ratings when the previous ratings expire, including when players stay in the lobby.

---

## [0.1.53] - 2026-09-26

### Added

- **Boost Sounds**: You can keep the original boost sound, use the sound for the new appearance, or select another installed boost sound. You can also change only the sound.

### Changed

- **Gold Rush Audio**: The Gold Rush preset makes its replacement sound from installed game files. The app makes backups of the appearance and sound. It applies and restores them together. Checks include sound banks that more than one boost uses.

### Fixed

- **Dev Boost**: Item Swapper can change encrypted Dev Boost packages with the current catalog key.
- **Alpha Preset Status**: The Gold Rush checkbox is selected only when the boost uses both the Alpha appearance and Alpha sound. Other combinations show as custom replacements.
- **Version Checks**: Version checks correctly compare release tags with four number parts to the equivalent app versions.
- **Build Checks**: Checks include the normal app build and the Microsoft Store build. These checks find Item Swapper errors before release.

---

## [0.1.52] - 2026-09-24

### Added

- **Tracker Peak Rating**: An optional setting shows each player's recorded peak rank and MMR. The display includes the playlist and season when this data is available.

### Changed

- **Rank and MMR Sources**: Tracker Network is the primary data source. If Tracker Network is unavailable, the app uses Rocket League MMR. If peak data is unavailable, the display identifies the current rank.
- **Settings Navigation**: The Overlay tab contains the Lobby, Session, and Boost tabs. Dashboard remains a separate tab.

---

## [0.1.51.0] - 2026-09-23

### Added

- **Item Swapper**: You can browse and search the Rocket League item catalog and change item appearances locally. The catalog includes bodies, wheels, boosts, trails, goal explosions, toppers, antennas, and paint finishes. The app makes replacements from installed game files. Each item has a separate backup of the original file for restoration.
- **Gold Rush Appearance**: The app makes the Alpha Boost appearance from installed UPK files and current package keys. It adjusts to game updates and keeps verified backups.

### Changed

- **Rank and MMR Sources**: If the primary MMR provider temporarily refuses PsyNet access, the app uses Tracker Network. A delay between requests helps keep rank data available.
- **Code Structure**: Separate modules contain network data transport, touch tracking, and Ballchasing cloud synchronization and download services. This change makes code maintenance easier.

### Fixed

- **Release Build**: The UPK parser and encryption code pass the latest Clippy checks. The release build completes successfully.

---

## [0.1.50] - 2026-09-06

### Added

- **Window Controls**: The title bar has buttons to minimize, maximize, restore, and close the settings window. You can also double-click the title bar to maximize or restore the window.
- **Window Size**: You can change the size of the settings window. Use the control at the bottom right or move a window edge.

### Fixed

- **Settings Footer**: Correct layout measurements keep the bottom controls in position. The window size does not change unexpectedly, and the page content stays in view.

---

## [0.1.49] - 2026-09-06

### Added

- **Match History**: You can sort the player history table by any column in ascending or descending order. The columns include games played, win rates, and last seen.
- **Replays Tab**: The Replays tab contains Uploader, Replay Library, and Tools & Maintenance sections. The app verifies the Ballchasing API token while you use the settings.
- **International Fonts**: The app uses system fonts when the primary font cannot show a character. Player names with special characters or Asian scripts appear correctly.
- **Support Guide**: A separate guide describes Setup Readiness, connection problems, mode detection logs, and privacy.

### Changed

- **Rank Data Source**: The app uses a faster and more reliable rank provider. It does not extract MMR data from web pages. This change prevents missing or delayed MMR results.
- **Replay Database**: A separate database contains records of uploaded replays. These records are no longer in the settings file. Settings remain separate, and replay scans stay fast as the replay collection increases.
- **Settings Layout**: The settings layout adjusts to smaller windows. Sliders show percentages. Setup Readiness appears at the top of the Setup tab.
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

- **Support Data Privacy**: By default, support bundles remove personal paths, player details, account details, match identifiers, replay identifiers, filenames, and recent logs. You can examine the exact content before you copy it. An option lets you include identifiable details.

### Fixed

- **Local Rank Updates**: Rank requests do not run at the same time. If the local player changes during a request, the app does not use the previous account's result.
- **Team Assignment**: Missing or invalid game data does not assign players to Blue. The app keeps known team assignments when updates are incomplete. Players with unknown teams remain separate.

---

## [0.1.47] - 2026-08-27

### Changed

- **Player Tracking**: Players with the same name remain separate. The app does not assign delayed rank results to the wrong player.

### Fixed

- **Replay Files**: The app rejects corrupt or invalid replay files before upload, download, replacement, or Hoops repair.
- **Hoops Repair**: The repair tool changes only recognized legacy Hoops replays. It verifies backups before it replaces or restores files.

---

## [0.1.46] - 2026-08-27

### Added

- **Setup Readiness**: Setup shows the status of the installation, Stats API configuration, restart, connection, and live data.
- **Arrange HUD**: You can move HUD panels. The controls include Done, Cancel, and Reset All.
- **Release Notes**: The update tool shows the release notes before it installs an update.

### Changed

- **Display Performance**: The dashboard and Replay Library draw faster, especially with large replay collections.
- **Dashboard Layout**: The layout adjusts to narrow windows. Event Feed is now Match Highlights. HUD colors are more consistent.

### Fixed

- **Overlay Start**: If the Stats API is disabled, the overlay start control returns you to Setup. Setup shows instructions.
- **Team Names**: Long club names do not move dashboard status details outside the window.
- **Replay Metadata**: The app rejects damaged replay headers and replay headers with unreasonable values.

---

## [0.1.45] - 2026-08-26

### Note

- **Unpublished Release**: The continuous integration checks failed for this tag. Thus, the tag did not produce release files. Release `0.1.46` includes all changes that were planned for this release.

---

## [0.1.44] - 2026-08-25

### Added

- **Club Team Names**: Dashboard score labels and player lists use team names from the Stats API. If the names are unavailable, the display uses Blue and Orange.
- **Replay Metadata**: Replay details include player statistics, goal times, match duration, and match type.

---

## [0.1.40] - 2026-07-07

### Fixed

- **Mode Detection**: Mode detection uses active Stats API players when this data is available. If a player replaces a bot during a 2v2 match, the session mode does not lock to 3v3.
- **Statistics**: Invalid game data does not cause incorrect dashboard scores, percentages, average MMR values, or history totals.

---

## [0.1.39] - 2026-06-30

### Added

- **Replay Touch Counts**: The app subtracts touch counts that occur during goal replays. Dashboard and overlay ball touch counts and car touch counts do not increase because of goal replays.

### Changed

- **Touch Settings**: The Touch Counters and Estimate teammate bumps options are in the Dashboard settings tab. Previously, these options were in the Overlay settings tab. Touch Counters removes duplicate touch counts.

### Fixed

- **Session Mode**: The session mode locks after the first goal. Temporary changes to the lobby players do not change the mode. For example, Twos does not change to Threes.

---

## [0.1.38] - 2026-06-30

### Added

- **Completed Match Statistics**: Player statistics remain on the dashboard until the next match starts. They do not disappear when players leave the lobby after a match.
- **Touch Controls**: Optional controls remove duplicate ball touch counts and car touch counts. An experimental comparison statistic estimates teammate bumps.
- **Steam Player Display**: Steam players have a different display style in the lobby and dashboard.

### Fixed

- **Dashboard Statistics**: Goal replays do not increase touch counts. Local player history and rank displays are more reliable.
- **Dashboard Layout**: The scoreboard and comparison panels stay in alignment when the full-screen mode or monitor changes.

---

## [0.1.17] - 2026-06-05

### Fixed

- **Windows Overlay Borders**: Native window borders and title bar buttons do not return while the overlay runs.

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

- **Windows Overlay Transparency**: The overlay does not become a black full-screen surface after start or a settings change.

---

## [0.1.10] - 2026-06-03

### Added

- **MMR Tracking**: The app detects player ranks more consistently across platforms and teams.

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
- **Background Gamepad Input**: Windows gamepad input works when the app is in the background and the game has focus.

---

## [0.1.6] - 2026-06-03

### Changed

- The code includes general corrections and alignment corrections in ui.rs.

---

## [0.1.5] - 2026-06-02

### Added

- **Teammate Boost Display**: You can select Bars, Circles, Compact, or Numbers for the teammate boost display.
- **Update Checks**: The app automatically checks GitHub releases for new versions.

---

## [0.1.4] - 2026-06-02

### Changed

- The user interface and documentation include improvements.

---

## [0.1.3] - 2026-06-01

### Added

- This release adds teammate HUD overlays, corrections to hotkey focus, full keyboard controls, and improvements to the user interface.
