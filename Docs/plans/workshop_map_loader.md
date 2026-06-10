# Implementation Plan: Rocket League Workshop Map Loader Integration (Archived)

This plan describes how to integrate RLPeak's **Workshop Map Loader** feature into the egui overlay settings GUI in Rust.

## User Review Required

> [!IMPORTANT]
> - We will run the map downloads in a background tokio thread to prevent freezing the egui settings UI.
> - Rocket League path configuration will be added to the overlay config, with automatic detection for common Linux steam paths (and Windows epic/steam paths).
> - We will add the `sysinfo` crate to dependencies to safely check if Rocket League is running before performing file modifications.

## Proposed Changes

### Build Configuration

#### [MODIFY] [Cargo.toml](file:///home/pengo/Dev/Rocketleaguesoverlay/Cargo.toml)
- Add `sysinfo = "0.33"` to the dependencies list to enable cross-platform process detection.

---

### Core State & Configuration

#### [MODIFY] [state.rs](file:///home/pengo/Dev/Rocketleaguesoverlay/src/state.rs)
- Add the following fields to the `Config` struct:
  - `rocket_league_path: String` (defaults to empty string, but will attempt auto-detection).
  - `active_workshop_map: Option<ActiveMapState>` (tracks the loaded workshop map metadata).
- Define `ActiveMapState` struct to store loaded map details (id, name, author, activation timestamp).
- Update the default configuration logic to auto-detect common Rocket League paths on Linux:
  - `~/.local/share/Steam/steamapps/common/rocketleague`
  - `~/.steam/steam/steamapps/common/rocketleague`
  - `~/.steam/root/steamapps/common/rocketleague`
  - As well as common Windows paths as fallback candidates.

---

### Core Logic

#### [NEW] [workshop.rs](file:///home/pengo/Dev/Rocketleaguesoverlay/src/workshop.rs)
Create a new module to handle the workshop map catalog loading, download caching, preflight checks, installation, and restoration.
- **Types**:
  - `WorkshopMap` - represents a catalog item from the maps index.
  - `DownloadStatus` - enum representing `Idle`, `Downloading(f32)`, `Copying`, `Success(String)`, and `Error(String)`.
- **Functions**:
  - `is_rocket_league_running()`: Uses `sysinfo` to check if `RocketLeague.exe` or `rocketleague` is running.
  - `fetch_maps_catalog()`: Downloads the maps index JSON from `https://api.rlpeak.com/v1/files/Plugins/workshop_map_loader/maps_index.json` using `wreq`.
  - `start_load_map(...)`: Spawns a background tokio task to download map files, check processes, overwrite target `mods/Labs_Utopia_P.upk`, and update configuration.
  - `restore_original_map(...)`: Deletes the `mods/Labs_Utopia_P.upk` file and clears state.

---

### User Interface

#### [MODIFY] [ui.rs](file:///home/pengo/Dev/Rocketleaguesoverlay/src/ui.rs)
- Register a new `SettingsTab::Workshop` in the enum and list it in `render_settings_tabs`.
- Update `MainApp` to hold temporary UI state (loading state, search query, maps list, download status).
- Implement `render_workshop_settings_tab(...)` for path input, active map rendering, maps search list, progress display, and instruction tooltips.

#### [MODIFY] [main.rs](file:///home/pengo/Dev/Rocketleaguesoverlay/src/main.rs)
- Declare `mod workshop;` at the top of the file.
