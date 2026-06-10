use crate::json_utils::{bool_field, decode_json_string_value, number_field, string_field};
use crate::session::SessionMode;
use crate::state::{AppState, LocalPlayerIdentity, PlayerInfo};
use crate::stats_api::{StatsApiTransport, TcpJsonSplitter};
use futures_util::StreamExt;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio_tungstenite::connect_async;

async fn simulate_key_tap(key: rdev::Key) -> Result<(), rdev::SimulateError> {
    rdev::simulate(&rdev::EventType::KeyPress(key))?;
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    rdev::simulate(&rdev::EventType::KeyRelease(key))?;
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    Ok(())
}

async fn simulate_auto_key_tap(key: rdev::Key, action: &str) -> bool {
    if !rocket_league_accepts_auto_input() {
        log::info!("{action} skipped: Rocket League is not the foreground window.");
        return false;
    }

    if let Err(error) = simulate_key_tap(key).await {
        log::error!("{action} key simulation failed: {error:?}");
        return false;
    }

    true
}

async fn simulate_auto_key_sequence(sequence: &str, action: &str) {
    let keys = parse_auto_key_sequence(sequence);
    if keys.is_empty() {
        log::error!("{action} skipped: no valid keys in sequence '{sequence}'.");
        return;
    }

    for key in keys {
        if !simulate_auto_key_tap(key, action).await {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(125)).await;
    }
}

fn parse_auto_key_sequence(sequence: &str) -> Vec<rdev::Key> {
    sequence
        .split([',', ' ', '+'])
        .filter_map(parse_auto_key)
        .collect()
}

fn parse_auto_key(token: &str) -> Option<rdev::Key> {
    let mut normalized = token
        .trim()
        .trim_matches(['[', ']'])
        .to_ascii_lowercase()
        .replace(['-', '_'], "");
    if normalized.is_empty() {
        return None;
    }
    if normalized.starts_with("key") && normalized.len() == 4 {
        normalized = normalized[3..].to_string();
    }

    match normalized.as_str() {
        "enter" | "return" => Some(rdev::Key::Return),
        "escape" | "esc" => Some(rdev::Key::Escape),
        "space" => Some(rdev::Key::Space),
        "tab" => Some(rdev::Key::Tab),
        "backspace" => Some(rdev::Key::Backspace),
        "uparrow" | "up" => Some(rdev::Key::UpArrow),
        "downarrow" | "down" => Some(rdev::Key::DownArrow),
        "leftarrow" | "left" => Some(rdev::Key::LeftArrow),
        "rightarrow" | "right" => Some(rdev::Key::RightArrow),
        "0" | "num0" | "key0" => Some(rdev::Key::Num0),
        "1" | "num1" | "key1" => Some(rdev::Key::Num1),
        "2" | "num2" | "key2" => Some(rdev::Key::Num2),
        "3" | "num3" | "key3" => Some(rdev::Key::Num3),
        "4" | "num4" | "key4" => Some(rdev::Key::Num4),
        "5" | "num5" | "key5" => Some(rdev::Key::Num5),
        "6" | "num6" | "key6" => Some(rdev::Key::Num6),
        "7" | "num7" | "key7" => Some(rdev::Key::Num7),
        "8" | "num8" | "key8" => Some(rdev::Key::Num8),
        "9" | "num9" | "key9" => Some(rdev::Key::Num9),
        "kp0" | "numpad0" => Some(rdev::Key::Kp0),
        "kp1" | "numpad1" => Some(rdev::Key::Kp1),
        "kp2" | "numpad2" => Some(rdev::Key::Kp2),
        "kp3" | "numpad3" => Some(rdev::Key::Kp3),
        "kp4" | "numpad4" => Some(rdev::Key::Kp4),
        "kp5" | "numpad5" => Some(rdev::Key::Kp5),
        "kp6" | "numpad6" => Some(rdev::Key::Kp6),
        "kp7" | "numpad7" => Some(rdev::Key::Kp7),
        "kp8" | "numpad8" => Some(rdev::Key::Kp8),
        "kp9" | "numpad9" => Some(rdev::Key::Kp9),
        "kpenter" | "numpadenter" => Some(rdev::Key::KpReturn),
        letter if letter.len() == 1 => match letter.as_bytes()[0] {
            b'a' => Some(rdev::Key::KeyA),
            b'b' => Some(rdev::Key::KeyB),
            b'c' => Some(rdev::Key::KeyC),
            b'd' => Some(rdev::Key::KeyD),
            b'e' => Some(rdev::Key::KeyE),
            b'f' => Some(rdev::Key::KeyF),
            b'g' => Some(rdev::Key::KeyG),
            b'h' => Some(rdev::Key::KeyH),
            b'i' => Some(rdev::Key::KeyI),
            b'j' => Some(rdev::Key::KeyJ),
            b'k' => Some(rdev::Key::KeyK),
            b'l' => Some(rdev::Key::KeyL),
            b'm' => Some(rdev::Key::KeyM),
            b'n' => Some(rdev::Key::KeyN),
            b'o' => Some(rdev::Key::KeyO),
            b'p' => Some(rdev::Key::KeyP),
            b'q' => Some(rdev::Key::KeyQ),
            b'r' => Some(rdev::Key::KeyR),
            b's' => Some(rdev::Key::KeyS),
            b't' => Some(rdev::Key::KeyT),
            b'u' => Some(rdev::Key::KeyU),
            b'v' => Some(rdev::Key::KeyV),
            b'w' => Some(rdev::Key::KeyW),
            b'x' => Some(rdev::Key::KeyX),
            b'y' => Some(rdev::Key::KeyY),
            b'z' => Some(rdev::Key::KeyZ),
            _ => None,
        },
        _ => None,
    }
}

