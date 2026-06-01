use crate::state::{AppState, PlayerInfo};
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
                while let Some(msg) = ws_stream.next().await {
                    if let Ok(msg) = msg
                        && let Ok(text) = msg.to_text()
                        && let Ok(json) = serde_json::from_str::<Value>(text)
                        && json["Event"] == "UpdateState"
                    {
                        handle_update_state(&state, &json["Data"]);
                    }
                }
                state.is_connected.store(false, Ordering::SeqCst);
                state.players.store(Arc::new(HashMap::new()));
            }
            Err(e) => {
                if format!("{}", e).contains("invalid HTTP version") {
                    println!("Detected raw TCP traffic. Switching to TCP mode...");
                    if let Ok(mut stream) = TcpStream::connect(addr).await {
                        println!("Connected to Rocket League via TCP!");
                        state.is_connected.store(true, Ordering::SeqCst);
                        let mut buffer = [0u8; 16384];
                        let mut leftover = String::new();
                        loop {
                            match stream.read(&mut buffer).await {
                                Ok(0) => break, // EOF
                                Ok(n) => {
                                    let text = format!(
                                        "{}{}",
                                        leftover,
                                        String::from_utf8_lossy(&buffer[..n])
                                    );
                                    leftover.clear();

                                    // Try to find complete JSON objects in the stream
                                    let mut start = 0;
                                    let mut depth = 0;
                                    let mut in_string = false;
                                    let mut escaped = false;

                                    for (i, c) in text.char_indices() {
                                        if escaped {
                                            escaped = false;
                                            continue;
                                        }
                                        match c {
                                            '\\' => escaped = true,
                                            '"' => in_string = !in_string,
                                            '{' if !in_string => {
                                                if depth == 0 {
                                                    start = i;
                                                }
                                                depth += 1;
                                            }
                                            '}' if !in_string => {
                                                depth -= 1;
                                                if depth == 0 {
                                                    let json_str = &text[start..=i];
                                                    if let Ok(json) =
                                                        serde_json::from_str::<Value>(json_str)
                                                    {
                                                        let event = json["Event"]
                                                            .as_str()
                                                            .unwrap_or("Unknown");
                                                        if event == "UpdateState" {
                                                            handle_update_state(
                                                                &state,
                                                                &json["Data"],
                                                            );
                                                        } else if event == "MatchEnded" || event == "MatchDestroyed" || event == "LobbyEntered" {
                                                            state.players.store(Arc::new(HashMap::new()));
                                                            state.local_player_name.store(Arc::new("".to_string()));
                                                            state.local_team.store(255, Ordering::SeqCst);
                                                            println!("Match ended, clearing player list.");
                                                        } else {
                                                            println!("Received event: {}", event);
                                                        }
                                                    }
                                                }
                                            }
                                            _ => {}
                                        }
                                    }

                                    if depth > 0 {
                                        leftover = text[start..].to_string();
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                        state.is_connected.store(false, Ordering::SeqCst);
                        state.players.store(Arc::new(HashMap::new()));
                    } else {
                        state.is_connected.store(false, Ordering::SeqCst);
                    }
                } else {
                    state.is_connected.store(false, Ordering::SeqCst);
                    eprintln!("Connection failed: {}. Retrying in 5s...", e);
                }
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

fn handle_update_state(state: &Arc<AppState>, data: &Value) {
    // If the data is a string, it might be double-encoded JSON
    let real_data = if let Some(s) = data.as_str() {
        // println!("Detected double-encoded JSON string, parsing internal content...");
        serde_json::from_str::<Value>(s).unwrap_or(data.clone())
    } else {
        data.clone()
    };

    if let Some(_obj) = real_data.as_object() {
        // Extract local player name
        if let Some(game) = real_data.get("game") {
            if let Some(client) = game["client"].as_str() {
                state.local_player_name.store(Arc::new(client.to_string()));
            } else if let Some(me) = game["me"].as_str() {
                state.local_player_name.store(Arc::new(me.to_string()));
            }
        }
    }
    
    let local_name = state.local_player_name.load();

    // Try "Players", "players", and even check if the data IS the player array
    let players_val = real_data
        .get("Players")
        .or_else(|| real_data.get("players"))
        .unwrap_or(&real_data); // Fallback: maybe Data is the array itself

    let mut new_players = HashMap::new();
    if let Some(players) = players_val.as_array() {
        for p in players {
            let name = p["Name"].as_str().unwrap_or("").trim().to_string();
            if name.is_empty() {
                continue;
            }
            
            // Check for isLocalPlayer flag
            let is_local_flag = p["IsLocalPlayer"].as_bool().unwrap_or(false) || p["isMe"].as_bool().unwrap_or(false);
            if is_local_flag {
                state.local_player_name.store(Arc::new(name.clone()));
            }

            let primary_id = p["PrimaryId"].as_str().unwrap_or("");
            let (platform, is_bot) = parse_platform(primary_id);
            let team = p["TeamNum"]
                .as_u64()
                .or_else(|| p["Team"].as_u64())
                .unwrap_or(0) as u8;
                
            if is_local_flag {
                state.local_team.store(team, Ordering::SeqCst);
            }
            let boost = p["Boost"].as_u64().unwrap_or(0) as u8;
            let score = p["Score"].as_u64().unwrap_or(0) as u32;
            let goals = p["Goals"].as_u64().unwrap_or(0) as u32;
            let saves = p["Saves"].as_u64().unwrap_or(0) as u32;

            let is_local = is_local_flag 
                || (local_name.to_lowercase() == name.to_lowercase() && !local_name.is_empty());

            new_players.insert(
                name.clone(),
                PlayerInfo {
                    name,
                    platform,
                    team,
                    is_bot,
                    is_local,
                    boost,
                    score,
                    goals,
                    saves,
                },
            );
        }
    }

    if !new_players.is_empty() {
        // println!("State Updated: {} players in lobby", new_players.len());
    }
    state.players.store(Arc::new(new_players));
}

fn parse_platform(id: &str) -> (String, bool) {
    if id.is_empty() {
        return ("Unknown".to_string(), false);
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
        assert_eq!(parse_platform("Bot|0|0"), ("BOT".to_string(), true));
        assert_eq!(
            parse_platform("Unknown|999|0"),
            ("Unknown".to_string(), false)
        );
        assert_eq!(parse_platform(""), ("Unknown".to_string(), false));
    }
}
