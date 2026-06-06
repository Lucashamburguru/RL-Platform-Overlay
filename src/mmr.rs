use crate::state::{AppState, PlayerInfo};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

const MMR_TRACKER_WARMUP_HOST: &str = "https://rocketleague.tracker.network";
const MMR_TRACKER_API_HOST: &str = "https://api.tracker.gg";
const MMR_RANKED_PLAYLIST_IDS: [i32; 10] = [10, 11, 12, 13, 27, 28, 29, 30, 34, 63];
const MMR_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone)]
struct MmrCacheEntry {
    snapshot: TrackerSnapshot,
    fetched_at: Instant,
}

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
        "ps4" | "ps5" | "psn" | "playstation" => ("psn", false),
        "xbox" | "xbl" | "xboxone" | "xboxseries" => ("xbl", false),
        "switch" | "nintendo" => ("switch", false),
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
        "ps4" | "ps5" | "psn" | "playstation" => ("psn", false),
        "xbox" | "xbl" | "xboxone" | "xboxseries" => ("xbl", false),
        "switch" | "nintendo" => ("switch", false),
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
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36")
        .header("Accept-Language", "en-US,en;q=0.9")
        .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8")
        .header("Referer", "https://rocketleague.tracker.network/")
        .send()
        .await;

    let api_url = tracker_api_url(player);
    let response = client.get(&api_url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36")
        .header("Accept-Language", "en-US,en;q=0.9")
        .header("Accept", "application/json, text/plain, */*")
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
        let mut fetching_players = std::collections::HashSet::new();
        let mut mmr_cache: HashMap<String, MmrCacheEntry> = HashMap::new();
        let mut cooldown_until = None;

        loop {
            // Sleep 5 seconds between fetches to be gentler on tracker.gg rate limits
            sleep(Duration::from_secs(5)).await;

            if let Some(until) = cooldown_until {
                if Instant::now() < until {
                    continue;
                }
                cooldown_until = None;
            }

            // Find a player that needs their MMR fetched
            let mut target_player = None;
            let mut cached_player = None;
            {
                let players = state.players.load();
                for (name, info) in players.iter() {
                    // Only fetch for supported platforms and non-bots
                    if info.is_local || info.is_bot {
                        continue;
                    }

                    let Some((cache_key, tracker_player)) = tracker_player_from_info(name, info)
                    else {
                        continue;
                    };

                    if let Some(cache_entry) = mmr_cache.get(&cache_key) {
                        if cache_entry.fetched_at.elapsed() < MMR_CACHE_TTL {
                            cached_player = Some((name.clone(), cache_entry.snapshot.clone()));
                            break;
                        }
                        mmr_cache.remove(&cache_key);
                        fetching_players.remove(&cache_key);
                    } else if info.mmr.is_some() {
                        continue;
                    }

                    if fetching_players.contains(&cache_key) {
                        continue;
                    }

                    target_player = Some((name.clone(), cache_key, tracker_player));
                    break;
                }
            }

            if let Some((name, snapshot)) = cached_player {
                let mut players_map = (**state.players.load()).clone();
                if let Some(player_info) = players_map.get_mut(&name)
                    && !player_info.is_local
                    && player_info.mmr.is_none()
                {
                    player_info.mmr = Some(snapshot);
                    state.players.store(Arc::new(players_map));
                }
                continue;
            }

            if let Some((name, cache_key, tracker_player)) = target_player {
                fetching_players.insert(cache_key.clone());
                append_tracker_log(
                    &state,
                    format!("Fetching MMR for {} ({})", name, tracker_player.platform),
                );
                match fetch_tracker_snapshot(&state.mmr_client, &tracker_player).await {
                    Ok(snapshot) => {
                        append_tracker_log(
                            &state,
                            format!(
                                "Success: Fetched {} ({}). Playlists: {}",
                                name,
                                tracker_player.platform,
                                snapshot.playlists.len()
                            ),
                        );
                        mmr_cache.insert(
                            cache_key.clone(),
                            MmrCacheEntry {
                                snapshot: snapshot.clone(),
                                fetched_at: Instant::now(),
                            },
                        );
                        let mut players_map = (**state.players.load()).clone();
                        if let Some(player_info) = players_map.get_mut(&name) {
                            player_info.mmr = Some(snapshot);
                            state.players.store(Arc::new(players_map));
                        }
                        fetching_players.remove(&cache_key);
                    }
                    Err(e) => {
                        if e.contains("404") {
                            // Profile not found or private on tracker.gg.
                            // Cache an empty MMR snapshot so we don't query this player again.
                            append_tracker_log(
                                &state,
                                format!(
                                    "Cached empty MMR (404/not found) for {} ({})",
                                    name, tracker_player.platform
                                ),
                            );
                            let snapshot = TrackerSnapshot::default();
                            mmr_cache.insert(
                                cache_key.clone(),
                                MmrCacheEntry {
                                    snapshot: snapshot.clone(),
                                    fetched_at: Instant::now(),
                                },
                            );
                            let mut players_map = (**state.players.load()).clone();
                            if let Some(player_info) = players_map.get_mut(&name) {
                                player_info.mmr = Some(snapshot);
                                state.players.store(Arc::new(players_map));
                            }
                            fetching_players.remove(&cache_key);
                        } else {
                            // Temporary error (rate limit, timeout, 403, etc.).
                            append_tracker_log(
                                &state,
                                format!(
                                    "Error fetching {} ({}): {}",
                                    name, tracker_player.platform, e
                                ),
                            );
                            if e.contains("429") || e.contains("403") {
                                eprintln!(
                                    "MMR fetching rate limited or blocked: {e}. Cooling down for 60 seconds."
                                );
                                cooldown_until = Some(Instant::now() + Duration::from_secs(60));
                            } else {
                                // For other errors (like timeouts), back off for 15 seconds.
                                cooldown_until = Some(Instant::now() + Duration::from_secs(15));
                            }
                            fetching_players.remove(&cache_key);
                        }
                    }
                }
            }
        }
    });
}

