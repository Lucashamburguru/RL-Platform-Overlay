use super::{mark_replays_uploaded, set_status, usize_to_u32_saturating};
use crate::json_utils::number_field_i32;
use crate::state::AppState;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use url::Url;

const BALLCHASING_API_BASE: &str = "https://ballchasing.com/api/";

pub fn maybe_start_initial_replay_cache_sync(state: &Arc<AppState>) -> bool {
    if state
        .system
        .config
        .load()
        .ballchasing_api_key
        .trim()
        .is_empty()
    {
        return false;
    }

    if state
        .replays
        .initial_cache_sync_started
        .swap(true, Ordering::SeqCst)
    {
        return false;
    }

    start_sync_replays_task(state.clone())
}

pub fn start_sync_replays_task(state: Arc<AppState>) -> bool {
    let Ok(operation_guard) = state.replays.maintenance_gate.clone().try_read_owned() else {
        return false;
    };
    if state.replays.sync_running.swap(true, Ordering::SeqCst) {
        return false;
    }

    tokio::spawn(async move {
        let _operation_guard = operation_guard;
        let state_clone = state.clone();
        if let Err(e) = run_sync_replays(state).await {
            set_status(&state_clone, &format!("Error: Sync failed ({e})"));
            log::error!("Sync replays execution error: {e}");
        }
        state_clone
            .replays
            .sync_running
            .store(false, Ordering::SeqCst);
    });
    true
}

fn parse_cloud_metadata(
    item: &serde_json::Value,
) -> Option<crate::replay_metadata::ReplayMetadataEntry> {
    let id = item["id"].as_str()?;
    let filename = format!("{}.replay", id.to_lowercase());

    let display_name = item["replay_title"]
        .as_str()
        .or_else(|| item["title"].as_str())
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| id.to_string());

    let date = item["date"]
        .as_str()
        .or_else(|| item["match_date"].as_str())
        .or_else(|| item["created"].as_str())
        .unwrap_or("")
        .to_string();

    let map_name = item["map_name"]
        .as_str()
        .or_else(|| item["map_code"].as_str())
        .unwrap_or("")
        .to_string();

    let team0_score = number_field_i32(&item["blue"], &["score"]);
    let team1_score = number_field_i32(&item["orange"], &["score"]);

    let mut player_names = Vec::new();
    if let Some(players) = item["blue"]["players"].as_array() {
        for p in players {
            if let Some(name) = p["name"].as_str() {
                player_names.push(name.to_string());
            }
        }
    }
    if let Some(players) = item["orange"]["players"].as_array() {
        for p in players {
            if let Some(name) = p["name"].as_str() {
                player_names.push(name.to_string());
            }
        }
    }

    let match_type = item["playlist_name"]
        .as_str()
        .or_else(|| item["playlist_id"].as_str())
        .unwrap_or("")
        .to_string();

    let player_name = item["uploader"]["name"].as_str().unwrap_or("").to_string();

    Some(crate::replay_metadata::ReplayMetadataEntry {
        filename,
        display_name,
        date,
        map_name,
        team0_score,
        team1_score,
        player_names,
        players: Vec::new(),
        goals: Vec::new(),
        replay_id: id.to_string(),
        duration_seconds: None,
        frame_count: None,
        file_size: 0, // Mark as cloud entry
        modified_unix_secs: None,
        error: String::new(),
        player_name,
        match_type,
    })
}

fn validated_ballchasing_api_url(raw: &str) -> Result<Url, String> {
    let base = Url::parse(BALLCHASING_API_BASE)
        .map_err(|error| format!("Ballchasing API base URL is invalid: {error}"))?;
    let url = base
        .join(raw)
        .map_err(|error| format!("Invalid Ballchasing pagination URL: {error}"))?;
    let valid_path = url.path() == "/api" || url.path().starts_with("/api/");
    if url.scheme() != "https"
        || url.host_str() != Some("ballchasing.com")
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || !valid_path
    {
        return Err(format!(
            "Rejected untrusted Ballchasing pagination URL: {url}"
        ));
    }
    Ok(url)
}

