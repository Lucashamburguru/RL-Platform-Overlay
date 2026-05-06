# RL-Platform-Overlay

![Overlay Preview](assets/hud_preview.png)

A Rocket League overlay for identifying player platforms in real-time.

## Features
- Platform detection (Steam, Epic, Xbox, PlayStation, Switch)
- Hotkey support (Full Keyboard & Controller)
- 200Hz controller polling for responsive input
- Toggle or hold-to-show modes
- Live connection status indicator
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

## Tech Stack
- **Language**: Rust
- **UI**: egui / eframe
- **Graphics**: WGPU
- **Input**: GilRs (Gamepad) & rdev (Keyboard)
- **Data**: [Rocket League Stats API](https://www.rocketleague.com/en/developer/stats-api)

## Build from source
Ensure you have the Rust toolchain installed.
```bash
cargo build --release
```

## License
MIT
