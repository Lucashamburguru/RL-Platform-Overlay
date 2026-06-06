use crate::json_utils::{decode_json_string_value, number_field, string_field};
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

pub async fn start_network_task(state: Arc<AppState>) {
    let url = "ws://127.0.0.1:49123";
    let addr = "127.0.0.1:49123";
    println!("Connecting to {}...", url);
    loop {
        // Try WebSocket first
        match connect_async(url).await {
            Ok((mut ws_stream, _)) => {
                println!("Connected to Rocket League via WebSocket!");
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
                    println!("Detected raw TCP traffic. Switching to TCP mode...");
                    if let Ok(mut stream) = TcpStream::connect(addr).await {
                        println!("Connected to Rocket League via TCP!");
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
                                    eprintln!("{message}");
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
                    eprintln!("Connection failed: {}. Retrying in 5s...", e);
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
        "MatchEnded" | "MatchDestroyed" | "LobbyEntered" => {
            state.players.store(Arc::new(HashMap::new()));
            state.local_player_name.store(Arc::new("".to_string()));
            state.local_team.store(255, Ordering::SeqCst);
            let mut session = (**state.session.load()).clone();
            session.handle_reset_event();
            state.session.store(Arc::new(session));
            println!("Match ended, clearing player list.");

            if state.config.load().ballchasing_enabled {
                let state_clone = state.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    crate::replays::trigger_replay_upload(state_clone, false);
                });
            }
        }
        _ => println!("Received event: {}", event),
    }
}

fn handle_update_state(state: &Arc<AppState>, data: &Value) {
    let real_data = decode_json_string_value(data);

    if let Some(_obj) = real_data.as_object() {
        // Extract local player identity from the game block when available.
        if let Some(game) = real_data.get("game").or_else(|| real_data.get("Game")) {
            if let Some(client) = string_field(game, &["client", "Client"]) {
                state.local_player_name.store(Arc::new(client.to_string()));
            } else if let Some(me) = string_field(game, &["me", "Me"]) {
                state.local_player_name.store(Arc::new(me.to_string()));
            } else if let Some(target) = game.get("target").or_else(|| game.get("Target")) {
                if let Some(target_name) = string_field(target, &["Name", "name"]) {
                    state
                        .local_player_name
                        .store(Arc::new(target_name.to_string()));
                }

                if let Some(target_team) =
                    number_field(target, &["TeamNum", "teamNum", "Team", "team"])
                {
                    state.local_team.store(target_team as u8, Ordering::SeqCst);
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
    let current_local_name = state.local_player_name.load();
    let current_local_name = current_local_name.trim();
    if let Some(players) = players_val.as_array() {
        for p in players {
            let name = string_field(p, &["Name", "name"])
                .unwrap_or("")
                .trim()
                .to_string();
            if name.is_empty() {
                continue;
            }

            // Check for isLocalPlayer flag
            let is_local = p["IsLocalPlayer"].as_bool().unwrap_or(false)
                || p["isLocalPlayer"].as_bool().unwrap_or(false)
                || p["isMe"].as_bool().unwrap_or(false)
                || (!current_local_name.is_empty()
                    && name.eq_ignore_ascii_case(current_local_name));

            if is_local {
                state.local_player_name.store(Arc::new(name.clone()));
            }

            let primary_id =
                string_field(p, &["PrimaryId", "primaryId", "primary_id"]).unwrap_or("");
            let (platform, is_bot) = parse_platform(primary_id);
            let team = number_field(p, &["TeamNum", "teamNum", "Team", "team"]).unwrap_or(0) as u8;

            if is_local {
                state.local_team.store(team, Ordering::SeqCst);
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
                    mmr,
                },
            );
        }
    }

    if !new_players.is_empty() {
        // println!("State Updated: {} players in lobby", new_players.len());
    }
    state.players.store(Arc::new(new_players));

    let local_team = state.local_team.load(Ordering::SeqCst);
    let local_team_hint = (local_team != 255).then_some(local_team);
    let mut session = (**state.session.load()).clone();
    session.handle_update_state(&real_data, local_team_hint);
    state.session.store(Arc::new(session));
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
    let platform = parts[0];
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
                "bHasTarget": true,
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
}