async fn run_sync_replays(state: Arc<AppState>) -> Result<(), String> {
    let config = state.system.config.load();
    let api_key = config.ballchasing_api_key.trim().to_string();
    if api_key.is_empty() {
        set_status(&state, "Error: API key is empty");
        return Ok(());
    }

    set_status(&state, "Syncing from ballchasing.com...");

    let client = &state.system.ballchasing_client;

    let mut next_url = Some(validated_ballchasing_api_url(
        "replays?uploader=me&count=200",
    )?);
    let mut fetched_ids = Vec::new();
    let mut cloud_entries = Vec::new();
    let mut pages_fetched = 0;

    // Fetch up to 500 replays (capping at 3 pages max to prevent infinite loops)
    while let Some(url) = next_url.take() {
        if pages_fetched >= 3 {
            break;
        }

        let response = client
            .get(url.as_str())
            .header("Authorization", &api_key)
            .send()
            .await
            .map_err(|e| format!("Network request failed: {e}"))?;

        let status = response.status();
        if !status.is_success() {
            set_status(&state, &format!("Error: Sync failed (HTTP {})", status));
            return Ok(());
        }

        let body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read body: {e}"))?;
        let json: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| format!("Failed to parse response JSON: {e}"))?;

        if let Some(list) = json["list"].as_array() {
            for item in list {
                if let Some(id) = item["id"].as_str() {
                    fetched_ids.push(id.to_string());
                }
                if let Some(entry) = parse_cloud_metadata(item) {
                    cloud_entries.push(entry);
                }
            }
        }

        next_url = json["next"]
            .as_str()
            .map(validated_ballchasing_api_url)
            .transpose()?;
        pages_fetched += 1;

        if next_url.is_some() {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    let count = fetched_ids.len();
    state
        .replays
        .ballchasing_cloud_count
        .store(usize_to_u32_saturating(count), Ordering::SeqCst);

    let cloud_entries = cloud_entries
        .into_iter()
        .map(|entry| (entry.filename.clone(), entry))
        .collect();
    state
        .replays
        .cloud_metadata_cache
        .store(Arc::new(cloud_entries));
    crate::replay_metadata::refresh_merged_metadata_cache(&state);

    // Update config cache with these formatted filenames
    let filenames: Vec<String> = fetched_ids
        .into_iter()
        .map(|id| format!("{}.replay", id.to_lowercase()))
        .collect();
    let added = mark_replays_uploaded(&state, &filenames).map_err(|error| {
        format!("Fetched {count} remote replays, but could not save upload membership: {error}")
    })?;

    set_status(
        &state,
        &format!(
            "Success: Synced {} replays (added {} new to local cache)",
            count, added
        ),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cloud_metadata_ignores_scores_that_do_not_fit_i32() {
        let normal = parse_cloud_metadata(&serde_json::json!({
            "id": "ABC123",
            "blue": {
                "score": 3,
                "players": [{"name": "Blue"}]
            },
            "orange": {
                "score": 2,
                "players": [{"name": "Orange"}]
            }
        }))
        .expect("cloud metadata should parse");

        assert_eq!(normal.team0_score, Some(3));
        assert_eq!(normal.team1_score, Some(2));
        assert_eq!(normal.player_names, vec!["Blue", "Orange"]);

        let oversized = parse_cloud_metadata(&serde_json::json!({
            "id": "DEF456",
            "blue": {
                "score": i32::MAX as i64 + 1
            },
            "orange": {
                "score": i32::MIN as i64 - 1
            }
        }))
        .expect("cloud metadata should parse without score fields");

        assert_eq!(oversized.team0_score, None);
        assert_eq!(oversized.team1_score, None);
    }

    #[test]
    fn pagination_url_validation_stays_on_ballchasing_api() {
        assert_eq!(
            validated_ballchasing_api_url("replays?after=cursor")
                .unwrap()
                .as_str(),
            "https://ballchasing.com/api/replays?after=cursor"
        );
        assert!(
            validated_ballchasing_api_url("https://ballchasing.com/api/replays?after=cursor")
                .is_ok()
        );

        for untrusted in [
            "http://ballchasing.com/api/replays",
            "https://evil.example/api/replays",
            "https://api.ballchasing.com/api/replays",
            "https://user:password@ballchasing.com/api/replays",
            "https://ballchasing.com:444/api/replays",
            "//evil.example/api/replays",
            "https://ballchasing.com/upload",
            "https://ballchasing.com/api/replays#fragment",
        ] {
            assert!(
                validated_ballchasing_api_url(untrusted).is_err(),
                "accepted {untrusted}"
            );
        }
    }
}
