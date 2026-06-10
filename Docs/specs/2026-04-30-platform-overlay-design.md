# Design Spec: Rocket League Platform Overlay

A lightweight, cross-platform (Linux/Windows) HUD overlay for Rocket League that displays the gaming platform of every player in the current lobby.

## 1. Goal
Provide a non-intrusive, "at-a-glance" way to see which platforms (Steam, Epic, PlayStation, Xbox, Switch) players in a match are using, triggered by a button hold.

## 2. Technical Stack
- **Language:** Rust (for performance and low-level system access).
- **GUI Framework:** `egui` with `eframe` (immediate-mode UI, extremely lightweight).
- **WebSocket:** `tokio-tungstenite` (to ingest the Rocket League Stats API).
- **Input:**
  - `gilrs` (Gamepad Input Library for Rust) for controller support.
  - `rdev` for global keyboard listening.
- **Async Runtime:** `tokio`.

## 3. Architecture

### 3.1 Data Flow
1. **Connection Manager:** Retries connection to `ws://127.0.0.1:49123` until successful.
2. **Parser:** Listens for `UpdateState` events.
   - Extract `Players` array.
   - Parse `PrimaryId` (Format: `Platform|Uid|Splitscreen`).
   - Identify bots (Platform prefix "Bot" or specific ID patterns).
3. **State:** Stores a `HashMap<String, PlayerInfo>` in a thread-safe `Arc<RwLock>`.
4. **Input Thread:** Listens for "Select" (Controller) or "Backspace" (Keyboard). Updates a shared `AtomicBool` for visibility.
5. **UI Loop:** Checks the visibility flag and the current state to render the overlay.

### 3.2 Platform Mapping
| Prefix | Display | Color/Icon |
|--------|---------|------------|
| Steam  | Steam   | Blue-ish   |
| Epic   | Epic    | Purple-ish |
| Ps4/5  | PS      | Blue       |
| Xbox   | Xbox    | Green      |
| Switch | Switch  | Red        |
| Bot    | BOT     | Grey       |

## 4. User Interface (UI)

### 4.1 Window Properties
- **Transparent:** The window background is invisible.
- **Always-on-top:** Stays above the game window.
- **Click-through:** Mouse events pass through to the game.
- **Borderless:** No title bar or window decorations.

### 4.2 Layout
- **Position:** Right-hand side, vertically centered.
- **Structure:**
  - Header: "Lobby Platforms" (small, dimmed).
  - List: Grouped by Team (Blue then Orange).
  - Row: `[Color Block] [Player Name] [Platform Label]`

## 5. Implementation Plan (High Level)
1. Initialize a basic `eframe` window with transparency and click-through.
2. Implement the WebSocket listener task.
3. Implement the global input listener task.
4. Build the platform parsing logic.
5. Design the `egui` rendering loop.

## 6. Constraints & Safety
- **No Injection:** Must not use memory injection or DLL hooks to avoid Anti-Cheat triggers.
- **Performance:** Must use < 1% CPU and minimal RAM (< 50MB).
- **Visibility:** Only visible when the designated key/button is held.
