# RL-Platform-Overlay

Shows Rocket League lobby info, ranks/MMR, teammate boost, and session stats while you play.

![Program Preview](assets/program-preview.png)

## What It Does

RL-Platform-Overlay is a separate desktop app that reads Rocket League's Stats API output and displays useful match information on top of the game.

It can show:

* Player platforms in the current lobby.
* Ranks and MMR when tracker.gg data is available.
* A teammate boost HUD for live boost tracking.
* Session wins, losses, win rate, streak, and session time.
* A dashboard view for another monitor.

It also includes replay tools:

* Upload finished matches to Ballchasing.com.
* Bulk upload older replay files.
* Repair some broken legacy Hoops replays while keeping a `.replay.bak` backup.
* Locally swap Standard Boost to Gold Rush / Alpha Boost assets, with restore support.

> [!IMPORTANT]
> This app runs outside Rocket League. It does not inject DLLs, modify game memory, or hook the renderer.

---

## Screenshots

The overlay can run directly over Rocket League, or you can keep the dashboard open on another monitor.

![Overlay Preview](assets/overlay-preview-small.png)

![Dashboard Preview](assets/dashboard-preview-small.png)

---

## Quick Start

1. Download the latest build from [Releases](https://github.com/Lucashamburguru/RL-Platform-Overlay/releases).
2. Open the app.
3. Go to **Setup**.
4. Click **Auto-detect** to find your Rocket League folder.
5. Click **Enable Stats API**.
6. Restart Rocket League if it was already running.
7. Set your hotkeys, click **Launch Overlay**, and play.

If auto-detect does not work, edit `TAGame/Config/DefaultStatsAPI.ini` in your Rocket League folder and set `PacketSendRate` to a value above `0`, such as `30.0`.

---

## Main Features

* **Lobby overlay**: Shows player names, platforms, ranks, and MMR without needing to Alt-Tab.
* **Teammate boost HUD**: Displays your teammate's boost with adjustable styles, size, and position.
* **Session tracker**: Tracks your current session record, win rate, streak, and play time.
* **Second-monitor dashboard**: Keeps lobby and session information visible on another screen.
* **Drag layouts**: Move and resize overlay panels, then keep them click-through while playing.
* **Hotkeys**: Toggle the overlay HUD or settings window from keyboard or controller buttons.
* **Replay uploader**: Sends replays to Ballchasing.com after matches, with controls for older replay uploads.
* **Hoops replay fixer**: Patches older broken Hoops replay files in place and saves a backup first.
* **Gold Rush swapper**: Applies the Gold Rush / Alpha Boost look locally and can restore the original assets.
* **Windows and Linux support**: Built as a native Rust app with egui/eframe.

---

## Developer Info

If you are a developer, want to compile from source, or want to contribute:

### Tech Stack

* **Language**: Rust
* **UI Framework**: egui / eframe (Glow renderer)
* **Input Hooking**: GilRs (Gamepad) & rdev (Keyboard)
* **Data Sources**: Rocket League Stats API and tracker.gg HTML scraping.

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
cargo build --release
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
cargo run -- --debug
```

### Debug Capture

To save raw game output for parser debugging:

```bash
cargo run --bin debug_game_output -- --seconds 30 --output rl_game_output_debug.txt
```

---

## AI Disclosure

This project was developed and refactored with the assistance of **Gemini** and **Codex** AI coding models.

---

## License

MIT
