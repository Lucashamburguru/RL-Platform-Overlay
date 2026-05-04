# RL-Platform-Overlay

A Rocket League overlay for identifying player platforms in real-time.

## Features
- Platform detection (Steam, Epic, Xbox, PlayStation, Switch)
- 200Hz controller polling for responsive hotkeys
- Toggle or hold-to-show modes
- Live connection status indicator
- Windows and Linux support

## Initial Setup

You must enable the Stats API in Rocket League before the overlay can receive data:

1.  Navigate to your Rocket League installation folder.
2.  Open `TAGame\Config\DefaultStatsAPI.ini`.
3.  Set `PacketSendRate` to a value greater than `0` (e.g., `30.0` or `60.0`).
4.  Restart Rocket League.

## Usage
1. Download or build the executable.
2. Open the settings menu and configure hotkeys.
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

## Dependencies (Linux)
- libxkbcommon
- libwayland
- libdbus

## License
MIT