fn rocket_league_accepts_auto_input() -> bool {
    #[cfg(target_os = "windows")]
    {
        is_rocket_league_foreground_window()
    }

    #[cfg(not(target_os = "windows"))]
    {
        true
    }
}

#[cfg(target_os = "windows")]
fn is_rocket_league_foreground_window() -> bool {
    use sysinfo::{Pid, ProcessesToUpdate, System};
    use winapi::um::winuser::{GetForegroundWindow, GetWindowThreadProcessId};

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return false;
        }

        let mut process_id = 0;
        GetWindowThreadProcessId(hwnd, &mut process_id);
        if process_id == 0 {
            return false;
        }

        let pid = Pid::from_u32(process_id);
        let mut system = System::new();
        system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);

        system
            .process(pid)
            .is_some_and(|process| crate::assets::is_rocket_league_name(process.name()))
    }
}

fn handle_match_reset(state: &Arc<AppState>, early_leave: bool) {
    let players = state.players.load();
    let is_online = players.values().any(|p| !p.is_local && !p.is_bot);

    let mut session = (**state.session.load()).clone();
    if early_leave && is_online {
        session.record_early_leave();
    }
    session.handle_reset_event();
    state.session.store(Arc::new(session));

    state.players.store(Arc::new(HashMap::new()));
    state.local_player_name.store(Arc::new("".to_string()));
    state
        .local_team
        .store(crate::state::NO_TEAM, Ordering::SeqCst);
    log::info!("Match ended, clearing player list.");
}

pub async fn start_network_task(state: Arc<AppState>) {
    start_network_task_with_addr(state, "127.0.0.1:49123").await;
}

