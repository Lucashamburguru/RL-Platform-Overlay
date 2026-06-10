# Design Spec: Settings & Control Panel Update

An interactive settings window to complement the Rocket League Platform Overlay, allowing for real-time configuration.

## 1. Goal
Provide a user-friendly interface to adjust the overlay's behavior and appearance without editing code or config files.

## 2. Architecture
- **Multi-Window Support:** Use `egui` Viewports to manage two windows from a single Rust process.
- **Shared State:** Expand the `AppState` to include configurable properties.
- **Persistence:** (Future) Save settings to a local `config.toml`.

## 3. Configurable Settings
| Category | Setting | Control Type |
|----------|---------|--------------|
| **Display** | Resolution | Dropdown / Input |
| **Display** | UI Scale | Slider |
| **Input** | Keyboard Trigger | Key Capture / Dropdown |
| **Visuals** | Transparency | Slider (0-255) |
| **Visuals** | Show Bots | Checkbox |

## 4. Interaction Model
1. **Settings Window:** Always interactive, standard OS window behavior.
2. **Overlay Window:** Stays in the background (unless triggered), mouse-passthrough.
3. **Sync:** Changes in the Settings Window (e.g., moving a transparency slider) are reflected instantly in the Overlay Window via the shared `AppState`.

## 5. UI Layout (Settings Window)
- **Tabs:** "General", "Visuals", "Input".
- **Footer:** "Quit" button and status indicator (connected/disconnected from Stats API).

## 6. Implementation Plan
1. Update `AppState` with new config fields.
2. Modify `main.rs` to launch two windows.
3. Implement the Settings Window UI in a new module.
4. Update the Overlay Window rendering to respect the new settings.
