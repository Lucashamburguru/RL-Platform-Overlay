# Design Spec: Resolution Presets

Adding standard resolution presets to the Rocket League Platform Overlay to improve screen coverage and positioning.

## 1. Goal
Allow users to select their monitor resolution from a list of standard presets to ensure the overlay window spans the correct area.

## 2. Supported Resolutions
- **1080p:** 1920 x 1080
- **1440p:** 2560 x 1440
- **4K:** 3840 x 2160
- **Ultrawide:** 3440 x 1440

## 3. UI Implementation
- A new ComboBox in the "Display" section of the Settings window.
- Selecting a preset updates the `window_size` in the `Config`.

## 4. Technical Detail
- When the resolution is changed, the next time the overlay viewport is rendered/updated, it will request the new size via `with_inner_size`.
