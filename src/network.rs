use crate::state::{AppState, PlayerInfo};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio_tungstenite::connect_async;
use futures_util::StreamExt;
use serde_json::Value;

pub async fn start_network_task(state: Arc<AppState>) {
    let url = "ws://127.0.0.1:49123";
    let addr = "127.0.0.1:49123";
    println!("Connecting to {}...", url);
    loop {
        // Try WebSocket first
        match connect_async(url).await {
            Ok((mut ws_stream, _)) => {
                println!("Connected to Rocket League via WebSocket!");
                while let Some(msg) = ws_stream.next().await {
                    if let Ok(msg) = msg {
                        if let Ok(text) = msg.to_text() {
                            if let Ok(json) = serde_json::from_str::<Value>(text) {
                                if json["Event"] == "UpdateState" {
                                    handle_update_state(&state, &json["Data"]);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                if format!("{}", e).contains("invalid HTTP version") {
                    println!("Detected raw TCP traffic. Switching to TCP mode...");
                    if let Ok(mut stream) = TcpStream::connect(addr).await {
                        println!("Connected to Rocket League via TCP!");
                        let mut buffer = [0u8; 16384];
                        let mut leftover = String::new();
                        loop {
                            match stream.read(&mut buffer).await {
                                Ok(0) => break, // EOF
                                Ok(n) => {
                                    let text = format!("{}{}", leftover, String::from_utf8_lossy(&buffer[..n]));
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
                                                if depth == 0 { start = i; }
                                                depth += 1;
                                            }
                                            '}' if !in_string => {
                                                depth -= 1;
                                                if depth == 0 {
                                                    let json_str = &text[start..=i];
                                                    if let Ok(json) = serde_json::from_str::<Value>(json_str) {
                                                    let event = json["Event"].as_str().unwrap_or("Unknown");
                                                    if event == "UpdateState" {
                                                        handle_update_state(&state, &json["Data"]);
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
                    }
                } else {
                    eprintln!("Connection failed: {}. Retrying in 5s...", e);
                }
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

fn handle_update_state(state: &Arc<AppState>, data: &Value) {
    static mut LOGGED: bool = false;
    unsafe {
        if !LOGGED {
            println!("FULL UPDATE STATE DATA: {}", serde_json::to_string_pretty(data).unwrap_or_default());
            LOGGED = true;
        }
    }
    let mut new_players = HashMap::new();
    if let Some(players) = data["Players"].as_array() {
        if players.is_empty() {
            println!("UpdateState: Players array is EMPTY");
        }
        for p in players {
            let name = p["Name"].as_str().unwrap_or("Unknown").to_string();
            let primary_id = p["PrimaryId"].as_str().unwrap_or("");
            let (platform, is_bot) = parse_platform(primary_id);
            let team = p["TeamNum"].as_u64().unwrap_or(0) as u8;
            
            println!("Parsed Player: {} (Team {}, ID: {}) -> Platform: {}", name, team, primary_id, platform);
            
            new_players.insert(name.clone(), PlayerInfo {
                name,
                platform,
                team,
                is_bot,
            });
        }
    } else {
        println!("UpdateState: No 'Players' field found in Data");
    }
    
    let count = new_players.len();
    state.players.store(Arc::new(new_players));
    println!("State Updated: {} players in lobby", count);
}

fn parse_platform(id: &str) -> (String, bool) {
    let parts: Vec<&str> = id.split('|').collect();
    if parts.is_empty() { return ("Unknown".to_string(), false); }
    let platform = parts[0];
    match platform {
        "Steam" => ("Steam".to_string(), false),
        "Epic" => ("Epic".to_string(), false),
        "Ps4" | "Ps5" => ("PlayStation".to_string(), false),
        "Xbox" | "XBoxOne" => ("Xbox".to_string(), false),
        "Switch" => ("Switch".to_string(), false),
        "Bot" => ("BOT".to_string(), true),
        _ => (platform.to_string(), false),
    }
}
