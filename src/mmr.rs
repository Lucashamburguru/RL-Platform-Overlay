use crate::state::AppState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

const MMR_TRACKER_WARMUP_HOST: &str = "https://rocketleague.tracker.network";
const MMR_TRACKER_API_HOST: &str = "https://api.tracker.gg";
const MMR_RANKED_PLAYLIST_IDS: [i32; 10] = [10, 11, 12, 13, 27, 28, 29, 30, 34, 63];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackerPlayer {
    pub platform: String,
    pub player_name: String,
    pub player_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerPlaylistSnapshot {
    pub name: String,
    pub rating: i32,
    pub matches: i32,
    pub tier_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrackerSnapshot {
    pub playlists: HashMap<i32, TrackerPlaylistSnapshot>,
    pub last_updated: Option<String>,
    pub current_season: Option<i32>,
}

fn is_ranked_playlist(playlist_id: i32) -> bool {
    MMR_RANKED_PLAYLIST_IDS.contains(&playlist_id)
}

fn tracker_api_url(player: &TrackerPlayer) -> String {
    let (tracker_platform, use_id) = match player.platform.to_lowercase().as_str() {
        "steam" => ("steam", true),
        "ps4" | "ps5" | "psn" => ("psn", false),
        "xbox" | "xbl" => ("xbl", false),
        "switch" => ("switch", false),
        _ => ("epic", false),
    };

    let search_term = if use_id {
        &player.player_id
    } else {
        &player.player_name
    };
    let encoded = urlencoding::encode(search_term);
    format!(
        "{MMR_TRACKER_API_HOST}/api/v2/rocket-league/standard/profile/{tracker_platform}/{encoded}"
    )
}

fn tracker_warmup_url(player: &TrackerPlayer) -> String {
    let (tracker_platform, use_id) = match player.platform.to_lowercase().as_str() {
        "steam" => ("steam", true),
        "ps4" | "ps5" | "psn" => ("psn", false),
        "xbox" | "xbl" => ("xbl", false),
        "switch" => ("switch", false),
        _ => ("epic", false),
    };

    let search_term = if use_id {
        &player.player_id
    } else {
        &player.player_name
    };
    let encoded = urlencoding::encode(search_term);
    format!("{MMR_TRACKER_WARMUP_HOST}/rocket-league/profile/{tracker_platform}/{encoded}/overview")
}

pub async fn fetch_tracker_snapshot(
    client: &wreq::Client,
    player: &TrackerPlayer,
) -> Result<TrackerSnapshot, String> {
    let warmup_url = tracker_warmup_url(player);
    // Warmup request to bypass some basic checks or establish cookies
    let _ = client.get(&warmup_url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .header("Accept-Language", "fr-FR,fr;q=0.9,en-US;q=0.8,en;q=0.7")
        .header("Referer", "https://rocketleague.tracker.network/")
        .send()
        .await;

    let api_url = tracker_api_url(player);
    let response = client.get(&api_url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .header("Accept-Language", "fr-FR,fr;q=0.9,en-US;q=0.8,en;q=0.7")
        .header("Referer", &warmup_url)
        .send()
        .await
        .map_err(|e| format!("api request error: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("non-200 status: {}", status));
    }

    let text = response
        .text()
        .await
        .map_err(|e| format!("decode error: {}", e))?;
    let payload: Value = serde_json::from_str(&text).map_err(|e| format!("json error: {}", e))?;

    extract_tracker_stats(&payload).ok_or_else(|| "Failed to extract stats".to_string())
}

fn extract_tracker_stats(payload: &Value) -> Option<TrackerSnapshot> {
    let data = payload.get("data")?;
    let metadata = data.get("metadata");
    let segments = data.get("segments")?.as_array()?;

    let mut snapshot = TrackerSnapshot {
        last_updated: metadata
            .and_then(|v| v.get("lastUpdated"))
            .and_then(|v| v.get("value"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        current_season: metadata
            .and_then(|v| v.get("currentSeason"))
            .and_then(Value::as_i64)
            .and_then(|v| i32::try_from(v).ok()),
        ..Default::default()
    };

    for segment in segments {
        if segment.get("type").and_then(Value::as_str) != Some("playlist") {
            continue;
        }

        let playlist_id = segment
            .get("attributes")
            .and_then(|v| v.get("playlistId"))
            .and_then(Value::as_i64)
            .and_then(|v| i32::try_from(v).ok());
        let Some(playlist_id) = playlist_id else {
            continue;
        };
        if !is_ranked_playlist(playlist_id) {
            continue;
        }

        let stats = segment.get("stats");
        let rating = stats
            .and_then(|v| v.get("rating"))
            .and_then(|v| v.get("value"))
            .and_then(Value::as_i64)
            .and_then(|v| i32::try_from(v).ok());
        let Some(rating) = rating else {
            continue;
        };

        let matches = stats
            .and_then(|v| v.get("matchesPlayed"))
            .and_then(|v| v.get("value"))
            .and_then(Value::as_i64)
            .and_then(|v| i32::try_from(v).ok())
            .unwrap_or(0);
        let name = segment
            .get("metadata")
            .and_then(|v| v.get("name"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("Playlist {playlist_id}"));
        let tier_name = stats
            .and_then(|v| v.get("tier"))
            .and_then(|v| v.get("metadata"))
            .and_then(|v| v.get("name"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_default();

        snapshot.playlists.insert(
            playlist_id,
            TrackerPlaylistSnapshot {
                name,
                rating,
                matches,
                tier_name,
            },
        );
    }

    Some(snapshot)
}

pub fn start_mmr_fetch_task(state: Arc<AppState>) {
    tokio::spawn(async move {
        let client = match wreq::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to build wreq client: {}", e);
                return;
            }
        };

        let mut fetching_players = std::collections::HashSet::new();
        loop {
            // Sleep 5 seconds between fetches to be gentler on tracker.gg rate limits
            sleep(Duration::from_secs(5)).await;

            // Find a player that needs their MMR fetched
            let mut target_player = None;
            {
                let players = state.players.load();
                for (name, info) in players.iter() {
                    // Only fetch for supported platforms and non-bots
                    if info.is_bot || info.mmr.is_some() || fetching_players.contains(name) {
                        continue;
                    }

                    let platform_lower = info.platform.to_lowercase();
                    let is_supported = matches!(
                        platform_lower.as_str(),
                        "steam" | "epic" | "ps4" | "ps5" | "psn" | "xbox" | "xbl" | "switch"
                    );

                    if is_supported {
                        // Extract the actual ID from the PrimaryId string (e.g. "Steam|76561197981997358|0")
                        let id_parts: Vec<&str> = info.primary_id.split('|').collect();
                        let actual_id = if id_parts.len() > 1 {
                            id_parts[1].to_string()
                        } else {
                            name.clone()
                        };
                        target_player = Some((name.clone(), info.platform.clone(), actual_id));
                        break;
                    }
                }
            }

            if let Some((name, platform, actual_id)) = target_player {
                fetching_players.insert(name.clone());
                let tracker_player = TrackerPlayer {
                    platform: platform.clone(),
                    player_name: name.clone(),
                    player_id: actual_id,
                };

                match fetch_tracker_snapshot(&client, &tracker_player).await {
                    Ok(snapshot) => {
                        let mut players_map = (**state.players.load()).clone();
                        if let Some(player_info) = players_map.get_mut(&name) {
                            player_info.mmr = Some(snapshot);
                            state.players.store(Arc::new(players_map));
                        }
                    }
                    Err(e) => {
                        if e.contains("404") {
                            // Profile not found or private on tracker.gg.
                            // Cache an empty MMR snapshot so we don't query this player again.
                            let mut players_map = (**state.players.load()).clone();
                            if let Some(player_info) = players_map.get_mut(&name) {
                                player_info.mmr = Some(TrackerSnapshot::default());
                                state.players.store(Arc::new(players_map));
                            }
                        } else {
                            // Temporary error (rate limit, timeout). Remove from fetching set to retry later.
                            fetching_players.remove(&name);
                        }
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pengiwin_steam() {
        let player = TrackerPlayer {
            platform: "Steam".to_string(),
            player_name: "PengiWin".to_string(),
            player_id: "PengiWin".to_string(),
        };
        println!("Fetching MMR for Steam/PengiWin...");
        let client = wreq::Client::builder().build().unwrap();
        match fetch_tracker_snapshot(&client, &player).await {
            Ok(snapshot) => {
                println!("Got snapshot with {} playlists", snapshot.playlists.len());
                for (id, pl) in snapshot.playlists {
                    println!("Playlist {}: {} MMR ({})", id, pl.rating, pl.tier_name);
                }
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_alfa_psn() {
        let players = vec![
            TrackerPlayer {
                platform: "Steam".to_string(),
                player_name: "PengiWin".to_string(),
                player_id: "76561198034789585".to_string(), // Example SteamID64
            },
            TrackerPlayer {
                platform: "Ps4".to_string(),
                player_name: "alfa_699".to_string(),
                player_id: "2695557321719975533".to_string(),
            },
            TrackerPlayer {
                platform: "Ps4".to_string(),
                player_name: "Cleanmolles".to_string(),
                player_id: "8318453829852839315".to_string(),
            },
            TrackerPlayer {
                platform: "Epic".to_string(),
                player_name: "pengiwin".to_string(),
                player_id: "pengiwin".to_string(),
            },
        ];
        let client = wreq::Client::builder().build().unwrap();
        for player in players {
            println!(
                "Fetching MMR for {}/{}...",
                player.platform, player.player_name
            );
            match fetch_tracker_snapshot(&client, &player).await {
                Ok(snapshot) => {
                    println!(
                        "Got snapshot with {} playlists for {}",
                        snapshot.playlists.len(),
                        player.player_name
                    );
                    for (id, pl) in snapshot.playlists {
                        println!("  Playlist {}: {} MMR ({})", id, pl.rating, pl.tier_name);
                    }
                }
                Err(e) => {
                    println!("Error for {}: {}", player.player_name, e);
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }
}
