# Design Spec: Persistent HUD & Advanced Settings

Refining the Rocket League Platform Overlay to be more robust, persistent, and configurable for multi-monitor setups.

## 1. Goal
Switch to a persistent window architecture to eliminate creation/destruction overhead and provide advanced positioning controls.

## 2. Updated Architecture
- **Persistent Viewport:** The Overlay window is created once.
- **Content Toggling:** The `is_visible` state only toggles the *rendering* of the player list, not the window itself.
- **Isolated Scaling:** `pixels_per_point` (UI Scale) is applied only to the Overlay viewport.

## 3. New Features
### 3.1 Monitor Selection
- The app will query available monitors (via `ctx.available_rect()`).
- A dropdown in Settings allows selecting which monitor to anchor the overlay to.

### 3.2 Anchored Positioning
- Dropdown with presets: `Top Left`, `Top Right`, `Bottom Left`, `Bottom Right`, `Center Right`.
- Layout logic will calculate the appropriate `egui::Area` anchor based on the selection.

### 3.3 Persistence & Launch
- "Launch Overlay" button in Settings to create the window.
- The window remains open until the Settings window is closed.

## 4. Implementation Details
- **State:** Add `monitor_index` and `anchor_position` to `Config`.
- **UI:** Move the `show_viewport_immediate` call to be conditional on a `is_launched` flag.
- **Rendering:** Use `ctx.set_pixels_per_point(config.ui_scale)` inside the overlay viewport.