fn tracker_player_from_info(name: &str, info: &PlayerInfo) -> Option<(String, TrackerPlayer)> {
    let platform_lower = info.platform.to_lowercase();
    if info.is_local || info.is_bot || platform_lower == "bot" || platform_lower == "unknown" {
        return None;
    }

    // Extract the actual ID from PrimaryId, e.g. "Steam|76561197981997358|0".
    let id_parts: Vec<&str> = info.primary_id.split('|').collect();
    let actual_id = id_parts
        .get(1)
        .filter(|id| !id.trim().is_empty())
        .map(|id| (*id).to_string())
        .unwrap_or_else(|| name.to_string());
    let cache_key = if info.primary_id.trim().is_empty() {
        format!("{}|{}", platform_lower, name.to_lowercase())
    } else {
        info.primary_id.to_lowercase()
    };

    Some((
        cache_key,
        TrackerPlayer {
            platform: info.platform.clone(),
            player_name: name.to_string(),
            player_id: actual_id,
        },
    ))
}

pub fn start_local_mmr_refresh(state: Arc<AppState>) {
    let current_state = state.local_mmr.load();
    if current_state.fetching {
        return;
    }

    let identity = (*state.local_player_identity.load()).clone();
    if !identity.is_known() {
        let mut local_mmr = (**current_state).clone();
        local_mmr.fetching = false;
        local_mmr.error = "Waiting for local player identity from Stats API.".to_string();
        state.local_mmr.store(Arc::new(local_mmr));
        return;
    }

    let Some((_, tracker_player)) = tracker_player_from_identity(&identity) else {
        let mut local_mmr = (**current_state).clone();
        local_mmr.fetching = false;
        local_mmr.error = "Local player platform is not supported by Tracker.".to_string();
        state.local_mmr.store(Arc::new(local_mmr));
        return;
    };

    let runtime = match tokio::runtime::Handle::try_current() {
        Ok(runtime) => runtime,
        Err(error) => {
            let mut local_mmr = (**current_state).clone();
            local_mmr.fetching = false;
            local_mmr.error = format!("Could not start local MMR refresh: {error}");
            state.local_mmr.store(Arc::new(local_mmr));
            return;
        }
    };

    let mut local_mmr = (**current_state).clone();
    local_mmr.fetching = true;
    local_mmr.error.clear();
    state.local_mmr.store(Arc::new(local_mmr));

    runtime.spawn(async move {
        append_tracker_log(
            &state,
            format!(
                "Fetching local player MMR for {} ({})",
                identity.name, identity.platform
            ),
        );
        let result = fetch_tracker_snapshot(&state.mmr_client, &tracker_player).await;

        let mut local_mmr = (**state.local_mmr.load()).clone();
        local_mmr.fetching = false;
        match result {
            Ok(snapshot) => {
                append_tracker_log(
                    &state,
                    format!(
                        "Success: Fetched local player {} ({}). Playlists: {}",
                        identity.name,
                        identity.platform,
                        snapshot.playlists.len()
                    ),
                );
                local_mmr.previous = local_mmr.current.take();
                local_mmr.current = Some(snapshot);
                local_mmr.last_updated_unix_ms = crate::stats_api::now_ms();
                local_mmr.error.clear();
            }
            Err(error) => {
                append_tracker_log(
                    &state,
                    format!(
                        "Error fetching local player {} ({}): {}",
                        identity.name, identity.platform, error
                    ),
                );
                local_mmr.error = error;
            }
        }
        state.local_mmr.store(Arc::new(local_mmr));
    });
}

