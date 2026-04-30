use crate::state::{AppState, PlayerInfo};
use std::collections::HashMap;
use std::sync::Arc;
use tokio_tungstenite::connect_async;
use futures_util::StreamExt;
use serde_json::Value;

pub async fn start_network_task(state: Arc<AppState>) {
    let url = "ws://127.0.0.1:49123";
    loop {
        if let Ok((mut ws_stream, _)) = connect_async(url).await {
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
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

fn handle_update_state(state: &Arc<AppState>, data: &Value) {
    let mut new_players = HashMap::new();
    if let Some(players) = data["Players"].as_array() {
        for p in players {
            let name = p["Name"].as_str().unwrap_or("Unknown").to_string();
            let primary_id = p["PrimaryId"].as_str().unwrap_or("");
            let (platform, is_bot) = parse_platform(primary_id);
            let team = p["TeamNum"].as_u64().unwrap_or(0) as u8;
            
            new_players.insert(name.clone(), PlayerInfo {
                name,
                platform,
                team,
                is_bot,
            });
        }
    }
    state.players.store(Arc::new(new_players));
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
