use crate::state::{AppState, PlayerInfo};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

const MMR_TRACKER_API_HOST: &str = "https://api.tracker.gg";
const MMR_TRACKED_PLAYLIST_IDS: [i32; 11] = [0, 10, 11, 12, 13, 27, 28, 29, 30, 34, 63];
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

fn is_tracked_playlist(playlist_id: i32) -> bool {
    MMR_TRACKED_PLAYLIST_IDS.contains(&playlist_id)
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

#[derive(thiserror::Error, Debug)]
pub enum MmrError {
    #[error("XUID resolution request failed: {0}")]
    XuidRequest(String),
    #[error("XUID resolution returned non-200 status: {0}")]
    XuidStatus(u16),
    #[error("XUID resolution decode error: {0}")]
    XuidDecode(String),
    #[error("XUID resolution json error: {0}")]
    XuidJson(String),
    #[error("API request error: {0}")]
    ApiRequest(#[from] wreq::Error),
    #[error("HTTP status error: {0}")]
    HttpStatus(u16),
    #[error("Decode error: {0}")]
    Decode(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Failed to extract stats")]
    ExtractStats,
}

pub async fn resolve_xuid_to_gamertag(
    client: &wreq::Client,
    cache: Option<&std::sync::Mutex<HashMap<String, String>>>,
    xuid: &str,
) -> Result<String, MmrError> {
    if let Some(gamertag) = cache
        .and_then(|c| c.lock().ok())
        .and_then(|g| g.get(xuid).cloned())
    {
        return Ok(gamertag);
    }

    let url = format!("https://api.geysermc.org/v2/xbox/gamertag/{}", xuid);
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| MmrError::XuidRequest(e.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        return Err(MmrError::XuidStatus(status.as_u16()));
    }

    #[derive(Deserialize)]
    struct GeyserResponse {
        gamertag: String,
    }

    let text = response
        .text()
        .await
        .map_err(|e| MmrError::XuidDecode(e.to_string()))?;

    let res: GeyserResponse =
        serde_json::from_str(&text).map_err(|e| MmrError::XuidJson(e.to_string()))?;

    if let Some(mut guard) = cache.and_then(|c| c.lock().ok()) {
        const MAX_XUID_CACHE_SIZE: usize = 2000;
        if guard.len() >= MAX_XUID_CACHE_SIZE {
            guard.clear();
        }
        guard.insert(xuid.to_string(), res.gamertag.clone());
    }

    Ok(res.gamertag)
}

async fn send_tracker_request(
    client: &wreq::Client,
    url: &str,
) -> Result<wreq::Response, wreq::Error> {
    client.get(url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36")
        .header("Accept-Language", "en-US,en;q=0.9")
        .header("Accept", "application/json, text/plain, */*")
        .header("Referer", "https://rocketleague.tracker.network/")
        .send()
        .await
}

pub async fn fetch_tracker_snapshot(
    client: &wreq::Client,
    cache: Option<&std::sync::Mutex<HashMap<String, String>>>,
    player: &TrackerPlayer,
) -> Result<TrackerSnapshot, MmrError> {
    let mut resolved_player = player.clone();
    let platform_lower = player.platform.to_lowercase();
    if (platform_lower == "xbox"
        || platform_lower == "xbl"
        || platform_lower == "xboxone"
        || platform_lower == "xboxseries")
        && player.player_id.parse::<u64>().is_ok()
    {
        match resolve_xuid_to_gamertag(client, cache, &player.player_id).await {
            Ok(gamertag) => {
                resolved_player.player_name = gamertag;
            }
            Err(err) => {
                log::error!(
                    "Failed to resolve Xbox XUID {} to gamertag: {}. Falling back to display name: {}",
                    player.player_id,
                    err,
                    player.player_name
                );
            }
        }
    }

    let api_url = tracker_api_url(&resolved_player);
    let mut response = send_tracker_request(client, &api_url).await?;

    let mut status = response.status();
    if status.as_u16() == 404
        && platform_lower == "steam"
        && resolved_player.player_id != resolved_player.player_name
    {
        let mut fallback_player = resolved_player.clone();
        fallback_player.player_id = fallback_player.player_name.clone();
        let fallback_url = tracker_api_url(&fallback_player);
        if let Ok(resp) = send_tracker_request(client, &fallback_url).await {
            let fallback_status = resp.status();
            if fallback_status.is_success() {
                response = resp;
                status = fallback_status;
            }
        }
    }

    if !status.is_success() {
        return Err(MmrError::HttpStatus(status.as_u16()));
    }

    let text = response
        .text()
        .await
        .map_err(|e| MmrError::Decode(e.to_string()))?;
    let payload: Value = serde_json::from_str(&text)?;

    extract_tracker_stats(&payload).ok_or(MmrError::ExtractStats)
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
        if !is_tracked_playlist(playlist_id) {
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

type CachedTrackerPlayer = Option<(String, TrackerSnapshot)>;
type PendingTrackerPlayer = Option<(String, String, TrackerPlayer)>;

fn select_next_player(
    players: &HashMap<String, PlayerInfo>,
    mmr_cache: &mut HashMap<String, MmrCacheEntry>,
    fetching_players: &mut std::collections::HashSet<String>,
    failed_fetches: &HashMap<String, Instant>,
) -> (CachedTrackerPlayer, PendingTrackerPlayer) {
    if players.len() <= 1 {
        return (None, None);
    }
    let mut target_player = None;
    let mut cached_player = None;
    for (name, info) in players.iter() {
        // Only fetch for supported platforms and non-bots
        if info.is_local || info.is_bot {
            continue;
        }

        let Some((cache_key, tracker_player)) = tracker_player_from_info(name, info) else {
            continue;
        };

        if let Some(cache_entry) = mmr_cache.get(&cache_key) {
            if cache_entry.fetched_at.elapsed() < MMR_CACHE_TTL {
                if info.mmr.is_none() {
                    cached_player = Some((name.clone(), cache_entry.snapshot.clone()));
                    break;
                } else {
                    continue;
                }
            }
            mmr_cache.remove(&cache_key);
            fetching_players.remove(&cache_key);
        } else if info.mmr.is_some() {
            continue;
        }

        if fetching_players.contains(&cache_key) {
            continue;
        }

        if failed_fetches.contains_key(&cache_key) {
            continue;
        }

        target_player = Some((name.clone(), cache_key, tracker_player));
        break;
    }
    (cached_player, target_player)
}

fn apply_player_mmr(
    state: &AppState,
    name: &str,
    snapshot: TrackerSnapshot,
    only_if_missing: bool,
) {
    state.game.players.rcu(|players| {
        let mut players_map = (**players).clone();
        if let Some(player_info) = players_map
            .get_mut(name)
            .filter(|p| !p.is_local && (!only_if_missing || p.mmr.is_none()))
        {
            player_info.mmr = Some(snapshot.clone());
        }
        Arc::new(players_map)
    });
}

pub fn start_mmr_fetch_task(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut fetching_players = std::collections::HashSet::new();
        let mut mmr_cache: HashMap<String, MmrCacheEntry> = HashMap::new();
        let mut failed_fetches: HashMap<String, Instant> = HashMap::new();
        let mut cooldown_until = None;

        loop {
            // Sleep 2 seconds between fetches to be gentler on tracker.gg rate limits
            sleep(Duration::from_secs(2)).await;

            // Clean up expired entries in failed_fetches
            failed_fetches.retain(|_, &mut instant| Instant::now() < instant);

            // Clean up expired entries in mmr_cache
            mmr_cache.retain(|_, entry| entry.fetched_at.elapsed() < MMR_CACHE_TTL);

            // Bound MMR cache capacity
            const MAX_MMR_CACHE_SIZE: usize = 2000;
            if mmr_cache.len() > MAX_MMR_CACHE_SIZE {
                let mut entries: Vec<(String, Instant)> = mmr_cache
                    .iter()
                    .map(|(k, e)| (k.clone(), e.fetched_at))
                    .collect();
                entries.sort_by_key(|e| e.1);
                for (key, _) in entries.iter().take(500) {
                    mmr_cache.remove(key);
                    fetching_players.remove(key);
                }
            }

            if let Some(until) = cooldown_until {
                if Instant::now() < until {
                    continue;
                }
                cooldown_until = None;
            }

            if !state.system.config.load().show_lobby_ranks {
                continue;
            }

            // Find a player that needs their MMR fetched
            let (cached_player, target_player) = {
                let players = state.game.players.load();
                select_next_player(
                    &players,
                    &mut mmr_cache,
                    &mut fetching_players,
                    &failed_fetches,
                )
            };

            if let Some((name, snapshot)) = cached_player {
                apply_player_mmr(&state, &name, snapshot, true);
                continue;
            }

            if let Some((name, cache_key, tracker_player)) = target_player {
                fetching_players.insert(cache_key.clone());
                append_tracker_log(
                    &state,
                    format!("Fetching MMR for {} ({})", name, tracker_player.platform),
                );
                match fetch_tracker_snapshot(
                    &state.system.http_client,
                    Some(&state.mmr.xuid_gamertag_cache),
                    &tracker_player,
                )
                .await
                {
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
                        apply_player_mmr(&state, &name, snapshot, false);
                        fetching_players.remove(&cache_key);
                    }
                    Err(e) => {
                        match e {
                            MmrError::HttpStatus(404) => {
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
                                apply_player_mmr(&state, &name, snapshot, false);
                                fetching_players.remove(&cache_key);
                            }
                            MmrError::HttpStatus(429) => {
                                append_tracker_log(
                                    &state,
                                    format!(
                                        "Error fetching {} ({}): Rate limited (429)",
                                        name, tracker_player.platform
                                    ),
                                );
                                log::warn!(
                                    "MMR fetching rate limited: 429. Cooling down for 60 seconds."
                                );
                                cooldown_until = Some(Instant::now() + Duration::from_secs(60));
                                failed_fetches.insert(
                                    cache_key.clone(),
                                    Instant::now() + Duration::from_secs(300),
                                );
                                fetching_players.remove(&cache_key);
                            }
                            MmrError::HttpStatus(403) => {
                                append_tracker_log(
                                    &state,
                                    format!(
                                        "Error fetching {} ({}): Blocked (403)",
                                        name, tracker_player.platform
                                    ),
                                );
                                log::warn!(
                                    "MMR fetching blocked: 403. Cooling down for 60 seconds."
                                );
                                cooldown_until = Some(Instant::now() + Duration::from_secs(60));
                                failed_fetches.insert(
                                    cache_key.clone(),
                                    Instant::now() + Duration::from_secs(300),
                                );
                                fetching_players.remove(&cache_key);
                            }
                            other => {
                                let err_str = other.to_string();
                                append_tracker_log(
                                    &state,
                                    format!(
                                        "Error fetching {} ({}): {}",
                                        name, tracker_player.platform, err_str
                                    ),
                                );
                                // For other errors (like timeouts), back off for 5 seconds globally.
                                cooldown_until = Some(Instant::now() + Duration::from_secs(5));
                                failed_fetches.insert(
                                    cache_key.clone(),
                                    Instant::now() + Duration::from_secs(120),
                                );
                                fetching_players.remove(&cache_key);
                            }
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
    // Bolt: Avoid collecting string splits into a Vec
    let actual_id = info.primary_id.split('|')
        .nth(1)
        .filter(|id| !id.trim().is_empty())
        .map(|id| id.to_string())
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
    if !state.system.config.load().show_lobby_ranks {
        return;
    }
    let current_state = state.mmr.local_mmr.load();
    if current_state.fetching {
        return;
    }

    let now = crate::stats_api::now_ms();
    let cooldown_ms = 15_000;
    if now.saturating_sub(current_state.last_updated_unix_ms) < cooldown_ms {
        let remaining_secs =
            (cooldown_ms - now.saturating_sub(current_state.last_updated_unix_ms)) / 1000;
        let mut local_mmr = (**current_state).clone();
        local_mmr.error = format!("Refresh on cooldown. Please wait {remaining_secs}s.");
        state.mmr.local_mmr.store(Arc::new(local_mmr));
        return;
    }

    let identity = (*state.game.local_player_identity.load()).clone();
    if !identity.is_known() {
        let mut local_mmr = (**current_state).clone();
        local_mmr.fetching = false;
        local_mmr.error = "Waiting for local player identity from Stats API.".to_string();
        state.mmr.local_mmr.store(Arc::new(local_mmr));
        return;
    }

    let Some((_, tracker_player)) = tracker_player_from_identity(&identity) else {
        let mut local_mmr = (**current_state).clone();
        local_mmr.fetching = false;
        local_mmr.error = "Local player platform is not supported by Tracker.".to_string();
        state.mmr.local_mmr.store(Arc::new(local_mmr));
        return;
    };

    let runtime = match tokio::runtime::Handle::try_current() {
        Ok(runtime) => runtime,
        Err(error) => {
            let mut local_mmr = (**current_state).clone();
            local_mmr.fetching = false;
            local_mmr.error = format!("Could not start local MMR refresh: {error}");
            state.mmr.local_mmr.store(Arc::new(local_mmr));
            return;
        }
    };

    let mut local_mmr = (**current_state).clone();
    local_mmr.fetching = true;
    local_mmr.error.clear();
    state.mmr.local_mmr.store(Arc::new(local_mmr));

    runtime.spawn(async move {
        append_tracker_log(
            &state,
            format!(
                "Fetching local player MMR for {} ({})",
                identity.name, identity.platform
            ),
        );
        let result = fetch_tracker_snapshot(
            &state.system.http_client,
            Some(&state.mmr.xuid_gamertag_cache),
            &tracker_player,
        )
        .await;

        let mut local_mmr = (**state.mmr.local_mmr.load()).clone();
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
                let err_str = error.to_string();
                append_tracker_log(
                    &state,
                    format!(
                        "Error fetching local player {} ({}): {}",
                        identity.name, identity.platform, err_str
                    ),
                );
                local_mmr.error = err_str;
            }
        }
        state.mmr.local_mmr.store(Arc::new(local_mmr));
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
    if let Ok(mut logs) = state.mmr.debug_tracker_logs.lock() {
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
        logs.push_back(format!("[{time_str}] {message}"));
        if logs.len() > 100 {
            logs.pop_front();
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
        match fetch_tracker_snapshot(&client, None, &player).await {
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
            match fetch_tracker_snapshot(&client, None, &player).await {
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
            .game
            .local_player_identity
            .store(Arc::new(LocalPlayerIdentity {
                name: "Me".to_string(),
                primary_id: "Steam|76561198000000000|0".to_string(),
                platform: "Steam".to_string(),
            }));

        start_local_mmr_refresh(state.clone());

        let local_mmr = state.mmr.local_mmr.load();
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

    #[test]
    fn test_select_next_player_cooldown() {
        let mut players = HashMap::new();
        players.insert(
            "Player1".to_string(),
            PlayerInfo {
                name: "Player1".to_string(),
                primary_id: "Epic|player1|0".to_string(),
                platform: "Epic".to_string(),
                ..Default::default()
            },
        );
        players.insert(
            "Player2".to_string(),
            PlayerInfo {
                name: "Player2".to_string(),
                primary_id: "Epic|player2|0".to_string(),
                platform: "Epic".to_string(),
                ..Default::default()
            },
        );

        let mut mmr_cache = HashMap::new();
        let mut fetching_players = std::collections::HashSet::new();
        let mut failed_fetches = HashMap::new();

        // 1. Select when no cooldowns are active.
        let (_, target) = select_next_player(
            &players,
            &mut mmr_cache,
            &mut fetching_players,
            &failed_fetches,
        );
        let selected_name = target.unwrap().0;
        assert!(selected_name == "Player1" || selected_name == "Player2");

        // 2. Put the selected player on cooldown in failed_fetches.
        let cache_key = if selected_name == "Player1" {
            "epic|player1|0".to_string()
        } else {
            "epic|player2|0".to_string()
        };
        failed_fetches.insert(cache_key, Instant::now() + Duration::from_secs(60));

        // 3. Select again, the other player must be chosen because the first one is on cooldown.
        let (_, target2) = select_next_player(
            &players,
            &mut mmr_cache,
            &mut fetching_players,
            &failed_fetches,
        );
        let second_selected_name = target2.unwrap().0;
        assert_ne!(selected_name, second_selected_name);
        assert!(second_selected_name == "Player1" || second_selected_name == "Player2");
    }

    #[test]
    fn local_mmr_refresh_skips_when_show_lobby_ranks_is_false() {
        let state = AppState::new();
        let mut config = (**state.system.config.load()).clone();
        config.show_lobby_ranks = false;
        state.system.config.store(Arc::new(config));

        state
            .game
            .local_player_identity
            .store(Arc::new(LocalPlayerIdentity {
                name: "Me".to_string(),
                primary_id: "Steam|76561198000000000|0".to_string(),
                platform: "Steam".to_string(),
            }));

        start_local_mmr_refresh(state.clone());

        let local_mmr = state.mmr.local_mmr.load();
        assert!(!local_mmr.fetching);
        assert!(local_mmr.error.is_empty());
    }

    #[tokio::test]
    #[ignore = "hits live geysermc endpoint"]
    async fn test_resolve_xuid_to_gamertag() {
        let client = wreq::Client::builder()
            .emulation(wreq_util::Emulation::Chrome128)
            .build()
            .unwrap();
        let res = resolve_xuid_to_gamertag(&client, None, "2535432196048835").await;
        assert_eq!(res.unwrap(), "Tim203");
    }
}
