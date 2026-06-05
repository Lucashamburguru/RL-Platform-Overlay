# RL-Platform-Overlay

![Overlay Preview](assets/hud_preview.png)

A Rocket League overlay for identifying player platforms, real-time MMR/ranks, session stats, and teammate boost in real time.

---

## Features

- **Platform & MMR Tracking**: Detects player platforms (Steam, Epic, Xbox, PlayStation, Switch) and queries real-time MMR and rank brackets from tracker.gg.
- **Session Stats Overlay**: Overlay HUD showing session statistics (wins, losses, win rate, win streak, and elapsed time).
- **Cosmetic Swapper (Alpha Boost)**: Built-in Alpha Boost swapper that replaces Standard Boost visual and audio assets with Gold Rush (locally only), complete with cache verification and safe original backup restoration.
- **Teammate Boost HUD**: Float teammate boost indicators with multiple layout and scale customization options.
- **Interactive Drag Positioning**: Arrange HUD panels directly on the screen in drag layout mode, which auto-saves positions and automatically disables when settings are closed to restore game click-through.
- **Clean Borderless Window Integration**: Windows-specific Win32 style verification that strips native caption controls and positions the window `height - 1.0` to bypass Direct Flip black-screen optimizations and hide all native close/minimize/maximize buttons.
- **Hotkey Support (Keyboard & Gamepads)**: Global keyboard hooks (`rdev`) and gamepad polling (`gilrs`) for toggling overlay visibility or settings.
- **Debug Capture Tool**: Saves raw Rocket League Stats API payloads for network and event analysis.
- **Lightweight & High Performance**: Built in native Rust using egui's hardware-accelerated immediate mode GUI, resulting in a tiny memory footprint (under 30MB) and near-zero CPU overhead so your game's FPS remains unaffected.
- **Cross-Platform**: Full Windows and Linux support.

---

## Initial Setup

The overlay communicates with Rocket League's native Stats API. You can enable this automatically from within the application settings:

1. Launch the application (it starts in stopped/launcher mode).
2. Open the **Setup** tab, then enter or click **Auto-detect** to locate your Rocket League directory.
3. Click **Enable Stats API**. The app will automatically configure the required settings in your `DefaultStatsAPI.ini` file.
4. Restart Rocket League if it was already running.

*Alternatively (Manual Setup)*:
If you prefer to configure it manually, navigate to your game installation's `TAGame/Config/` directory, open `DefaultStatsAPI.ini` in a text editor, set `PacketSendRate` to a value greater than `0` (e.g. `30.0`), and restart the game.

---

## Usage

1. Download the pre-built executable from [Releases](https://github.com/Lucashamburguru/RL-Platform-Overlay/releases) or build it from source.
2. Open the settings menu and configure your preferred hotkeys (supports keyboard and gamepads).
3. Move the window to the monitor you play on.
4. Press **Launch Overlay** and you are done.

---

## Debug Capture

To save raw game output for parser debugging:
```bash
cargo run --bin debug_game_output -- --seconds 30 --output rl_game_output_debug.txt
```

The tool connects to the local Rocket League Stats API and writes payloads plus derived summaries to the output file.

### Running in Debug Mode

You can run the application with the `--debug` command-line flag to expose a dedicated **Debug** tab inside the settings interface (useful for inspecting raw packet data, process logs, and network state).

*   **Windows (compiled binary)**:
    *   Open PowerShell or Command Prompt in the folder containing the executable and run:
        ```powershell
        .\rl-platform-overlay.exe --debug
        ```
    *   Alternatively, right-click `rl-platform-overlay.exe`, select **Create shortcut**, right-click the newly created shortcut, select **Properties**, and append ` --debug` to the end of the **Target** field (e.g. `C:\path\to\rl-platform-overlay.exe --debug`).
*   **Linux (compiled binary)**:
    *   Open a terminal in the folder containing the binary and run:
        ```bash
        ./rl-platform-overlay --debug
        ```
*   **If running from source**:
    ```bash
    cargo run -- --debug
    ```

---

## Tech Stack

- **Language**: Rust
- **UI**: egui / eframe (Glow renderer)
- **Input**: GilRs (Gamepad) & rdev (Keyboard)
- **Data**: [Rocket League Stats API](https://www.rocketleague.com/en/developer/stats-api) and tracker.gg HTML scraping.

---

## Build from Source

Ensure you have the Rust toolchain installed.

### Windows Build Dependencies

Building this project from source on Windows requires **CMake**, **NASM** (Netwide Assembler), and **LLVM** (for `libclang` used by `bindgen` to compile BoringSSL/wreq dependencies).

You can install all three dependencies quickly using `winget` (Windows Package Manager):

1. **Install tools via PowerShell**:
   ```powershell
   winget install Kitware.CMake
   winget install NASM.NASM
   winget install LLVM.LLVM
   ```

2. **Configure Environment Variables**:
   - Open the Windows Start menu, search for **"Edit the system environment variables"**, and click it.
   - Click the **"Environment Variables..."** button.
   - Under **User variables** (or System variables), click **"New..."** to add `LIBCLANG_PATH`:
     - **Variable name**: `LIBCLANG_PATH`
     - **Variable value**: `C:\Program Files\LLVM\bin` (or your custom LLVM installation bin path)
   - Click **OK** to save.

3. **Restart your IDE / Terminal**:
   - Close and restart VS Code, command prompt, or PowerShell so that the updated `PATH` and `LIBCLANG_PATH` environment variables are loaded.

> [!NOTE]
> Since this program was primarily developed and tested in Linux, additional packages, build tools (like Visual Studio Build Tools), or configuration steps might be required depending on your local Windows development environment.

Once complete, run the build command:
```bash
cargo build --release
```

---

## AI Disclosure

This project was developed and refactored with the assistance of **Gemini** and **Codex** AI coding models.

## License

MIT
