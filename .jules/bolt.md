## 2024-07-06 - [Avoid `.to_lowercase()` in parsing and rendering loops]
**Learning:** High-frequency components (like parsing API responses or rendering lists in immediate-mode UI like egui) should avoid allocating `String`s when comparing values. Memory instructions note `eq_ignore_ascii_case()` and `.as_bytes().windows(len).any(...)` are better choices.
**Action:** Always refactor string allocations for equality or substring checking to use non-allocating alternatives, especially in immediate-mode UIs and API parsing paths.
## 2024-07-06 - [Avoid `.to_lowercase()` in parsing and rendering loops]
**Learning:** High-frequency components (like parsing API responses or rendering lists in immediate-mode UI like egui) should avoid allocating `String`s when comparing values. Memory instructions note `eq_ignore_ascii_case()` and `.as_bytes().windows(len).any(...)` are better choices.
**Action:** Always refactor string allocations for equality or substring checking to use non-allocating alternatives, especially in immediate-mode UIs and API parsing paths.

## 2024-07-06 - [Avoid `.to_lowercase()` in parsing and rendering loops]
**Learning:** High-frequency components (like parsing API responses or rendering lists in immediate-mode UI like egui) should avoid allocating `String`s when comparing values. Memory instructions note `eq_ignore_ascii_case()` and `.as_bytes().windows(len).any(...)` are better choices.
**Action:** Always refactor string allocations for equality or substring checking to use non-allocating alternatives, especially in immediate-mode UIs and API parsing paths.
