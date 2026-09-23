use crate::session::SessionMode;
use crate::state::{AppState, DashboardMatchSnapshot, PlayerInfo, PlayerMap, standard_team};
use crate::stats_api_parser::{
    RosterSignature, StatsApiEvent, StatsApiParseContext, parse_stats_api_event,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

mod touch_tracking;
mod transport;

use touch_tracking::{
    apply_replay_touch_offsets, clear_replay_touch_offsets, clear_teammate_bump_estimator,
    debounce_touch_counters, estimate_teammate_bumps, record_replay_touch_offsets,
};
pub use transport::{start_network_task, start_network_task_with_addr};

fn clear_lobby_state(state: &AppState) {
    state
        .game
        .player_roster_epoch
        .fetch_add(1, Ordering::SeqCst);
    state.game.players.store(Arc::new(HashMap::new()));
    if let Ok(mut debounce) = state.game.touch_counter_debounce.lock() {
        debounce.clear();
    }
    clear_teammate_bump_estimator(state);
    clear_replay_touch_offsets(state);
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
            let local_team_hint = standard_team(local_team);
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

            #[cfg(not(feature = "microsoft-store"))]
            let state_clone = state.clone();
            #[cfg(not(feature = "microsoft-store"))]
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
    let active_match_id = state.game.session.load().active_match_id.clone();
    let fallback_match_scope = if active_match_id.is_empty() {
        format!(
            "epoch-{}",
            state.game.player_roster_epoch.load(Ordering::SeqCst)
        )
    } else {
        active_match_id
    };
    parse_stats_api_event(
        json,
        StatsApiParseContext {
            current_local_name: current_local_name.trim(),
            cached_identity: &cached_identity,
            previous_players: &previous_players,
            fallback_match_scope: &fallback_match_scope,
            previous_match_scope: &fallback_match_scope,
        },
    )
}

fn handle_update_state(state: &Arc<AppState>, parsed_event: &StatsApiEvent) {
    update_match_guid_diagnostics(state, parsed_event.match_guid.as_deref().unwrap_or(""));

    let was_watching_replay = state.flags.is_watching_replay.load(Ordering::SeqCst);
    if parsed_event.is_replay {
        update_session_from_event(state, parsed_event, false);
        return;
    }
    if was_watching_replay {
        record_replay_touch_offsets(state, parsed_event);
        update_session_from_event(state, parsed_event, false);
        return;
    }

    let active_match_id = state.game.session.load().active_match_id.clone();
    if parsed_event
        .match_guid
        .as_deref()
        .is_some_and(|match_guid| !active_match_id.is_empty() && match_guid != active_match_id)
    {
        state
            .game
            .local_team
            .store(crate::state::NO_TEAM, Ordering::SeqCst);
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

    if standard_team(state.game.local_team.load(Ordering::SeqCst)).is_none()
        && parsed_event.local_player_hint.is_none()
        && let Some(target_team) = parsed_event.target_team
    {
        state.game.local_team.store(target_team, Ordering::SeqCst);
    }

    if !parsed_event.players.is_empty() {
        // println!("State Updated: {} players in lobby", new_players.len());
    }
    let mut players = parsed_event.players.clone();
    apply_replay_touch_offsets(state, parsed_event.match_guid.as_deref(), &mut players);
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

    let frame_roster = parsed_event.players.clone();
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
    if let Some(team) = standard_team(local_player.team) {
        state.game.local_team.store(team, Ordering::SeqCst);
    }
    let identity_requires_refresh =
        state.update_local_player_identity(local_player.identity.clone());
    if identity_requires_refresh {
        crate::mmr::start_local_mmr_refresh(state.clone());
    }
}

fn store_players_preserving_mmr(state: &Arc<AppState>, players: PlayerMap) -> bool {
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

fn update_session_from_event(
    state: &Arc<AppState>,
    parsed_event: &StatsApiEvent,
    roster_changed: bool,
) {
    let local_team = state.game.local_team.load(Ordering::SeqCst);
    let local_team_hint = standard_team(local_team);
    let mode_inference = parsed_event.mode.inference();
    let session_mode = mode_inference.mode;
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
        || (session_mode == current_session.active_mode
            && mode_inference
                .source
                .is_more_authoritative_than(current_session.active_mode_source))
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
        session.handle_update_state_with_mode_source(
            &parsed_event.data,
            local_team_hint,
            session_mode,
            mode_inference.source,
        );
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
    let local_team = standard_team(local_team);
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

#[cfg(test)]
mod tests;
