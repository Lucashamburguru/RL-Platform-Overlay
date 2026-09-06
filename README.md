# RL Platform Overlay

Keep the information you want from Rocket League close at hand. RL Platform
Overlay shows your lobby, ranks/MMR, teammate boost, and session stats while
you play—without making you leave the game.

![Program Preview](assets/program-preview.png)

The overlay is a separate desktop app that reads Rocket League’s built-in Stats
API and draws its own information over the game. A second-monitor dashboard is
available when you want a larger view.

> [!IMPORTANT]
> **Out-of-process design:** The app runs outside Rocket League. It does not
> inject DLLs, read or modify game memory, or hook the renderer. This is an
> intentionally conservative design, but it is not a guarantee about future
> game or anti-cheat policy.

## Quick Start

1. Download the latest version from [Releases](https://github.com/Lucashamburguru/RL-Platform-Overlay/releases) and open it.
2. Open **Setup**, then click **Auto-detect** to find your Rocket League folder.
3. Click **Enable Stats API**. If Rocket League is already open, restart it.
4. Choose your hotkeys and use **Arrange HUD** to place the panels where you want them.
5. Click **Launch Overlay** and start playing.

If Auto-detect cannot find the game, select your Rocket League folder manually.
You can also edit `TAGame/Config/DefaultStatsAPI.ini` yourself and set
`PacketSendRate` to a value above `0`, such as `30.0`.

---

## What You Get

### While you play

* **Lobby overlay:** See player names, platforms, ranks, and MMR without
  Alt-Tabbing.
* **Teammate boost HUD:** Keep an eye on your teammate’s boost with adjustable
  styles, size, and position.
* **Session tracker:** Follow your record, win rate, streak, and play time.
* **Second-monitor dashboard:** Put a larger lobby and session view on another
  screen.
* **Arrangeable panels:** Move the panels where they work best, then keep them
  click-through while you play.
* **Hotkeys:** Show or hide the HUD and settings from your keyboard or
  controller.

### Replays and local tools

* **Ballchasing integration:** Upload, download, and organize replays through
  ballchasing.com.
* **Hoops replay fixer:** Repair supported legacy Hoops replays, with a backup
  created first.
* **Gold Rush swapper:** Apply the Gold Rush / Alpha Boost look locally.

The app changes local Rocket League files only when you choose the Gold Rush
swapper or Hoops replay fixer.

---

## Screenshots

Use the compact overlay during a match, or keep the dashboard open on another
monitor for a roomier view.

![Overlay Preview](assets/overlay-preview-small.png)

![Dashboard Preview](assets/dashboard-preview-small.png)

---

## Help and Support

Something not working as expected? The [support and troubleshooting
guide](docs/support.md) walks through Setup Readiness, connection problems,
incorrect game-mode/team detection, privacy-aware diagnostics, and recent Game
API logs.

---

## Developer Info

If you are a developer, want to compile from source, or want to contribute:

### Tech Stack

* **Language**: Rust
* **UI Framework**: egui / eframe (Glow renderer)
* **Input Hooking**: GilRs (Gamepad) & rdev (Keyboard)
* **Data Sources**: Rocket League Stats API and a pluggable MMR provider.

### Project Documentation

* [Architecture](docs/architecture.md)
* [Rocket League Stats API notes](docs/API/stats-api.md)
* [Support and troubleshooting](docs/support.md)
* [Release process](docs/releasing.md)
* [Security advisory policy](docs/security-advisories.md)

### Build from Source

Ensure you have the Rust toolchain installed.

#### Windows Build Dependencies

Building on Windows requires **CMake**, **NASM** (Netwide Assembler), and **LLVM** for `libclang`, which is used by `bindgen` while compiling BoringSSL/wreq dependencies.

You can install them with `winget`:

```powershell
winget install Kitware.CMake
winget install NASM.NASM
winget install LLVM.LLVM
```

Set `LIBCLANG_PATH` to your LLVM bin folder, for example `C:\Program Files\LLVM\bin`, then restart your terminal.

> [!NOTE]
> This project is mostly developed and tested on Linux. Windows builds may need extra packages, Visual Studio Build Tools, or local environment tweaks.

#### Build Command

```bash
cargo build --locked --release
```

### Running in Debug Mode

You can run the application with the `--debug` command-line flag to expose a dedicated **Debug** tab inside the settings interface (useful for inspecting raw packet data, process logs, and network state).

Windows compiled binary:

```powershell
.\rl-platform-overlay.exe --debug
```

Linux compiled binary:

```bash
./rl-platform-overlay --debug
```

From source:

```bash
cargo run --locked -- --debug
```

### Debug Capture

To save raw game output for parser debugging:

```bash
cargo run --locked --bin debug_game_output -- --seconds 30 --output rl_game_output_debug.txt
```

### Reporting Stats API Detection Issues

The app can save recent Game API events after a detection problem occurs, so a
developer capture does not normally need to be started in advance. See
[Support and troubleshooting](docs/support.md#the-game-mode-teams-or-match-state-is-wrong).

---

## AI Disclosure

This project was developed and refactored with the assistance of **Gemini** and **Codex** AI coding models.

---

## License

MIT