pub async fn start_network_task_with_addr(state: Arc<AppState>, addr: &str) {
    let url = format!("ws://{addr}");
    log::info!("Connecting to {}...", url);
    loop {
        // Try WebSocket first
        match connect_async(&url).await {
            Ok((mut ws_stream, _)) => {
                log::info!("Connected to Rocket League via WebSocket!");
                state.is_connected.store(true, Ordering::SeqCst);
                update_transport(&state, StatsApiTransport::WebSocket);
                while let Some(msg) = ws_stream.next().await {
                    if let Ok(msg) = msg
                        && let Ok(text) = msg.to_text()
                    {
                        match serde_json::from_str::<Value>(text) {
                            Ok(json) => handle_event(&state, &json),
                            Err(error) => update_parse_error(&state, error.to_string()),
                        }
                    }
                }
                state.is_connected.store(false, Ordering::SeqCst);
                state.players.store(Arc::new(HashMap::new()));
                state.local_player_name.store(Arc::new("".to_string()));
            }
            Err(e) => {
                if format!("{}", e).contains("invalid HTTP version") {
                    log::info!("Detected raw TCP traffic. Switching to TCP mode...");
                    if let Ok(mut stream) = TcpStream::connect(addr).await {
                        log::info!("Connected to Rocket League via TCP!");
                        state.is_connected.store(true, Ordering::SeqCst);
                        update_transport(&state, StatsApiTransport::Tcp);
                        let mut buffer = [0u8; 16384];
                        let mut splitter = TcpJsonSplitter::default();
                        loop {
                            match stream.read(&mut buffer).await {
                                Ok(0) => break, // EOF
                                Ok(n) => {
                                    let text = String::from_utf8_lossy(&buffer[..n]);
                                    for json_str in splitter.push(&text) {
                                        match serde_json::from_str::<Value>(&json_str) {
                                            Ok(json) => handle_event(&state, &json),
                                            Err(error) => {
                                                update_parse_error(&state, error.to_string())
                                            }
                                        }
                                    }
                                }
                                Err(error) => {
                                    let message = format!("TCP read error: {error}");
                                    log::error!("{message}");
                                    update_connection_error(&state, message);
                                    break;
                                }
                            }
                        }
                        state.is_connected.store(false, Ordering::SeqCst);
                        state.players.store(Arc::new(HashMap::new()));
                        state.local_player_name.store(Arc::new("".to_string()));
                    } else {
                        state.is_connected.store(false, Ordering::SeqCst);
                        update_connection_error(&state, "Could not connect via TCP.".to_string());
                    }
                } else {
                    state.is_connected.store(false, Ordering::SeqCst);
                    log::error!("Connection failed: {}. Retrying in 5s...", e);
                    update_connection_error(&state, e.to_string());
                }
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

fn handle_event(state: &Arc<AppState>, json: &Value) {
    let event = json["Event"].as_str().unwrap_or("Unknown");
    update_last_event(state, event);
    match event {
        "UpdateState" => handle_update_state(state, &json["Data"]),
        "MatchEnded" => {
            let local_team = state.local_team.load(Ordering::SeqCst);
            let local_team_hint = (local_team != crate::state::NO_TEAM).then_some(local_team);
            let mut session = (**state.session.load()).clone();
            session.handle_match_ended(&json["Data"], local_team_hint);
            state.session.store(Arc::new(session));

            handle_match_reset(state, false);

            let state_clone = state.clone();
            tokio::spawn(async move {
                let config = state_clone.config.load();
                if config.auto_gg {
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    log::info!("Auto-GG: sending configured key sequence...");
                    simulate_auto_key_sequence(&config.auto_gg_sequence, "Auto-GG").await;
                }

                if config.auto_freeplay {
                    tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
                    log::info!("Auto-Freeplay: Navigating to Free Play...");
                    if !simulate_auto_key_tap(rdev::Key::Escape, "Auto-Freeplay").await {
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                    if !simulate_auto_key_tap(rdev::Key::DownArrow, "Auto-Freeplay").await {
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    if !simulate_auto_key_tap(rdev::Key::Return, "Auto-Freeplay").await {
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    if !simulate_auto_key_tap(rdev::Key::DownArrow, "Auto-Freeplay").await {
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    if !simulate_auto_key_tap(rdev::Key::Return, "Auto-Freeplay").await {
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
                    if !simulate_auto_key_tap(rdev::Key::Return, "Auto-Freeplay").await {
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    if !simulate_auto_key_tap(rdev::Key::Return, "Auto-Freeplay").await {
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    if !simulate_auto_key_tap(rdev::Key::Return, "Auto-Freeplay").await {
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    let _ = simulate_auto_key_tap(rdev::Key::Return, "Auto-Freeplay").await;
                }
            });

            if state.config.load().ballchasing_enabled {
                let state_clone = state.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    crate::replays::trigger_replay_upload(state_clone, false);
                });
            }
        }
        "MatchDestroyed" | "LobbyEntered" => {
            handle_match_reset(state, true);
        }
        _ => log::debug!("Received event: {}", event),
    }
}

fn handle_update_state(state: &Arc<AppState>, data: &Value) {
    let real_data = decode_json_string_value(data);
    let mut target_name_hint = None;
    let mut target_team_hint = None;
    let mut b_has_target = false;

    if let Some(_obj) = real_data.as_object() {
        // Extract local player identity from the game block when available.
        if let Some(game) = real_data.get("game").or_else(|| real_data.get("Game")) {
            b_has_target = bool_field(game, &["bHasTarget", "hasTarget"]).unwrap_or(false);

            if let Some(client) = string_field(game, &["client", "Client"]) {
                state.local_player_name.store(Arc::new(client.to_string()));
            } else if let Some(me) = string_field(game, &["me", "Me"]) {
                state.local_player_name.store(Arc::new(me.to_string()));
            } else if !b_has_target
                && let Some(target) = game.get("target").or_else(|| game.get("Target"))
            {
                if let Some(target_name) = string_field(target, &["Name", "name"]) {
                    target_name_hint = Some(target_name.to_string());
                }

                if let Some(target_team) =
                    number_field(target, &["TeamNum", "teamNum", "Team", "team"])
                {
                    target_team_hint = Some(target_team as u8);
                }
            }
        }
    }

    // Try "Players", "players", and even check if the data IS the player array
    let players_val = real_data
        .get("Players")
        .or_else(|| real_data.get("players"))
        .unwrap_or(&real_data); // Fallback: maybe Data is the array itself

    let mut new_players = HashMap::new();
    let mut player_count = None;
    let current_local_name = state.local_player_name.load();
    let current_local_name = current_local_name.trim();
    let has_known_local_name = !current_local_name.is_empty();
    let target_name_hint_ref = target_name_hint.as_deref().unwrap_or("").trim();
    let current_local_name = if current_local_name.is_empty() {
        target_name_hint_ref
    } else {
        current_local_name
    };
    let mut parsed_local_team = None;
    let mut parsed_local_name = None;
    if let Some(players) = players_val.as_array() {
        let mut parsed_players = 0usize;
        for p in players {
            let name = string_field(p, &["Name", "name"])
                .unwrap_or("")
                .trim()
                .to_string();
            if name.is_empty() {
                continue;
            }
            parsed_players += 1;

            // Check for isLocalPlayer flag
            let mut is_local = p["IsLocalPlayer"].as_bool().unwrap_or(false)
                || p["isLocalPlayer"].as_bool().unwrap_or(false)
                || p["isMe"].as_bool().unwrap_or(false)
                || (!current_local_name.is_empty()
                    && name.eq_ignore_ascii_case(current_local_name));

            // If we are spectating another player, ensure they are not marked as local
            if b_has_target {
                let cached_identity = state.local_player_identity.load();
                if cached_identity.is_known() && !name.eq_ignore_ascii_case(&cached_identity.name) {
                    is_local = false;
                }
            }

            if is_local {
                state.local_player_name.store(Arc::new(name.clone()));
                parsed_local_name = Some(name.clone());
            }

            let primary_id =
                string_field(p, &["PrimaryId", "primaryId", "primary_id"]).unwrap_or("");
            let (platform, is_bot) = parse_platform(primary_id);
            let team = number_field(p, &["TeamNum", "teamNum", "Team", "team"]).unwrap_or(0) as u8;

            if is_local {
                state.local_team.store(team, Ordering::SeqCst);
                parsed_local_team = Some(team);
                let first_known_identity =
                    state.update_local_player_identity(LocalPlayerIdentity {
                        name: name.clone(),
                        primary_id: primary_id.to_string(),
                        platform: platform.clone(),
                    });
                if first_known_identity {
                    crate::mmr::start_local_mmr_refresh(state.clone());
                }
            }

            let boost = number_field(p, &["Boost", "boost"]).unwrap_or(0) as u8;
            let score = number_field(p, &["Score", "score"]).unwrap_or(0) as u32;
            let goals = number_field(p, &["Goals", "goals"]).unwrap_or(0) as u32;
            let saves = number_field(p, &["Saves", "saves"]).unwrap_or(0) as u32;
            let touches = number_field(p, &["Touches", "touches"]).unwrap_or(0) as u32;
            let car_touches =
                number_field(p, &["CarTouches", "carTouches", "car_touches"]).unwrap_or(0) as u32;
            let demos = number_field(p, &["Demos", "demos"]).unwrap_or(0) as u32;

            // Preserve MMR if we already have it
            let previous_players = state.players.load();
            let mmr = previous_players
                .get(&name)
                .and_then(|prev| prev.mmr.clone());

            new_players.insert(
                name.clone(),
                PlayerInfo {
                    name,
                    primary_id: primary_id.to_string(),
                    platform,
                    team,
                    is_bot,
                    is_local,
                    boost,
                    score,
                    goals,
                    saves,
                    touches,
                    car_touches,
                    demos,
                    mmr,
                },
            );
        }
        player_count = Some(parsed_players);
    }

    if !has_known_local_name
        && parsed_local_name.is_none()
        && let Some(target_name) = target_name_hint
    {
        state.local_player_name.store(Arc::new(target_name));
    }

    if state.local_team.load(Ordering::SeqCst) == crate::state::NO_TEAM
        && parsed_local_team.is_none()
        && let Some(target_team) = target_team_hint
    {
        state.local_team.store(target_team, Ordering::SeqCst);
    }

    if !new_players.is_empty() {
        // println!("State Updated: {} players in lobby", new_players.len());
    }
    state.players.rcu(|current_players| {
        let mut final_players = new_players.clone();
        for (name, player) in final_players.iter_mut() {
            if let Some(prev) = current_players.get(name)
                && player.mmr.is_none()
                && prev.mmr.is_some()
            {
                player.mmr = prev.mmr.clone();
            }
        }
        Arc::new(final_players)
    });

    let local_team = state.local_team.load(Ordering::SeqCst);
    let local_team_hint = (local_team != crate::state::NO_TEAM).then_some(local_team);
    let mode_hint = real_data
        .get("Game")
        .or_else(|| real_data.get("game"))
        .and_then(session_mode_hint_from_game);
    let session_mode = SessionMode::infer(mode_hint, player_count);
    let mut session = (**state.session.load()).clone();
    session.handle_update_state(&real_data, local_team_hint, session_mode);
    state.session.store(Arc::new(session));
}

fn session_mode_hint_from_game(game: &Value) -> Option<&str> {
    string_field(
        game,
        &[
            "Arena",
            "arena",
            "Map",
            "map",
            "MapName",
            "mapName",
            "GameMode",
            "gameMode",
            "GameInfo",
            "gameInfo",
            "Playlist",
            "playlist",
            "PlaylistName",
            "playlistName",
            "Mutator",
            "mutator",
            "MutatorName",
            "mutatorName",
            "Rules",
            "rules",
        ],
    )
}

fn update_transport(state: &Arc<AppState>, transport: StatsApiTransport) {
    let mut diagnostics = (**state.network_diagnostics.load()).clone();
    diagnostics.transport = transport;
    diagnostics.last_connection_error.clear();
    state.network_diagnostics.store(Arc::new(diagnostics));
}

fn update_last_event(state: &Arc<AppState>, event: &str) {
    let mut diagnostics = (**state.network_diagnostics.load()).clone();
    diagnostics.last_event = event.to_string();
    diagnostics.last_event_unix_ms = crate::stats_api::now_ms();
    diagnostics.last_parse_error.clear();
    state.network_diagnostics.store(Arc::new(diagnostics));
}

fn update_parse_error(state: &Arc<AppState>, error: String) {
    let mut diagnostics = (**state.network_diagnostics.load()).clone();
    diagnostics.last_parse_error = error;
    state.network_diagnostics.store(Arc::new(diagnostics));
}

fn update_connection_error(state: &Arc<AppState>, error: String) {
    let mut diagnostics = (**state.network_diagnostics.load()).clone();
    diagnostics.last_connection_error = error;
    state.network_diagnostics.store(Arc::new(diagnostics));
}

fn parse_platform(id: &str) -> (String, bool) {
    if id.is_empty() {
        return ("Unknown".to_string(), false);
    }
    if id == "Unknown|0|0" {
        return ("BOT".to_string(), true);
    }
    let parts: Vec<&str> = id.split('|').collect();
    let platform = parts.first().copied().unwrap_or("Unknown");
    match platform {
        "Steam" => ("Steam".to_string(), false),
        "Epic" => ("Epic".to_string(), false),
        "Ps4" | "Ps5" | "PlayStation" | "PSN" => ("PlayStation".to_string(), false),
        "Xbox" | "XBoxOne" | "XBL" => ("Xbox".to_string(), false),
        "Switch" | "Nintendo" => ("Switch".to_string(), false),
        "Bot" => ("BOT".to_string(), true),
        _ => (platform.to_string(), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_parse_platform() {
        assert_eq!(parse_platform("Steam|123|0"), ("Steam".to_string(), false));
        assert_eq!(parse_platform("Epic|456|0"), ("Epic".to_string(), false));
        assert_eq!(
            parse_platform("Ps4|789|0"),
            ("PlayStation".to_string(), false)
        );
        assert_eq!(
            parse_platform("Ps5|012|0"),
            ("PlayStation".to_string(), false)
        );
        assert_eq!(parse_platform("Xbox|345|0"), ("Xbox".to_string(), false));
        assert_eq!(
            parse_platform("Switch|678|0"),
            ("Switch".to_string(), false)
        );
        assert_eq!(parse_platform("Unknown|0|0"), ("BOT".to_string(), true));
        assert_eq!(parse_platform("Bot|0|0"), ("BOT".to_string(), true));
        assert_eq!(
            parse_platform("Unknown|999|0"),
            ("Unknown".to_string(), false)
        );
        assert_eq!(parse_platform(""), ("Unknown".to_string(), false));
        assert_eq!(parse_platform("Steam"), ("Steam".to_string(), false));
        assert_eq!(parse_platform("Epic"), ("Epic".to_string(), false));
    }

    #[test]
    fn test_parse_auto_gg_key_sequences() {
        assert_eq!(
            parse_auto_key_sequence("T,G,G,Enter"),
            vec![
                rdev::Key::KeyT,
                rdev::Key::KeyG,
                rdev::Key::KeyG,
                rdev::Key::Return
            ]
        );
        assert_eq!(
            parse_auto_key_sequence("1,1"),
            vec![rdev::Key::Num1, rdev::Key::Num1]
        );
        assert_eq!(
            parse_auto_key_sequence("KeyT KeyG KeyG Return"),
            vec![
                rdev::Key::KeyT,
                rdev::Key::KeyG,
                rdev::Key::KeyG,
                rdev::Key::Return
            ]
        );
    }

    #[test]
    fn test_update_state_marks_local_player_and_teammate_team() {
        let state = AppState::new();
        let data = json!({
            "players": [
                {
                    "name": "Me",
                    "primaryId": "Steam|1|0",
                    "team": 1,
                    "boost": 33,
                    "isMe": true
                },
                {
                    "name": "Mate",
                    "primaryId": "Epic|2|0",
                    "team": 1,
                    "boost": 88
                },
                {
                    "name": "Opponent",
                    "primaryId": "Xbox|3|0",
                    "team": 0,
                    "boost": 44
                }
            ]
        });

        handle_update_state(&state, &data);

        let players = state.players.load();
        assert_eq!(&**state.local_player_name.load(), "Me");
        assert!(players["Me"].is_local);
        assert_eq!(players["Me"].team, 1);
        assert_eq!(players["Mate"].boost, 88);
        assert!(!players["Opponent"].is_local);
    }

    #[test]
    fn test_update_state_uses_game_target_for_local_player() {
        let state = AppState::new();
        let data = json!({
            "Players": [
                {
                    "Name": "cyberPeng",
                    "PrimaryId": "Steam|76561197981997358|0",
                    "TeamNum": 0,
                    "Boost": 33
                },
                {
                    "Name": "C-Block",
                    "PrimaryId": "Unknown|0|0",
                    "TeamNum": 0,
                    "Boost": 88
                },
                {
                    "Name": "Rainmaker",
                    "PrimaryId": "Unknown|0|0",
                    "TeamNum": 1
                }
            ],
            "Game": {
                "bHasTarget": false,
                "Target": {
                    "Name": "cyberPeng",
                    "Shortcut": 1,
                    "TeamNum": 0
                }
            }
        });

        handle_update_state(&state, &data);

        let players = state.players.load();
        assert_eq!(&**state.local_player_name.load(), "cyberPeng");
        assert_eq!(state.local_team.load(Ordering::SeqCst), 0);
        assert!(players["cyberPeng"].is_local);
        assert_eq!(players["C-Block"].team, 0);
        assert_eq!(players["C-Block"].boost, 88);
    }

    #[test]
    fn test_update_state_uses_cached_local_player_name_without_local_flag() {
        let state = AppState::new();
        state
            .local_player_name
            .store(Arc::new("CachedName".to_string()));
        let data = json!({
            "Players": [
                {
                    "Name": "CachedName",
                    "PrimaryId": "Steam|1|0",
                    "TeamNum": 1
                },
                {
                    "Name": "Opponent",
                    "PrimaryId": "Epic|2|0",
                    "TeamNum": 0
                }
            ]
        });

        handle_update_state(&state, &data);

        let players = state.players.load();
        assert!(players["CachedName"].is_local);
        assert_eq!(state.local_team.load(Ordering::SeqCst), 1);
        assert!(!players["Opponent"].is_local);
    }

    #[test]
    fn test_update_state_in_spectate_mode_does_not_overwrite_local_player() {
        let state = AppState::new();
        state.update_local_player_identity(crate::state::LocalPlayerIdentity {
            name: "MyRealName".to_string(),
            primary_id: "Steam|76561197981997358|0".to_string(),
            platform: "Steam".to_string(),
        });
        state
            .local_player_name
            .store(Arc::new("MyRealName".to_string()));

        let data = json!({
            "Game": {
                "bHasTarget": true,
                "Target": {
                    "Name": "SpectatedPlayer",
                    "TeamNum": 0
                }
            },
            "Players": [
                {
                    "Name": "SpectatedPlayer",
                    "PrimaryId": "Epic|999|0",
                    "TeamNum": 0,
                    "IsLocalPlayer": true
                }
            ]
        });

        handle_update_state(&state, &data);

        let players = state.players.load();
        // The spectated player should not be marked as local
        assert!(!players["SpectatedPlayer"].is_local);
        // The local player identity should still be MyRealName
        assert_eq!(state.local_player_identity.load().name, "MyRealName");
        assert_eq!(**state.local_player_name.load(), "MyRealName");
    }

    #[test]
    fn test_lobby_event_clears_websocket_state() {
        let state = AppState::new();
        handle_update_state(
            &state,
            &json!({
                "Players": [
                    {
                        "Name": "Me",
                        "PrimaryId": "Steam|1|0",
                        "TeamNum": 0,
                        "IsLocalPlayer": true
                    }
                ]
            }),
        );
        state.is_connected.store(true, Ordering::SeqCst);

        handle_event(&state, &json!({ "Event": "LobbyEntered" }));

        assert!(state.players.load().is_empty());
        assert_eq!(&**state.local_player_name.load(), "");
    }

    #[test]
    fn test_lobby_event_keeps_local_identity_for_manual_mmr_refresh() {
        let state = AppState::new();
        handle_update_state(
            &state,
            &json!({
                "Players": [
                    {
                        "Name": "Me",
                        "PrimaryId": "Steam|76561198000000000|0",
                        "TeamNum": 0,
                        "IsLocalPlayer": true
                    }
                ]
            }),
        );

        handle_event(&state, &json!({ "Event": "LobbyEntered" }));

        let identity = state.local_player_identity.load();
        assert_eq!(identity.name, "Me");
        assert_eq!(identity.platform, "Steam");
        assert_eq!(identity.primary_id, "Steam|76561198000000000|0");

        let config = state.config.load();
        assert_eq!(config.cached_local_player_identity.name, "Me");
        assert_eq!(config.cached_local_player_identity.platform, "Steam");
        assert_eq!(
            config.cached_local_player_identity.primary_id,
            "Steam|76561198000000000|0"
        );
    }

    #[test]
    fn test_early_leave_online_match_records_loss() {
        let state = AppState::new();
        handle_update_state(
            &state,
            &json!({
                "MatchGuid": "guid123",
                "Players": [
                    {
                        "Name": "Me",
                        "PrimaryId": "Steam|1|0",
                        "TeamNum": 0,
                        "IsLocalPlayer": true,
                        "Boost": 100
                    },
                    {
                        "Name": "Opponent",
                        "PrimaryId": "Epic|2|0",
                        "TeamNum": 1,
                        "Boost": 100
                    }
                ],
                "Game": {
                    "Teams": [
                        {"TeamNum": 0, "Score": 0},
                        {"TeamNum": 1, "Score": 0}
                    ]
                }
            }),
        );

        assert_eq!(state.session.load().active_match_id, "guid123");

        handle_event(&state, &json!({ "Event": "LobbyEntered" }));

        let session = state.session.load();
        assert_eq!(session.losses, 1);
        assert_eq!(session.matches_played, 1);
        assert_eq!(session.last_result, crate::session::MatchResult::Loss);
        assert_eq!(
            session.mode_records[&crate::session::SessionMode::Ones].losses,
            1
        );
    }

    #[test]
    fn test_early_leave_offline_match_ignored() {
        let state = AppState::new();
        handle_update_state(
            &state,
            &json!({
                "MatchGuid": "guid123",
                "Players": [
                    {
                        "Name": "Me",
                        "PrimaryId": "Steam|1|0",
                        "TeamNum": 0,
                        "IsLocalPlayer": true
                    },
                    {
                        "Name": "Bot1",
                        "PrimaryId": "Unknown|0|0",
                        "TeamNum": 1
                    }
                ],
                "Game": {
                    "Teams": [
                        {"TeamNum": 0, "Score": 0},
                        {"TeamNum": 1, "Score": 0}
                    ]
                }
            }),
        );

        handle_event(&state, &json!({ "Event": "LobbyEntered" }));

        let session = state.session.load();
        assert_eq!(session.losses, 0);
        assert_eq!(session.matches_played, 0);
    }

    #[test]
    fn test_update_state_infers_session_mode_from_total_players() {
        let state = AppState::new();
        handle_update_state(
            &state,
            &json!({
                "MatchGuid": "guid123",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true},
                    {"Name": "Mate", "PrimaryId": "Epic|2|0", "TeamNum": 0},
                    {"Name": "Opp1", "PrimaryId": "Xbox|3|0", "TeamNum": 1},
                    {"Name": "Bot1", "PrimaryId": "Unknown|0|0", "TeamNum": 1}
                ]
            }),
        );

        assert_eq!(
            state.session.load().active_mode,
            crate::session::SessionMode::Twos
        );
    }

    #[test]
    fn test_update_state_without_players_uses_unknown_session_mode() {
        let state = AppState::new();
        handle_update_state(
            &state,
            &json!({
                "MatchGuid": "guid123",
                "Game": {
                    "Teams": [
                        {"TeamNum": 0, "Score": 2},
                        {"TeamNum": 1, "Score": 1}
                    ],
                    "bHasWinner": true,
                    "Winner": "Blue"
                }
            }),
        );

        let session = state.session.load();
        assert_eq!(session.wins, 0);
        assert!(session.mode_records.is_empty());
        assert_eq!(session.active_mode, crate::session::SessionMode::Unknown);
    }

    #[test]
    fn test_update_state_prefers_arena_mode_over_player_count() {
        let state = AppState::new();
        handle_update_state(
            &state,
            &json!({
                "MatchGuid": "guid123",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true},
                    {"Name": "Mate", "PrimaryId": "Epic|2|0", "TeamNum": 0},
                    {"Name": "Opp1", "PrimaryId": "Xbox|3|0", "TeamNum": 1},
                    {"Name": "Opp2", "PrimaryId": "Ps4|4|0", "TeamNum": 1}
                ],
                "Game": {
                    "Arena": "HoopsStadium_P",
                    "Teams": [
                        {"TeamNum": 0, "Score": 4},
                        {"TeamNum": 1, "Score": 2}
                    ],
                    "bHasWinner": true,
                    "Winner": "Blue"
                }
            }),
        );

        let session = state.session.load();
        assert_eq!(session.active_mode, crate::session::SessionMode::Hoops);
        assert_eq!(
            session.mode_records[&crate::session::SessionMode::Hoops].wins,
            1
        );
    }

    #[test]
    fn test_offline_extra_mode_does_not_fall_back_to_ones() {
        let state = AppState::new();
        handle_update_state(
            &state,
            &json!({
                "MatchGuid": "guid123",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true},
                    {"Name": "BotA", "PrimaryId": "Unknown|0|0", "TeamNum": 0},
                    {"Name": "BotB", "PrimaryId": "Unknown|0|0", "TeamNum": 1},
                    {"Name": "BotC", "PrimaryId": "Unknown|0|0", "TeamNum": 1}
                ],
                "Game": {
                    "GameInfo": "GameInfo_Basketball.GameInfo.GameInfo_Basketball:Archetype"
                }
            }),
        );

        assert_eq!(
            state.session.load().active_mode,
            crate::session::SessionMode::Hoops
        );
    }

    #[test]
    fn test_standard_offline_uses_total_player_count_for_mode() {
        let state = AppState::new();
        handle_update_state(
            &state,
            &json!({
                "MatchGuid": "guid123",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true},
                    {"Name": "BotA", "PrimaryId": "Unknown|0|0", "TeamNum": 1}
                ],
                "Game": {
                    "Arena": "Stadium_P"
                }
            }),
        );

        assert_eq!(
            state.session.load().active_mode,
            crate::session::SessionMode::Ones
        );

        handle_update_state(
            &state,
            &json!({
                "MatchGuid": "guid456",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true},
                    {"Name": "BotA", "PrimaryId": "Unknown|0|0", "TeamNum": 0},
                    {"Name": "BotB", "PrimaryId": "Unknown|0|0", "TeamNum": 1},
                    {"Name": "BotC", "PrimaryId": "Unknown|0|0", "TeamNum": 1}
                ],
                "Game": {
                    "Arena": "Stadium_P"
                }
            }),
        );

        assert_eq!(
            state.session.load().active_mode,
            crate::session::SessionMode::Twos
        );
    }

    #[test]
    fn test_targeted_opponent_at_match_end_does_not_turn_hoops_loss_into_win() {
        let state = AppState::new();
        handle_update_state(
            &state,
            &json!({
                "MatchGuid": "guid123",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true},
                    {"Name": "Mate", "PrimaryId": "Epic|2|0", "TeamNum": 0},
                    {"Name": "Opp1", "PrimaryId": "Xbox|3|0", "TeamNum": 1},
                    {"Name": "Opp2", "PrimaryId": "Ps4|4|0", "TeamNum": 1}
                ],
                "Game": {
                    "Arena": "HoopsStadium_P",
                    "Teams": [
                        {"TeamNum": 0, "Score": 1},
                        {"TeamNum": 1, "Score": 1}
                    ],
                    "Target": {
                        "Name": "Me",
                        "TeamNum": 0
                    }
                }
            }),
        );

        handle_update_state(
            &state,
            &json!({
                "MatchGuid": "guid123",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0},
                    {"Name": "Mate", "PrimaryId": "Epic|2|0", "TeamNum": 0},
                    {"Name": "Opp1", "PrimaryId": "Xbox|3|0", "TeamNum": 1},
                    {"Name": "Opp2", "PrimaryId": "Ps4|4|0", "TeamNum": 1}
                ],
                "Game": {
                    "Arena": "HoopsStadium_P",
                    "Teams": [
                        {"TeamNum": 0, "Score": 1},
                        {"TeamNum": 1, "Score": 2}
                    ],
                    "bHasWinner": true,
                    "Winner": "Orange",
                    "Target": {
                        "Name": "Opp1",
                        "TeamNum": 1
                    }
                }
            }),
        );

        let session = state.session.load();
        assert_eq!(session.wins, 0);
        assert_eq!(session.losses, 1);
        assert_eq!(session.last_result, crate::session::MatchResult::Loss);
        assert_eq!(
            session.mode_records[&crate::session::SessionMode::Hoops].losses,
            1
        );
    }

    #[test]
    fn test_match_ended_event_records_hoops_loss_from_winner_team_num() {
        let state = AppState::new();
        handle_update_state(
            &state,
            &json!({
                "MatchGuid": "guid123",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true},
                    {"Name": "Mate", "PrimaryId": "Epic|2|0", "TeamNum": 0},
                    {"Name": "Opp1", "PrimaryId": "Xbox|3|0", "TeamNum": 1},
                    {"Name": "Opp2", "PrimaryId": "Ps4|4|0", "TeamNum": 1}
                ],
                "Game": {
                    "Arena": "HoopsStadium_P",
                    "Teams": [
                        {"TeamNum": 0, "Score": 1},
                        {"TeamNum": 1, "Score": 2}
                    ]
                }
            }),
        );

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _guard = runtime.enter();
        handle_event(
            &state,
            &json!({
                "Event": "MatchEnded",
                "Data": {
                    "MatchGuid": "guid123",
                    "WinnerTeamNum": 1
                }
            }),
        );

        let session = state.session.load();
        assert_eq!(session.wins, 0);
        assert_eq!(session.losses, 1);
        assert_eq!(session.matches_played, 1);
        assert_eq!(
            session.mode_records[&crate::session::SessionMode::Hoops].losses,
            1
        );
        assert!(session.active_match_id.is_empty());
    }
}
