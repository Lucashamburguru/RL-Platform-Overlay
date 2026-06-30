use crate::session::SessionMode;
use crate::state::{
    AppState, DashboardMatchSnapshot, PlayerInfo, TeammateBumpTouch, TouchCounterDebounce,
};
use crate::stats_api::{StatsApiTransport, TcpJsonSplitter};
use crate::stats_api_parser::{
    RosterSignature, StatsApiEvent, StatsApiParseContext, parse_stats_api_event,
};
use futures_util::StreamExt;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio_tungstenite::connect_async;

fn clear_lobby_state(state: &AppState) {
    state.game.players.store(Arc::new(HashMap::new()));
    if let Ok(mut debounce) = state.game.touch_counter_debounce.lock() {
        debounce.clear();
    }
    clear_teammate_bump_estimator(state);
    state.game.match_roster.store(Arc::new(HashMap::new()));
    state.game.match_roster_guid.store(Arc::new(String::new()));
    state.game.local_player_name.store(Arc::new("".to_string()));
}

fn handle_match_reset(state: &Arc<AppState>, early_leave: bool) {
    let is_replay = state.flags.is_watching_replay.swap(false, Ordering::SeqCst);

    let players = state.game.players.load();
    let match_roster = state.game.match_roster.load();
    let is_online =
        players.values().any(is_online_player) || match_roster.values().any(is_online_player);

    let mut session = (**state.game.session.load()).clone();
    session.is_watching_replay = is_replay;
    let matches_before = session.matches_played;
    if early_leave && is_online {
        session.record_early_leave();
    }
    if session.matches_played > matches_before {
        update_result_diagnostics(state, &session, "early_leave");
        crate::history::record_completed_match(state, &session);
    }
    session.handle_reset_event();
    session.is_watching_replay = false;
    state.game.session.store(Arc::new(session));

    clear_lobby_state(state);
    state
        .game
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
                state.flags.is_connected.store(true, Ordering::SeqCst);
                update_transport(&state, StatsApiTransport::WebSocket);
                while let Some(msg_res) = ws_stream.next().await {
                    match msg_res {
                        Ok(msg) => {
                            if let Ok(text) = msg.to_text() {
                                match serde_json::from_str::<Value>(text) {
                                    Ok(json) => handle_event(&state, &json),
                                    Err(error) => update_parse_error(&state, error.to_string()),
                                }
                            }
                        }
                        Err(error) => {
                            log::error!("WebSocket stream error: {}", error);
                            update_parse_error(&state, error.to_string());
                            break;
                        }
                    }
                }
                state.flags.is_connected.store(false, Ordering::SeqCst);
                clear_lobby_state(&state);
            }
            Err(e) => {
                if matches!(
                    e,
                    tokio_tungstenite::tungstenite::Error::Protocol(
                        tokio_tungstenite::tungstenite::error::ProtocolError::HttparseError(_)
                    )
                ) {
                    log::info!("Detected raw TCP traffic. Switching to TCP mode...");
                    if let Ok(mut stream) = TcpStream::connect(addr).await {
                        log::info!("Connected to Rocket League via TCP!");
                        state.flags.is_connected.store(true, Ordering::SeqCst);
                        update_transport(&state, StatsApiTransport::Tcp);
                        let mut buffer = [0u8; 16384];
                        let mut splitter = TcpJsonSplitter::default();
                        loop {
                            match stream.read(&mut buffer).await {
                                Ok(0) => break, // EOF
                                Ok(n) => {
                                    for json_str in splitter.push(&buffer[..n]) {
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
                        state.flags.is_connected.store(false, Ordering::SeqCst);
                        clear_lobby_state(&state);
                    } else {
                        state.flags.is_connected.store(false, Ordering::SeqCst);
                        update_connection_error(&state, "Could not connect via TCP.".to_string());
                    }
                } else {
                    state.flags.is_connected.store(false, Ordering::SeqCst);
                    log::error!("Connection failed: {}. Retrying in 5s...", e);
                    update_connection_error(&state, e.to_string());
                }
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

fn handle_event(state: &Arc<AppState>, json: &Value) {
    let parsed_event = parse_event_for_state(state, json);
    update_last_event(state, &parsed_event.event_name);
    match parsed_event.event_name.as_str() {
        "UpdateState" => handle_update_state(state, &parsed_event),
        "ClockUpdatedSeconds" => {
            state.game.session.rcu(|current_session| {
                let mut session = (**current_session).clone();
                session.handle_clock_update(&parsed_event.data);
                if !session.round_started {
                    session.handle_round_started();
                }
                Arc::new(session)
            });
        }
        "RoundStarted" | "BallHit" | "GoalScored" | "StatfeedEvent" => {
            let current_session = state.game.session.load();
            if !current_session.round_started {
                let mut session = (**current_session).clone();
                session.handle_round_started();
                state.game.session.store(Arc::new(session));
            }
        }
        "ReplayCreated" | "ReplayPlaybackStart" => {
            state.flags.is_watching_replay.store(true, Ordering::SeqCst);
            state.game.session.rcu(|session| {
                let mut s = (**session).clone();
                s.is_watching_replay = true;
                Arc::new(s)
            });
        }
        "ReplayPlaybackEnd" | "ReplayWillEnd" => {
            handle_match_reset(state, false);
        }
        "MatchEnded" => {
            if state.flags.is_watching_replay.load(Ordering::SeqCst) {
                return;
            }
            let local_team = state.game.local_team.load(Ordering::SeqCst);
            let local_team_hint = (local_team != crate::state::NO_TEAM).then_some(local_team);
            let mut session = (**state.game.session.load()).clone();
            let matches_before = session.matches_played;
            session.handle_match_ended(&parsed_event.data, local_team_hint);
            if session.matches_played > matches_before {
                update_result_diagnostics(state, &session, "match_ended");
                crate::history::record_completed_match(state, &session);
            }
            session.handle_reset_event();
            state.game.session.store(Arc::new(session));

            state
                .game
                .local_team
                .store(crate::state::NO_TEAM, Ordering::SeqCst);

            let state_clone = state.clone();
            tokio::spawn(async move {
                if state_clone
                    .system
                    .is_simulating_input
                    .swap(true, Ordering::SeqCst)
                {
                    log::warn!("Key simulation already in progress, ignoring duplicate trigger.");
                    return;
                }

                let config = state_clone.system.config.load();
                let mut system = sysinfo::System::new();

                if config.auto_gg {
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    log::info!("Auto-GG: sending configured key sequence...");
                    crate::automation::simulate_sequence(
                        &config.auto_gg_sequence,
                        "Auto-GG",
                        125,
                        &mut system,
                    )
                    .await;
                }

                if config.auto_freeplay {
                    tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
                    log::info!("Auto-Freeplay: Navigating to Free Play...");
                    crate::automation::simulate_sequence(
                        &config.auto_freeplay_sequence,
                        "Auto-Freeplay",
                        0,
                        &mut system,
                    )
                    .await;
                }

                state_clone
                    .system
                    .is_simulating_input
                    .store(false, Ordering::SeqCst);
            });

            if state.system.config.load().ballchasing_enabled {
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
        _ => log::debug!("Received event: {}", parsed_event.event_name),
    }
}

fn parse_event_for_state(state: &Arc<AppState>, json: &Value) -> StatsApiEvent {
    let current_local_name = state.game.local_player_name.load();
    let cached_identity = state.game.local_player_identity.load();
    let previous_players = state.game.players.load();
    parse_stats_api_event(
        json,
        StatsApiParseContext {
            current_local_name: current_local_name.trim(),
            cached_identity: &cached_identity,
            previous_players: &previous_players,
        },
    )
}

fn handle_update_state(state: &Arc<AppState>, parsed_event: &StatsApiEvent) {
    update_match_guid_diagnostics(state, parsed_event.match_guid.as_deref().unwrap_or(""));

    if parsed_event.is_replay || state.flags.is_watching_replay.load(Ordering::SeqCst) {
        update_session_from_event(state, parsed_event, false);
        return;
    }

    let has_known_local_name = !state.game.local_player_name.load().trim().is_empty();
    apply_local_player_update(state, parsed_event);

    if !has_known_local_name
        && parsed_event.local_player_hint.is_none()
        && !parsed_event.has_target
        && let Some(target_name) = parsed_event.target_name.as_ref()
    {
        state
            .game
            .local_player_name
            .store(Arc::new(target_name.clone()));
    }

    if state.game.local_team.load(Ordering::SeqCst) == crate::state::NO_TEAM
        && parsed_event.local_player_hint.is_none()
        && let Some(target_team) = parsed_event.target_team
    {
        state.game.local_team.store(target_team, Ordering::SeqCst);
    }

    if !parsed_event.players.is_empty() {
        // println!("State Updated: {} players in lobby", new_players.len());
    }
    let mut players = parsed_event.players.clone();
    let previous_players = state.game.players.load();
    let config = state.system.config.load();
    if config.debounce_touch_counters {
        let now = std::time::Instant::now();
        debounce_touch_counters(state, &mut players, now);
        if config.estimate_teammate_bumps {
            estimate_teammate_bumps(state, &previous_players, &players, now);
        } else {
            clear_teammate_bump_estimator(state);
        }
    } else {
        clear_teammate_bump_estimator(state);
    }
    let roster_changed = store_players_preserving_mmr(state, players.clone());
    let parsed_event = StatsApiEvent {
        players,
        ..parsed_event.clone()
    };
    update_match_roster(state, &parsed_event);

    update_session_from_event(state, &parsed_event, roster_changed);
    update_dashboard_match_snapshot(state, &parsed_event);
}

fn update_match_roster(state: &Arc<AppState>, parsed_event: &StatsApiEvent) {
    let match_guid = parsed_event.mode.match_guid.trim();
    if match_guid.is_empty() || parsed_event.players.is_empty() {
        return;
    }

    let frame_roster: HashMap<String, PlayerInfo> = parsed_event
        .players
        .values()
        .filter_map(|player| {
            crate::history::player_key(player).map(|key| (key.as_str().to_string(), player.clone()))
        })
        .collect();
    if frame_roster.is_empty() {
        return;
    }

    let current_guid = state.game.match_roster_guid.load();
    if current_guid.as_str() != match_guid {
        state
            .game
            .match_roster_guid
            .store(Arc::new(match_guid.to_string()));
        state.game.match_roster.store(Arc::new(frame_roster));
        return;
    }
    drop(current_guid);

    state.game.match_roster.rcu(|current_roster| {
        let mut merged = (**current_roster).clone();
        for (key, player) in &frame_roster {
            merged.insert(key.clone(), player.clone());
        }
        Arc::new(merged)
    });
}

fn is_online_player(player: &PlayerInfo) -> bool {
    !player.is_local && !player.is_bot
}

fn apply_local_player_update(state: &Arc<AppState>, parsed_event: &StatsApiEvent) {
    let Some(local_player) = parsed_event.local_player_hint.as_ref() else {
        return;
    };

    state
        .game
        .local_player_name
        .store(Arc::new(local_player.name.clone()));
    state
        .game
        .local_team
        .store(local_player.team, Ordering::SeqCst);
    let first_known_identity = state.update_local_player_identity(local_player.identity.clone());
    if first_known_identity {
        crate::mmr::start_local_mmr_refresh(state.clone());
    }
}

fn store_players_preserving_mmr(
    state: &Arc<AppState>,
    players: HashMap<String, PlayerInfo>,
) -> bool {
    let previous_players = state.game.players.load();
    let history_roster_changed =
        RosterSignature::from_players(&previous_players) != RosterSignature::from_players(&players);
    let new_players_arc = Arc::new(players);
    state.game.players.rcu(|current_players| {
        let needs_merge = current_players.iter().any(|(name, player)| {
            player.mmr.is_some()
                && new_players_arc
                    .get(name)
                    .is_some_and(|new_player| new_player.mmr.is_none())
        });

        if !needs_merge {
            return new_players_arc.clone();
        }

        let mut final_players = (*new_players_arc).clone();
        for (name, player) in final_players.iter_mut() {
            if player.mmr.is_none()
                && let Some(prev) = current_players.get(name)
            {
                player.mmr = prev.mmr.clone();
            }
        }
        Arc::new(final_players)
    });
    if history_roster_changed {
        update_roster_change_diagnostics(state);
        crate::history::refresh_lobby_history(state);
    }
    history_roster_changed
}

const BALL_TOUCH_DEBOUNCE_MS: u64 = 200;
const CAR_TOUCH_DEBOUNCE_MS: u64 = 450;

fn debounce_touch_counters(
    state: &Arc<AppState>,
    players: &mut HashMap<String, PlayerInfo>,
    now: std::time::Instant,
) {
    let Ok(mut debounce) = state.game.touch_counter_debounce.lock() else {
        return;
    };
    debounce.retain(|key, _| {
        players
            .values()
            .any(|player| touch_debounce_key(player) == *key)
    });

    for player in players.values_mut() {
        let key = touch_debounce_key(player);
        let entry = match debounce.entry(key) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let mut state = TouchCounterDebounce::new(player);
                if state.accepted_touches > 0 {
                    state.last_touch_increment_at = Some(now);
                }
                if state.accepted_car_touches > 0 {
                    state.last_car_touch_increment_at = Some(now);
                }
                entry.insert(state)
            }
        };

        player.touches = debounced_touch_value(
            player.touches,
            &mut entry.accepted_touches,
            &mut entry.last_touch_increment_at,
            std::time::Duration::from_millis(BALL_TOUCH_DEBOUNCE_MS),
            now,
        );
        player.car_touches = debounced_touch_value(
            player.car_touches,
            &mut entry.accepted_car_touches,
            &mut entry.last_car_touch_increment_at,
            std::time::Duration::from_millis(CAR_TOUCH_DEBOUNCE_MS),
            now,
        );
    }
}

fn estimate_teammate_bumps(
    state: &Arc<AppState>,
    previous_players: &HashMap<String, PlayerInfo>,
    players: &HashMap<String, PlayerInfo>,
    now: std::time::Instant,
) {
    let accepted_increments = accepted_car_touch_increments(previous_players, players);
    let Ok(mut estimator) = state.game.teammate_bump_estimator.lock() else {
        return;
    };
    let window = std::time::Duration::from_millis(CAR_TOUCH_DEBOUNCE_MS);
    estimator
        .pending
        .retain(|_, touch| now.duration_since(touch.at) <= window);

    for (key, team) in accepted_increments {
        if team <= 1 {
            estimator
                .pending
                .insert(key, TeammateBumpTouch { team, at: now });
        }
    }

    let pending: Vec<_> = estimator
        .pending
        .iter()
        .map(|(key, touch)| (key.clone(), *touch))
        .collect();
    if pending.len() != 2 {
        return;
    }

    let team = pending[0].1.team;
    if pending[1].1.team != team || team > 1 {
        return;
    }

    estimator.team_bumps[team as usize] += 1;
    for (key, _) in pending {
        estimator.pending.remove(&key);
    }
}

fn clear_teammate_bump_estimator(state: &AppState) {
    if let Ok(mut estimator) = state.game.teammate_bump_estimator.lock() {
        estimator.pending.clear();
        estimator.team_bumps = [0, 0];
    }
}

fn accepted_car_touch_increments(
    previous_players: &HashMap<String, PlayerInfo>,
    players: &HashMap<String, PlayerInfo>,
) -> Vec<(String, u8)> {
    let previous_by_key: HashMap<_, _> = previous_players
        .values()
        .map(|player| (touch_debounce_key(player), player.car_touches))
        .collect();

    players
        .values()
        .filter_map(|player| {
            let key = touch_debounce_key(player);
            let previous = previous_by_key.get(&key)?;
            (player.car_touches > *previous).then_some((key, player.team))
        })
        .collect()
}

fn debounced_touch_value(
    raw_value: u32,
    accepted_value: &mut u32,
    last_increment_at: &mut Option<std::time::Instant>,
    debounce_window: std::time::Duration,
    now: std::time::Instant,
) -> u32 {
    if raw_value < *accepted_value {
        *accepted_value = raw_value;
        *last_increment_at = None;
        return raw_value;
    }
    if raw_value == *accepted_value {
        return raw_value;
    }

    if last_increment_at.is_some_and(|last| now.duration_since(last) < debounce_window) {
        return *accepted_value;
    }

    *accepted_value = raw_value;
    *last_increment_at = Some(now);
    raw_value
}

fn touch_debounce_key(player: &PlayerInfo) -> String {
    crate::history::player_key(player)
        .map(|key| key.as_str().to_string())
        .unwrap_or_else(|| player.name.trim().to_ascii_lowercase())
}

fn update_session_from_event(
    state: &Arc<AppState>,
    parsed_event: &StatsApiEvent,
    roster_changed: bool,
) {
    let local_team = state.game.local_team.load(Ordering::SeqCst);
    let local_team_hint = (local_team != crate::state::NO_TEAM).then_some(local_team);
    let session_mode = parsed_event.mode.session_mode();
    let current_session = state.game.session.load();
    let score_changed = current_session.blue_score != parsed_event.score.blue_score
        || current_session.orange_score != parsed_event.score.orange_score;
    let result_pending = parsed_event.has_winner
        && !current_session.matches_last_recorded_result(
            &parsed_event.data,
            local_team_hint,
            session_mode,
        );
    let mode_changed = current_session.active_match_id != parsed_event.mode.match_guid
        || ((!current_session.round_started
            || current_session.active_mode == SessionMode::Unknown)
            && session_mode != SessionMode::Unknown
            && current_session.active_mode != session_mode)
        || (local_team_hint.is_some() && current_session.local_team != local_team_hint);
    let round_start_relevant =
        !current_session.round_started && players_have_round_stats(parsed_event);

    if roster_changed
        || score_changed
        || result_pending
        || mode_changed
        || round_start_relevant
        || current_session.would_change(&parsed_event.data, local_team_hint, session_mode)
    {
        let mut session = (**current_session).clone();
        let matches_before = session.matches_played;
        session.handle_update_state(&parsed_event.data, local_team_hint, session_mode);
        state
            .flags
            .is_watching_replay
            .store(session.is_watching_replay, Ordering::SeqCst);
        if session.matches_played > matches_before {
            update_result_diagnostics(state, &session, "update_state");
            crate::history::record_completed_match(state, &session);
        } else if parsed_event.has_winner
            && session.matches_last_recorded_result(
                &parsed_event.data,
                local_team_hint,
                session_mode,
            )
        {
            update_duplicate_result_diagnostics(state, "duplicate_result_signature");
        }
        state.game.session.store(Arc::new(session));
    }
}

fn update_dashboard_match_snapshot(state: &Arc<AppState>, parsed_event: &StatsApiEvent) {
    let match_guid = parsed_event.mode.match_guid.trim();
    if match_guid.is_empty() || parsed_event.players.is_empty() {
        return;
    }

    let current_session = state.game.session.load();
    if current_session.is_watching_replay {
        return;
    }

    let local_team = state.game.local_team.load(Ordering::SeqCst);
    let local_team = (local_team != crate::state::NO_TEAM).then_some(local_team);
    let session = (**current_session).clone();
    let team_bumps = state
        .game
        .teammate_bump_estimator
        .lock()
        .map(|estimator| estimator.team_bumps)
        .unwrap_or([0, 0]);
    state.game.dashboard_match_snapshot.rcu(|current| {
        let mut snapshot = if current.match_guid == match_guid {
            let mut snapshot = (**current).clone();
            for (name, player) in &parsed_event.players {
                snapshot.players.insert(name.clone(), player.clone());
            }
            snapshot
        } else {
            DashboardMatchSnapshot {
                match_guid: match_guid.to_string(),
                players: parsed_event.players.clone(),
                ..Default::default()
            }
        };

        snapshot.session = session.clone();
        snapshot.local_team = local_team.or(session.local_team);
        snapshot.team_bumps = team_bumps;
        Arc::new(snapshot)
    });
}

fn players_have_round_stats(parsed_event: &StatsApiEvent) -> bool {
    parsed_event.players.values().any(|player| {
        player.score > 0
            || player.touches > 0
            || player.goals > 0
            || player.saves > 0
            || player.demos > 0
    })
}

fn update_diagnostics<F>(state: &AppState, mut f: F)
where
    F: FnMut(&mut crate::state::NetworkDiagnostics),
{
    let f_ref = &mut f;
    state.system.network_diagnostics.rcu(|d| {
        let mut d_clone = (**d).clone();
        f_ref(&mut d_clone);
        Arc::new(d_clone)
    });
}

fn update_transport(state: &AppState, transport: StatsApiTransport) {
    update_diagnostics(state, |d| {
        d.transport = transport;
        d.last_connection_error.clear();
    });
}

fn update_last_event(state: &AppState, event: &str) {
    let current = state.system.network_diagnostics.load();
    let now = crate::stats_api::now_ms();
    let elapsed_ms = now.saturating_sub(current.last_event_unix_ms);
    if current.last_event == event && current.last_parse_error.is_empty() && elapsed_ms < 1000 {
        return;
    }
    drop(current);

    update_diagnostics(state, |d| {
        d.last_event = event.to_string();
        d.last_event_unix_ms = now;
        d.last_event_rate_estimate = if elapsed_ms > 0 && elapsed_ms < 10_000 {
            format!("{:.1}/s", 1000.0 / elapsed_ms as f64)
        } else {
            String::new()
        };
        d.last_parse_error.clear();
    });
}

fn update_roster_change_diagnostics(state: &AppState) {
    update_diagnostics(state, |d| {
        d.last_roster_signature_change_unix_ms = crate::stats_api::now_ms();
    });
}

fn update_match_guid_diagnostics(state: &AppState, match_guid: &str) {
    update_diagnostics(state, |d| {
        d.last_match_guid = match_guid.to_string();
    });
}

fn update_result_diagnostics(
    state: &AppState,
    session: &crate::session::SessionState,
    source: &str,
) {
    update_diagnostics(state, |d| {
        d.last_result_signature = format!(
            "source={source}, mode={}, local_team={}, blue={}, orange={}, result={}",
            session.active_mode.label(),
            session
                .local_team
                .map(|team| team.to_string())
                .unwrap_or_else(|| "Unknown".to_string()),
            session.blue_score,
            session.orange_score,
            session.last_result.label()
        );
        d.last_duplicate_result_suppression_reason.clear();
    });
}

fn update_duplicate_result_diagnostics(state: &AppState, reason: &str) {
    update_diagnostics(state, |d| {
        d.last_duplicate_result_suppression_reason = reason.to_string();
    });
}

fn update_parse_error(state: &AppState, error: String) {
    update_diagnostics(state, |d| {
        d.last_parse_error = error.clone();
    });
}

fn update_connection_error(state: &AppState, error: String) {
    update_diagnostics(state, |d| {
        d.last_connection_error = error.clone();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{SequenceStep, parse_sequence};
    use crate::stats_api_parser::parse_platform;
    use serde_json::json;
    use std::sync::atomic::Ordering;

    fn handle_update_state_payload(state: &Arc<AppState>, data: &Value) {
        handle_event(state, &json!({ "Event": "UpdateState", "Data": data }));
    }

    #[test]
    fn test_parse_platform() {
        assert_eq!(parse_platform("Steam|123|0"), ("Steam".to_string(), false));
        assert_eq!(parse_platform("Epic|456|0"), ("Epic".to_string(), false));
        assert_eq!(parse_platform("Ps4|789|0"), ("PSN".to_string(), false));
        assert_eq!(parse_platform("Ps5|012|0"), ("PSN".to_string(), false));
        assert_eq!(parse_platform("PS4|789|0"), ("PSN".to_string(), false));
        assert_eq!(parse_platform("PS5|012|0"), ("PSN".to_string(), false));
        assert_eq!(parse_platform("Xbox|345|0"), ("Xbox".to_string(), false));
        assert_eq!(parse_platform("XboxOne|345|0"), ("Xbox".to_string(), false));
        assert_eq!(parse_platform("XBoxOne|345|0"), ("Xbox".to_string(), false));
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
            parse_sequence("T,G,G,Enter", 0),
            vec![
                SequenceStep::Key(rdev::Key::KeyT),
                SequenceStep::Key(rdev::Key::KeyG),
                SequenceStep::Key(rdev::Key::KeyG),
                SequenceStep::Key(rdev::Key::Return)
            ]
        );
        assert_eq!(
            parse_sequence("1,1", 0),
            vec![
                SequenceStep::Key(rdev::Key::Num1),
                SequenceStep::Key(rdev::Key::Num1)
            ]
        );
        assert_eq!(
            parse_sequence("KeyT KeyG KeyG Return", 0),
            vec![
                SequenceStep::Key(rdev::Key::KeyT),
                SequenceStep::Key(rdev::Key::KeyG),
                SequenceStep::Key(rdev::Key::KeyG),
                SequenceStep::Key(rdev::Key::Return)
            ]
        );
        assert_eq!(
            parse_sequence("Escape, Delay400, Return", 200),
            vec![
                SequenceStep::Key(rdev::Key::Escape),
                SequenceStep::Delay(std::time::Duration::from_millis(200)),
                SequenceStep::Delay(std::time::Duration::from_millis(400)),
                SequenceStep::Key(rdev::Key::Return),
                SequenceStep::Delay(std::time::Duration::from_millis(200))
            ]
        );
    }

    #[test]
    fn store_players_preserves_existing_mmr_when_update_omits_it() {
        let state = AppState::new();
        let mut playlists = std::collections::HashMap::new();
        playlists.insert(
            13,
            crate::mmr::TrackerPlaylistSnapshot {
                name: "Ranked Doubles 2v2".to_string(),
                rating: 1234,
                matches: 10,
                tier_name: "Champion I".to_string(),
            },
        );
        let snapshot = crate::mmr::TrackerSnapshot {
            playlists,
            last_updated: Some("now".to_string()),
            current_season: Some(20),
        };

        let mut initial = std::collections::HashMap::new();
        initial.insert(
            "Opponent".to_string(),
            crate::state::PlayerInfo {
                name: "Opponent".to_string(),
                primary_id: "Epic|2|0".to_string(),
                platform: "Epic".to_string(),
                mmr: Some(snapshot),
                ..Default::default()
            },
        );
        store_players_preserving_mmr(&state, initial);

        let mut update_without_mmr = std::collections::HashMap::new();
        update_without_mmr.insert(
            "Opponent".to_string(),
            crate::state::PlayerInfo {
                name: "Opponent".to_string(),
                primary_id: "Epic|2|0".to_string(),
                platform: "Epic".to_string(),
                boost: 80,
                mmr: None,
                ..Default::default()
            },
        );
        store_players_preserving_mmr(&state, update_without_mmr);

        let players = state.game.players.load();
        let preserved = players["Opponent"].mmr.as_ref().unwrap();
        assert_eq!(preserved.playlists[&13].rating, 1234);
        assert_eq!(players["Opponent"].boost, 80);
    }

    #[test]
    fn touch_counter_debounce_suppresses_rapid_duplicate_increments() {
        let state = AppState::new();
        let start = std::time::Instant::now();
        let mut players = HashMap::new();
        players.insert(
            "Me".to_string(),
            PlayerInfo {
                name: "Me".to_string(),
                primary_id: "Steam|1|0".to_string(),
                platform: "Steam".to_string(),
                touches: 1,
                car_touches: 1,
                ..Default::default()
            },
        );
        debounce_touch_counters(&state, &mut players, start);
        assert_eq!(players["Me"].touches, 1);
        assert_eq!(players["Me"].car_touches, 1);

        players.get_mut("Me").unwrap().touches = 2;
        players.get_mut("Me").unwrap().car_touches = 2;
        debounce_touch_counters(
            &state,
            &mut players,
            start + std::time::Duration::from_millis(199),
        );
        assert_eq!(players["Me"].touches, 1);
        assert_eq!(players["Me"].car_touches, 1);

        players.get_mut("Me").unwrap().touches = 2;
        players.get_mut("Me").unwrap().car_touches = 2;
        debounce_touch_counters(
            &state,
            &mut players,
            start + std::time::Duration::from_millis(200),
        );
        assert_eq!(players["Me"].touches, 2);
        assert_eq!(players["Me"].car_touches, 1);

        players.get_mut("Me").unwrap().car_touches = 2;
        debounce_touch_counters(
            &state,
            &mut players,
            start + std::time::Duration::from_millis(450),
        );
        assert_eq!(players["Me"].car_touches, 2);
    }

    #[test]
    fn touch_counter_debounce_accepts_counter_resets() {
        let state = AppState::new();
        let start = std::time::Instant::now();
        let mut players = HashMap::new();
        players.insert(
            "Me".to_string(),
            PlayerInfo {
                name: "Me".to_string(),
                primary_id: "Steam|1|0".to_string(),
                platform: "Steam".to_string(),
                touches: 4,
                car_touches: 3,
                ..Default::default()
            },
        );
        debounce_touch_counters(&state, &mut players, start);

        players.get_mut("Me").unwrap().touches = 0;
        players.get_mut("Me").unwrap().car_touches = 0;
        debounce_touch_counters(
            &state,
            &mut players,
            start + std::time::Duration::from_millis(20),
        );

        assert_eq!(players["Me"].touches, 0);
        assert_eq!(players["Me"].car_touches, 0);
    }

    fn touch_player(name: &str, team: u8, car_touches: u32) -> PlayerInfo {
        PlayerInfo {
            name: name.to_string(),
            team,
            car_touches,
            ..Default::default()
        }
    }

    #[test]
    fn teammate_bump_estimator_counts_same_team_pair_once() {
        let state = AppState::new();
        let start = std::time::Instant::now();
        let mut previous = HashMap::new();
        previous.insert("BlueOne".to_string(), touch_player("BlueOne", 0, 2));
        previous.insert("BlueTwo".to_string(), touch_player("BlueTwo", 0, 4));

        let mut players = previous.clone();
        players.get_mut("BlueOne").unwrap().car_touches = 3;
        estimate_teammate_bumps(&state, &previous, &players, start);
        assert_eq!(
            state
                .game
                .teammate_bump_estimator
                .lock()
                .unwrap()
                .team_bumps,
            [0, 0]
        );

        previous = players.clone();
        players.get_mut("BlueTwo").unwrap().car_touches = 5;
        estimate_teammate_bumps(
            &state,
            &previous,
            &players,
            start + std::time::Duration::from_millis(300),
        );

        assert_eq!(
            state
                .game
                .teammate_bump_estimator
                .lock()
                .unwrap()
                .team_bumps,
            [1, 0]
        );
    }

    #[test]
    fn teammate_bump_estimator_ignores_window_with_opponent_increment() {
        let state = AppState::new();
        let start = std::time::Instant::now();
        let mut previous = HashMap::new();
        previous.insert("BlueOne".to_string(), touch_player("BlueOne", 0, 1));
        previous.insert("BlueTwo".to_string(), touch_player("BlueTwo", 0, 1));
        previous.insert("OrangeOne".to_string(), touch_player("OrangeOne", 1, 1));

        let mut players = previous.clone();
        players.get_mut("BlueOne").unwrap().car_touches = 2;
        players.get_mut("BlueTwo").unwrap().car_touches = 2;
        players.get_mut("OrangeOne").unwrap().car_touches = 2;
        estimate_teammate_bumps(&state, &previous, &players, start);

        assert_eq!(
            state
                .game
                .teammate_bump_estimator
                .lock()
                .unwrap()
                .team_bumps,
            [0, 0]
        );
    }

    #[test]
    fn teammate_bump_estimator_ignores_suppressed_debounce_increment() {
        let state = AppState::new();
        let start = std::time::Instant::now();
        let mut previous = HashMap::new();
        previous.insert("BlueOne".to_string(), touch_player("BlueOne", 0, 1));
        previous.insert("BlueTwo".to_string(), touch_player("BlueTwo", 0, 1));

        let players = previous.clone();
        estimate_teammate_bumps(&state, &previous, &players, start);

        assert_eq!(
            state
                .game
                .teammate_bump_estimator
                .lock()
                .unwrap()
                .team_bumps,
            [0, 0]
        );
    }

    #[test]
    fn dashboard_match_snapshot_keeps_players_until_next_match() {
        let state = AppState::new();
        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "match-one",
                "Game": {
                    "Teams": [
                        {"TeamNum": 0, "Score": 1},
                        {"TeamNum": 1, "Score": 0}
                    ]
                },
                "Players": [
                    {
                        "Name": "Me",
                        "PrimaryId": "Steam|1|0",
                        "TeamNum": 0,
                        "Score": 500,
                        "IsLocalPlayer": true
                    },
                    {
                        "Name": "Opponent",
                        "PrimaryId": "Epic|2|0",
                        "TeamNum": 1,
                        "Score": 300
                    }
                ]
            }),
        );

        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "match-one",
                "Game": {
                    "Teams": [
                        {"TeamNum": 0, "Score": 2},
                        {"TeamNum": 1, "Score": 0}
                    ]
                },
                "Players": [
                    {
                        "Name": "Me",
                        "PrimaryId": "Steam|1|0",
                        "TeamNum": 0,
                        "Score": 700,
                        "IsLocalPlayer": true
                    }
                ]
            }),
        );

        let snapshot = state.game.dashboard_match_snapshot.load();
        assert_eq!(snapshot.match_guid, "match-one");
        assert_eq!(snapshot.players["Me"].score, 700);
        assert_eq!(snapshot.players["Opponent"].score, 300);
        assert_eq!(snapshot.session.blue_score, 2);
        drop(snapshot);

        handle_update_state_payload(
            &state,
            &json!({
                "Players": [
                    {
                        "Name": "Me",
                        "PrimaryId": "Steam|1|0",
                        "TeamNum": 0,
                        "Score": 0,
                        "IsLocalPlayer": true
                    }
                ]
            }),
        );

        let snapshot = state.game.dashboard_match_snapshot.load();
        assert_eq!(snapshot.match_guid, "match-one");
        assert_eq!(snapshot.players["Opponent"].score, 300);
    }

    #[test]
    fn replay_update_state_does_not_overwrite_match_player_stats() {
        let state = AppState::new();
        let mut config = (*state.system.config.load_full()).clone();
        config.debounce_touch_counters = true;
        config.estimate_teammate_bumps = true;
        state.system.config.store(Arc::new(config));

        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "match-one",
                "Game": {
                    "Teams": [
                        {"TeamNum": 0, "Score": 1},
                        {"TeamNum": 1, "Score": 0}
                    ],
                    "bReplay": false,
                    "bHasWinner": false
                },
                "Players": [
                    {
                        "Name": "Me",
                        "PrimaryId": "Steam|1|0",
                        "TeamNum": 0,
                        "Score": 300,
                        "Touches": 4,
                        "CarTouches": 2,
                        "IsLocalPlayer": true
                    },
                    {
                        "Name": "Mate",
                        "PrimaryId": "Epic|2|0",
                        "TeamNum": 0,
                        "Score": 100,
                        "Touches": 2,
                        "CarTouches": 1
                    },
                    {
                        "Name": "Opponent",
                        "PrimaryId": "Xbox|3|0",
                        "TeamNum": 1,
                        "Score": 50,
                        "Touches": 1,
                        "CarTouches": 1
                    }
                ]
            }),
        );

        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "match-one",
                "Game": {
                    "Teams": [
                        {"TeamNum": 0, "Score": 1},
                        {"TeamNum": 1, "Score": 0}
                    ],
                    "bReplay": true,
                    "bHasWinner": false
                },
                "Players": [
                    {
                        "Name": "Me",
                        "PrimaryId": "Steam|1|0",
                        "TeamNum": 0,
                        "Score": 900,
                        "Touches": 40,
                        "CarTouches": 20,
                        "IsLocalPlayer": true
                    },
                    {
                        "Name": "Mate",
                        "PrimaryId": "Epic|2|0",
                        "TeamNum": 0,
                        "Score": 800,
                        "Touches": 30,
                        "CarTouches": 20
                    },
                    {
                        "Name": "Opponent",
                        "PrimaryId": "Xbox|3|0",
                        "TeamNum": 1,
                        "Score": 700,
                        "Touches": 20,
                        "CarTouches": 20
                    }
                ]
            }),
        );

        assert!(state.flags.is_watching_replay.load(Ordering::SeqCst));
        assert!(state.game.session.load().is_watching_replay);

        let players = state.game.players.load();
        assert_eq!(players["Me"].score, 300);
        assert_eq!(players["Me"].touches, 4);
        assert_eq!(players["Me"].car_touches, 2);
        assert_eq!(players["Mate"].score, 100);
        assert_eq!(players["Mate"].touches, 2);
        assert_eq!(players["Mate"].car_touches, 1);
        drop(players);

        let snapshot = state.game.dashboard_match_snapshot.load();
        assert_eq!(snapshot.players["Me"].score, 300);
        assert_eq!(snapshot.players["Me"].touches, 4);
        assert_eq!(snapshot.players["Me"].car_touches, 2);
        assert_eq!(snapshot.team_bumps, [0, 0]);
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

        handle_update_state_payload(&state, &data);

        let players = state.game.players.load();
        assert_eq!(&**state.game.local_player_name.load(), "Me");
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

        handle_update_state_payload(&state, &data);

        let players = state.game.players.load();
        assert_eq!(&**state.game.local_player_name.load(), "cyberPeng");
        assert_eq!(state.game.local_team.load(Ordering::SeqCst), 0);
        assert!(players["cyberPeng"].is_local);
        assert_eq!(players["C-Block"].team, 0);
        assert_eq!(players["C-Block"].boost, 88);
    }

    #[test]
    fn test_update_state_uses_cached_local_player_name_without_local_flag() {
        let state = AppState::new();
        state
            .game
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

        handle_update_state_payload(&state, &data);

        let players = state.game.players.load();
        assert!(players["CachedName"].is_local);
        assert_eq!(state.game.local_team.load(Ordering::SeqCst), 1);
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
            .game
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

        handle_update_state_payload(&state, &data);

        let players = state.game.players.load();
        // The spectated player should not be marked as local
        assert!(!players["SpectatedPlayer"].is_local);
        // The local player identity should still be MyRealName
        assert_eq!(state.game.local_player_identity.load().name, "MyRealName");
        assert_eq!(**state.game.local_player_name.load(), "MyRealName");
    }

    #[test]
    fn test_lobby_event_clears_websocket_state() {
        let state = AppState::new();
        handle_update_state_payload(
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
        state.flags.is_connected.store(true, Ordering::SeqCst);

        handle_event(&state, &json!({ "Event": "LobbyEntered" }));

        assert!(state.game.players.load().is_empty());
        assert_eq!(&**state.game.local_player_name.load(), "");
    }

    #[test]
    fn test_lobby_event_keeps_local_identity_for_manual_mmr_refresh() {
        let state = AppState::new();
        handle_update_state_payload(
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

        let identity = state.game.local_player_identity.load();
        assert_eq!(identity.name, "Me");
        assert_eq!(identity.platform, "Steam");
        assert_eq!(identity.primary_id, "Steam|76561198000000000|0");

        let config = state.system.config.load();
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
        handle_update_state_payload(
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

        assert_eq!(state.game.session.load().active_match_id, "guid123");

        handle_event(&state, &json!({ "Event": "RoundStarted" }));
        handle_event(&state, &json!({ "Event": "LobbyEntered" }));

        let session = state.game.session.load();
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
        handle_update_state_payload(
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

        let session = state.game.session.load();
        assert_eq!(session.losses, 0);
        assert_eq!(session.matches_played, 0);
    }

    #[test]
    fn test_update_state_infers_session_mode_from_total_players() {
        let state = AppState::new();
        handle_update_state_payload(
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
            state.game.session.load().active_mode,
            crate::session::SessionMode::Twos
        );
    }

    #[test]
    fn test_round_started_partial_roster_can_correct_to_twos() {
        let state = AppState::new();
        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "guid123",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true},
                    {"Name": "Opp1", "PrimaryId": "Xbox|3|0", "TeamNum": 1}
                ],
                "Game": {
                    "Arena": "Stadium_P"
                }
            }),
        );

        assert_eq!(
            state.game.session.load().active_mode,
            crate::session::SessionMode::Ones
        );

        handle_event(&state, &json!({ "Event": "RoundStarted" }));
        handle_update_state_payload(
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
                    "Arena": "Stadium_P"
                }
            }),
        );

        let session = state.game.session.load();
        assert!(session.round_started);
        assert_eq!(session.active_mode, crate::session::SessionMode::Twos);
    }

    #[test]
    fn test_update_state_without_players_uses_unknown_session_mode() {
        let state = AppState::new();
        handle_update_state_payload(
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

        let session = state.game.session.load();
        assert_eq!(session.wins, 0);
        assert!(session.mode_records.is_empty());
        assert_eq!(session.active_mode, crate::session::SessionMode::Unknown);
    }

    #[test]
    fn test_update_state_detects_freeplay_capture_shape() {
        let state = AppState::new();
        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "",
                "Players": [
                    {
                        "Name": "cyberPeng",
                        "PrimaryId": "Steam|76561197981997358|0",
                        "TeamNum": 0,
                        "Boost": 100
                    }
                ],
                "Game": {
                    "Arena": "Park_Rainy_P",
                    "bHasTarget": true,
                    "Target": {"Name": "cyberPeng", "TeamNum": 0}
                }
            }),
        );

        let players = state.game.players.load();
        assert_eq!(&**state.game.local_player_name.load(), "cyberPeng");
        assert_eq!(state.game.local_team.load(Ordering::SeqCst), 0);
        assert!(players["cyberPeng"].is_local);
        let session = state.game.session.load();
        assert_eq!(session.active_mode, crate::session::SessionMode::Freeplay);
        assert_eq!(session.matches_played, 0);
        assert!(session.mode_records.is_empty());
    }

    #[test]
    fn test_private_match_with_target_records_update_state_winner() {
        let state = AppState::new();
        let base_players = json!([
            {
                "Name": "cyberPeng",
                "PrimaryId": "Steam|76561197981997358|0",
                "TeamNum": 0,
                "Score": 124,
                "Goals": 1,
                "Touches": 18
            },
            {"Name": "Roundhouse", "PrimaryId": "Unknown|0|0", "TeamNum": 1},
            {"Name": "Viper", "PrimaryId": "Unknown|0|0", "TeamNum": 1},
            {"Name": "Jester", "PrimaryId": "Unknown|0|0", "TeamNum": 0},
            {"Name": "Samara", "PrimaryId": "Unknown|0|0", "TeamNum": 0},
            {"Name": "Caveman", "PrimaryId": "Unknown|0|0", "TeamNum": 1}
        ]);

        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "5D10ADA011F16578035ABBB9B9C3C4DE",
                "Players": base_players.clone(),
                "Game": {
                    "Arena": "Park_Night_P",
                    "Teams": [
                        {"TeamNum": 0, "Score": 0},
                        {"TeamNum": 1, "Score": 0}
                    ],
                    "bHasWinner": false,
                    "Winner": "",
                    "bHasTarget": true,
                    "Target": {"Name": "cyberPeng", "TeamNum": 0}
                }
            }),
        );
        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "5D10ADA011F16578035ABBB9B9C3C4DE",
                "Players": base_players,
                "Game": {
                    "Arena": "Park_Night_P",
                    "Teams": [
                        {"TeamNum": 0, "Score": 1},
                        {"TeamNum": 1, "Score": 0}
                    ],
                    "bReplay": false,
                    "bHasWinner": true,
                    "Winner": "Blue",
                    "bHasTarget": false
                }
            }),
        );

        let session = state.game.session.load();
        assert_eq!(state.game.local_team.load(Ordering::SeqCst), 0);
        assert_eq!(session.active_mode, crate::session::SessionMode::Threes);
        assert_eq!(session.wins, 1);
        assert_eq!(session.matches_played, 1);
        assert_eq!(
            session.mode_records[&crate::session::SessionMode::Threes].wins,
            1
        );
    }

    #[test]
    fn test_target_shortcut_sets_local_team_before_session_result() {
        let state = AppState::new();

        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "shortcut-session-guid",
                "Players": [
                    {
                        "Name": "WindowsPlayer",
                        "PrimaryId": "Steam|76561197981997358|0",
                        "Shortcut": 5,
                        "TeamNum": 1,
                        "Score": 550,
                        "Goals": 1,
                        "Touches": 8
                    },
                    {
                        "Name": "Mate",
                        "PrimaryId": "Epic|2|0",
                        "Shortcut": 6,
                        "TeamNum": 1
                    },
                    {
                        "Name": "OpponentOne",
                        "PrimaryId": "Xbox|3|0",
                        "Shortcut": 7,
                        "TeamNum": 0
                    },
                    {
                        "Name": "OpponentTwo",
                        "PrimaryId": "PS4|4|0",
                        "Shortcut": 8,
                        "TeamNum": 0
                    }
                ],
                "Game": {
                    "Arena": "Park_P",
                    "Teams": [
                        {"TeamNum": 0, "Score": 0},
                        {"TeamNum": 1, "Score": 1}
                    ],
                    "bReplay": false,
                    "bHasWinner": true,
                    "Winner": "Orange",
                    "bHasTarget": true,
                    "Target": {"Shortcut": 5, "TeamNum": 1}
                }
            }),
        );

        let players = state.game.players.load();
        assert_eq!(&**state.game.local_player_name.load(), "WindowsPlayer");
        assert_eq!(state.game.local_team.load(Ordering::SeqCst), 1);
        assert!(players["WindowsPlayer"].is_local);
        drop(players);

        let session = state.game.session.load();
        assert_eq!(session.active_mode, crate::session::SessionMode::Twos);
        assert_eq!(session.wins, 1);
        assert_eq!(session.matches_played, 1);
        assert_eq!(
            session.mode_records[&crate::session::SessionMode::Twos].wins,
            1
        );
    }

    #[test]
    fn test_update_state_prefers_arena_mode_over_player_count() {
        let state = AppState::new();
        handle_update_state_payload(
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

        let session = state.game.session.load();
        assert_eq!(session.active_mode, crate::session::SessionMode::Hoops);
        assert_eq!(
            session.mode_records[&crate::session::SessionMode::Hoops].wins,
            1
        );
    }

    #[test]
    fn test_offline_extra_mode_does_not_fall_back_to_ones() {
        let state = AppState::new();
        handle_update_state_payload(
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
            state.game.session.load().active_mode,
            crate::session::SessionMode::Hoops
        );
    }

    #[test]
    fn test_standard_offline_uses_total_player_count_for_mode() {
        let state = AppState::new();
        handle_update_state_payload(
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
            state.game.session.load().active_mode,
            crate::session::SessionMode::Ones
        );

        handle_update_state_payload(
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
            state.game.session.load().active_mode,
            crate::session::SessionMode::Twos
        );
    }

    #[test]
    fn test_targeted_opponent_at_match_end_does_not_turn_hoops_loss_into_win() {
        let state = AppState::new();
        handle_update_state_payload(
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

        handle_update_state_payload(
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

        let session = state.game.session.load();
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
        handle_update_state_payload(
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

        let session = state.game.session.load();
        assert_eq!(session.wins, 0);
        assert_eq!(session.losses, 1);
        assert_eq!(session.matches_played, 1);
        assert_eq!(
            session.mode_records[&crate::session::SessionMode::Hoops].losses,
            1
        );
        assert!(session.active_match_id.is_empty());
    }

    #[test]
    fn test_stat_only_frames_do_not_change_roster_signature_diagnostics() {
        let state = AppState::new();
        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "guid123",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true, "Boost": 10, "Score": 0},
                    {"Name": "Opponent", "PrimaryId": "Epic|2|0", "TeamNum": 1, "Boost": 20, "Score": 0}
                ],
                "Game": {
                    "Teams": [
                        {"TeamNum": 0, "Score": 0},
                        {"TeamNum": 1, "Score": 0}
                    ]
                }
            }),
        );
        let first_roster_change = state
            .system
            .network_diagnostics
            .load()
            .last_roster_signature_change_unix_ms;

        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "guid123",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true, "Boost": 99, "Score": 300, "Touches": 12},
                    {"Name": "Opponent", "PrimaryId": "Epic|2|0", "TeamNum": 1, "Boost": 0, "Score": 100, "Touches": 6}
                ],
                "Game": {
                    "Teams": [
                        {"TeamNum": 0, "Score": 0},
                        {"TeamNum": 1, "Score": 0}
                    ]
                }
            }),
        );

        let diagnostics = state.system.network_diagnostics.load();
        assert_eq!(
            diagnostics.last_roster_signature_change_unix_ms,
            first_roster_change
        );
        let players = state.game.players.load();
        assert_eq!(players["Me"].boost, 99);
        assert_eq!(players["Me"].score, 300);
    }

    #[test]
    fn test_match_ended_then_final_update_state_records_one_result() {
        let state = AppState::new();
        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "guid123",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true},
                    {"Name": "Opponent", "PrimaryId": "Epic|2|0", "TeamNum": 1}
                ],
                "Game": {
                    "Teams": [
                        {"TeamNum": 0, "Score": 3},
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
                "Data": {"MatchGuid": "guid123", "WinnerTeamNum": 0}
            }),
        );
        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "guid123",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true},
                    {"Name": "Opponent", "PrimaryId": "Epic|2|0", "TeamNum": 1}
                ],
                "Game": {
                    "Teams": [
                        {"TeamNum": 0, "Score": 3},
                        {"TeamNum": 1, "Score": 2}
                    ],
                    "bHasWinner": true,
                    "Winner": "Blue"
                }
            }),
        );

        let session = state.game.session.load();
        assert_eq!(session.wins, 1);
        assert_eq!(session.matches_played, 1);
    }

    #[test]
    fn test_history_records_player_who_left_before_final_frame() {
        let state = AppState::new();
        let _ = crate::history::clear_history(&state);
        let mut config = state.system.config.load().as_ref().clone();
        config.history_enabled = true;
        state.save_config(config);

        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "guid-left",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true},
                    {"Name": "Stayed", "PrimaryId": "Epic|2|0", "TeamNum": 1},
                    {"Name": "LeftEarly", "PrimaryId": "Xbox|3|0", "TeamNum": 1}
                ],
                "Game": {
                    "Teams": [
                        {"TeamNum": 0, "Score": 0},
                        {"TeamNum": 1, "Score": 0}
                    ]
                }
            }),
        );
        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "guid-left",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true},
                    {"Name": "Stayed", "PrimaryId": "Epic|2|0", "TeamNum": 1}
                ],
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

        let summaries = crate::history::load_all_player_summaries(&state).unwrap();
        let names: Vec<_> = summaries
            .iter()
            .map(|summary| summary.name.as_str())
            .collect();
        assert!(names.contains(&"Stayed"));
        assert!(names.contains(&"LeftEarly"));
        assert!(!names.contains(&"Me"));

        let _ = crate::history::clear_history(&state);
    }

    #[test]
    fn test_ready_up_new_guid_with_same_final_result_records_one_result() {
        let state = AppState::new();
        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "old-guid",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true},
                    {"Name": "Opponent", "PrimaryId": "Epic|2|0", "TeamNum": 1}
                ],
                "Game": {
                    "Teams": [
                        {"TeamNum": 0, "Score": 1},
                        {"TeamNum": 1, "Score": 0}
                    ],
                    "bHasWinner": true,
                    "Winner": "Blue"
                }
            }),
        );
        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "new-guid",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true},
                    {"Name": "Opponent", "PrimaryId": "Epic|2|0", "TeamNum": 1}
                ],
                "Game": {
                    "Teams": [
                        {"TeamNum": 0, "Score": 1},
                        {"TeamNum": 1, "Score": 0}
                    ],
                    "bHasWinner": true,
                    "Winner": "Blue"
                }
            }),
        );

        let session = state.game.session.load();
        assert_eq!(session.wins, 1);
        assert_eq!(session.matches_played, 1);
        assert_eq!(session.active_match_id, "old-guid");
    }

    #[test]
    fn test_match_destroyed_followed_by_stale_frames_does_not_double_count() {
        let state = AppState::new();
        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "guid123",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true},
                    {"Name": "Opponent", "PrimaryId": "Epic|2|0", "TeamNum": 1}
                ],
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
        handle_event(&state, &json!({"Event": "MatchDestroyed"}));
        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "guid123",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true},
                    {"Name": "Opponent", "PrimaryId": "Epic|2|0", "TeamNum": 1}
                ],
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

        let session = state.game.session.load();
        assert_eq!(session.wins, 1);
        assert_eq!(session.matches_played, 1);
    }

    #[test]
    fn test_replay_playback_bypasses_all_updates() {
        let state = AppState::new();

        // 1. Send ReplayPlaybackStart to mark replay mode active
        handle_event(&state, &json!({"Event": "ReplayPlaybackStart"}));
        assert!(state.flags.is_watching_replay.load(Ordering::SeqCst));
        assert!(state.game.session.load().is_watching_replay);

        // 2. Send UpdateState representing a completed match inside the replay
        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "guid123",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true},
                    {"Name": "Opponent", "PrimaryId": "Epic|2|0", "TeamNum": 1}
                ],
                "Game": {
                    "bReplay": true,
                    "Teams": [
                        {"TeamNum": 0, "Score": 3},
                        {"TeamNum": 1, "Score": 1}
                    ],
                    "bHasWinner": true,
                    "Winner": "Blue"
                }
            }),
        );

        // Verify no wins/losses/matches recorded
        let session = state.game.session.load();
        assert_eq!(session.wins, 0);
        assert_eq!(session.losses, 0);
        assert_eq!(session.matches_played, 0);

        // 3. Send MatchEnded event and verify it returns early and does not increment stats
        handle_event(
            &state,
            &json!({
                "Event": "MatchEnded",
                "Data": {
                    "MatchGuid": "guid123",
                    "WinnerTeamNum": 0
                }
            }),
        );
        let session2 = state.game.session.load();
        assert_eq!(session2.wins, 0);
        assert_eq!(session2.matches_played, 0);

        // 4. Send MatchDestroyed to simulate exiting the replay playback
        handle_event(&state, &json!({"Event": "MatchDestroyed"}));
        assert!(!state.flags.is_watching_replay.load(Ordering::SeqCst));
        assert!(!state.game.session.load().is_watching_replay);
    }

    #[test]
    fn test_replay_auto_detection_from_first_frame() {
        let state = AppState::new();

        // Simulate connecting mid-replay (no ReplayPlaybackStart event received).
        // Send first UpdateState frame with bReplay: true.
        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "guid999",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true},
                    {"Name": "Opponent", "PrimaryId": "Epic|2|0", "TeamNum": 1}
                ],
                "Game": {
                    "Teams": [
                        {"TeamNum": 0, "Score": 0},
                        {"TeamNum": 1, "Score": 0}
                    ],
                    "bReplay": true,
                    "bHasWinner": false,
                    "Winner": ""
                }
            }),
        );

        // Verify that is_watching_replay was automatically set to true on both flags and session
        assert!(state.flags.is_watching_replay.load(Ordering::SeqCst));
        assert!(state.game.session.load().is_watching_replay);

        // Subsequent winner frame within this replay match does not record results
        handle_update_state_payload(
            &state,
            &json!({
                "MatchGuid": "guid999",
                "Players": [
                    {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true},
                    {"Name": "Opponent", "PrimaryId": "Epic|2|0", "TeamNum": 1}
                ],
                "Game": {
                    "Teams": [
                        {"TeamNum": 0, "Score": 5},
                        {"TeamNum": 1, "Score": 2}
                    ],
                    "bReplay": true,
                    "bHasWinner": true,
                    "Winner": "Blue"
                }
            }),
        );

        let session = state.game.session.load();
        assert_eq!(session.wins, 0);
        assert_eq!(session.matches_played, 0);
    }
}
