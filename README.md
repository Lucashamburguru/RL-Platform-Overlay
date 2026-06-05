# RL-Platform-Overlay

![Overlay Preview](assets/hud_preview.png)

A Rocket League overlay for identifying player platforms and teammate boost in real time.

## Features
- Platform detection (Steam, Epic, Xbox, PlayStation, Switch)
- Bot detection for Stats API bot IDs
- Optional teammate boost HUD with multiple display styles
- Hotkey support (Full Keyboard & Controller)
- 200Hz controller polling for responsive input
- Toggle or hold-to-show modes
- Live connection status indicator
- Notification-only update checker for new GitHub releases
- Debug capture tool for saving raw Rocket League Stats API output
- Windows and Linux support

## Initial Setup

You must enable the Stats API in Rocket League before the overlay can receive data:

1.  Navigate to your Rocket League installation folder:
    - **Windows (Epic)**: `C:\Program Files\Epic Games\rocketleague\TAGame\Config\`
    - **Windows (Steam)**: `C:\Program Files (x86)\Steam\steamapps\common\rocketleague\TAGame\Config\`
    - **Linux (Steam)**: `~/.local/share/Steam/steamapps/common/rocketleague/TAGame/Config/`
2.  Open `DefaultStatsAPI.ini`.
3.  Set `PacketSendRate` to a value greater than `0` (e.g., `30.0` or `60.0`).
4.  Restart Rocket League.

## Usage
1. Download or build the executable.
2. Open the settings menu and configure your preferred hotkeys (supports keyboard and gamepads).
3. Move the window to the monitor you play on.
4. Press "Launch Overlay" and you are done.

## Debug Capture
To save raw game output for parser debugging:
```bash
cargo run --bin debug_game_output -- --seconds 30 --output rl_game_output_debug.txt
```

The tool connects to the local Rocket League Stats API and writes payloads plus derived summaries to the output file.

To show the in-app Debug tab:
```bash
cargo run -- --debug
```

## Tech Stack
- **Language**: Rust
- **UI**: egui / eframe
- **Graphics**: Glow renderer through eframe
- **Input**: GilRs (Gamepad) & rdev (Keyboard)
- **Data**: [Rocket League Stats API](https://www.rocketleague.com/en/developer/stats-api)

## Build from source
Ensure you have the Rust toolchain installed.
```bash
cargo build --release
```

## License
MIT