fn tracker_player_from_identity(
    identity: &crate::state::LocalPlayerIdentity,
) -> Option<(String, TrackerPlayer)> {
    let info = PlayerInfo {
        name: identity.name.clone(),
        primary_id: identity.primary_id.clone(),
        platform: identity.platform.clone(),
        ..Default::default()
    };
    tracker_player_from_info(&identity.name, &info)
}

fn append_tracker_log(state: &AppState, message: String) {
    if let Ok(mut logs) = state.debug_tracker_logs.lock() {
        let time_str =
            if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                let secs = now.as_secs();
                let h = (secs / 3600) % 24;
                let m = (secs / 60) % 60;
                let s = secs % 60;
                format!("{:02}:{:02}:{:02}", h, m, s)
            } else {
                "00:00:00".to_string()
            };
        logs.push(format!("[{time_str}] {message}"));
        if logs.len() > 100 {
            logs.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{LocalPlayerIdentity, PlayerInfo};

    #[tokio::test]
    #[ignore = "hits live tracker.gg endpoints"]
    async fn test_pengiwin_steam() {
        let player = TrackerPlayer {
            platform: "Steam".to_string(),
            player_name: "PengiWin".to_string(),
            player_id: "PengiWin".to_string(),
        };
        println!("Fetching MMR for Steam/PengiWin...");
        let client = wreq::Client::builder()
            .emulation(wreq_util::Emulation::Chrome128)
            .build()
            .unwrap();
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
    #[ignore = "hits live tracker.gg endpoints"]
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
        let client = wreq::Client::builder()
            .emulation(wreq_util::Emulation::Chrome128)
            .build()
            .unwrap();
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

    #[test]
    fn local_mmr_refresh_without_runtime_reports_error() {
        let state = AppState::new();
        state
            .local_player_identity
            .store(Arc::new(LocalPlayerIdentity {
                name: "Me".to_string(),
                primary_id: "Steam|76561198000000000|0".to_string(),
                platform: "Steam".to_string(),
            }));

        start_local_mmr_refresh(state.clone());

        let local_mmr = state.local_mmr.load();
        assert!(!local_mmr.fetching);
        assert!(
            local_mmr
                .error
                .starts_with("Could not start local MMR refresh:")
        );
    }

    #[test]
    fn tracker_player_from_info_skips_local_player() {
        let info = PlayerInfo {
            name: "Me".to_string(),
            primary_id: "Steam|1|0".to_string(),
            platform: "Steam".to_string(),
            is_local: true,
            ..Default::default()
        };

        assert!(tracker_player_from_info("Me", &info).is_none());
    }

    #[test]
    fn tracker_player_from_info_uses_stable_primary_id_cache_key() {
        let info = PlayerInfo {
            name: "Opponent".to_string(),
            primary_id: "Steam|76561198000000000|0".to_string(),
            platform: "Steam".to_string(),
            ..Default::default()
        };

        let (cache_key, player) = tracker_player_from_info("Opponent", &info).unwrap();

        assert_eq!(cache_key, "steam|76561198000000000|0");
        assert_eq!(player.player_id, "76561198000000000");
        assert_eq!(player.player_name, "Opponent");
    }
}
