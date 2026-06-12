use crate::session::SessionMode;
use crate::state::{AppState, PlayerInfo};
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

async fn simulate_key_tap(key: rdev::Key) -> Result<(), rdev::SimulateError> {
    rdev::simulate(&rdev::EventType::KeyPress(key))?;
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    rdev::simulate(&rdev::EventType::KeyRelease(key))?;
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    Ok(())
}

async fn simulate_auto_key_tap(key: rdev::Key, action: &str) -> bool {
    if !rocket_league_accepts_auto_input() {
        log::info!("{action} skipped: Rocket League is not the foreground window.");
        return false;
    }

    if let Err(error) = simulate_key_tap(key).await {
        log::error!("{action} key simulation failed: {error:?}");
        return false;
    }

    true
}

async fn simulate_sequence(sequence: &str, action: &str, default_delay_ms: u64) {
    let steps = parse_sequence(sequence, default_delay_ms);
    if steps.is_empty() {
        log::error!("{action} skipped: no valid steps in sequence '{sequence}'.");
        return;
    }

    for step in steps {
        match step {
            SequenceStep::Key(key) => {
                if !simulate_auto_key_tap(key, action).await {
                    return;
                }
            }
            SequenceStep::Delay(dur) => {
                tokio::time::sleep(dur).await;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SequenceStep {
    Key(rdev::Key),
    Delay(std::time::Duration),
}

fn parse_sequence(seq: &str, default_delay_ms: u64) -> Vec<SequenceStep> {
    let mut steps = Vec::new();
    let tokens = seq.split([',', ' ', '+']);
    for token in tokens {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        let token_lower = token.to_lowercase();
        if token_lower.starts_with("delay") || token_lower.starts_with("wait") {
            let ms: u64 = token_lower
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .unwrap_or(default_delay_ms);
            steps.push(SequenceStep::Delay(std::time::Duration::from_millis(ms)));
        } else if let Some(key) = parse_auto_key(token) {
            steps.push(SequenceStep::Key(key));
            if default_delay_ms > 0 {
                steps.push(SequenceStep::Delay(std::time::Duration::from_millis(
                    default_delay_ms,
                )));
            }
        }
    }
    steps
}

fn parse_auto_key(token: &str) -> Option<rdev::Key> {
    let mut normalized = token
        .trim()
        .trim_matches(['[', ']'])
        .to_ascii_lowercase()
        .replace(['-', '_'], "");
    if normalized.is_empty() {
        return None;
    }
    if normalized.starts_with("key") && normalized.len() == 4 {
        normalized = normalized[3..].to_string();
    }

    match normalized.as_str() {
        "enter" | "return" => Some(rdev::Key::Return),
        "escape" | "esc" => Some(rdev::Key::Escape),
        "space" => Some(rdev::Key::Space),
        "tab" => Some(rdev::Key::Tab),
        "backspace" => Some(rdev::Key::Backspace),
        "uparrow" | "up" => Some(rdev::Key::UpArrow),
        "downarrow" | "down" => Some(rdev::Key::DownArrow),
        "leftarrow" | "left" => Some(rdev::Key::LeftArrow),
        "rightarrow" | "right" => Some(rdev::Key::RightArrow),
        "0" | "num0" | "key0" => Some(rdev::Key::Num0),
        "1" | "num1" | "key1" => Some(rdev::Key::Num1),
        "2" | "num2" | "key2" => Some(rdev::Key::Num2),
        "3" | "num3" | "key3" => Some(rdev::Key::Num3),
        "4" | "num4" | "key4" => Some(rdev::Key::Num4),
        "5" | "num5" | "key5" => Some(rdev::Key::Num5),
        "6" | "num6" | "key6" => Some(rdev::Key::Num6),
        "7" | "num7" | "key7" => Some(rdev::Key::Num7),
        "8" | "num8" | "key8" => Some(rdev::Key::Num8),
        "9" | "num9" | "key9" => Some(rdev::Key::Num9),
        "kp0" | "numpad0" => Some(rdev::Key::Kp0),
        "kp1" | "numpad1" => Some(rdev::Key::Kp1),
        "kp2" | "numpad2" => Some(rdev::Key::Kp2),
        "kp3" | "numpad3" => Some(rdev::Key::Kp3),
        "kp4" | "numpad4" => Some(rdev::Key::Kp4),
        "kp5" | "numpad5" => Some(rdev::Key::Kp5),
        "kp6" | "numpad6" => Some(rdev::Key::Kp6),
        "kp7" | "numpad7" => Some(rdev::Key::Kp7),
        "kp8" | "numpad8" => Some(rdev::Key::Kp8),
        "kp9" | "numpad9" => Some(rdev::Key::Kp9),
        "kpenter" | "numpadenter" => Some(rdev::Key::KpReturn),
        letter if letter.len() == 1 => match letter.as_bytes()[0] {
            b'a' => Some(rdev::Key::KeyA),
            b'b' => Some(rdev::Key::KeyB),
            b'c' => Some(rdev::Key::KeyC),
            b'd' => Some(rdev::Key::KeyD),
            b'e' => Some(rdev::Key::KeyE),
            b'f' => Some(rdev::Key::KeyF),
            b'g' => Some(rdev::Key::KeyG),
            b'h' => Some(rdev::Key::KeyH),
            b'i' => Some(rdev::Key::KeyI),
            b'j' => Some(rdev::Key::KeyJ),
            b'k' => Some(rdev::Key::KeyK),
            b'l' => Some(rdev::Key::KeyL),
            b'm' => Some(rdev::Key::KeyM),
            b'n' => Some(rdev::Key::KeyN),
            b'o' => Some(rdev::Key::KeyO),
            b'p' => Some(rdev::Key::KeyP),
            b'q' => Some(rdev::Key::KeyQ),
            b'r' => Some(rdev::Key::KeyR),
            b's' => Some(rdev::Key::KeyS),
            b't' => Some(rdev::Key::KeyT),
            b'u' => Some(rdev::Key::KeyU),
            b'v' => Some(rdev::Key::KeyV),
            b'w' => Some(rdev::Key::KeyW),
            b'x' => Some(rdev::Key::KeyX),
            b'y' => Some(rdev::Key::KeyY),
            b'z' => Some(rdev::Key::KeyZ),
            _ => None,
        },
        _ => None,
    }
}

fn rocket_league_accepts_auto_input() -> bool {
    #[cfg(target_os = "windows")]
    {
        is_rocket_league_foreground_window()
    }

    #[cfg(not(target_os = "windows"))]
    {
        true
    }
}

#[cfg(target_os = "windows")]
fn is_rocket_league_foreground_window() -> bool {
    use sysinfo::{Pid, ProcessesToUpdate, System};
    use winapi::um::winuser::{GetForegroundWindow, GetWindowThreadProcessId};

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return false;
        }

        let mut process_id = 0;
        GetWindowThreadProcessId(hwnd, &mut process_id);
        if process_id == 0 {
            return false;
        }

        let pid = Pid::from_u32(process_id);
        let mut system = System::new();
        system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);

        system
            .process(pid)
            .is_some_and(|process| crate::assets::is_rocket_league_name(process.name()))
    }
}

fn handle_match_reset(state: &Arc<AppState>, early_leave: bool) {
    let players = state.game.players.load();
    let is_online = players.values().any(|p| !p.is_local && !p.is_bot);

    let mut session = (**state.game.session.load()).clone();
    let matches_before = session.matches_played;
    if early_leave && is_online {
        session.record_early_leave();
    }
    if session.matches_played > matches_before {
        update_result_diagnostics(state, &session, "early_leave");
        crate::history::record_completed_match(state, &session);
    }
    session.handle_reset_event();
    state.game.session.store(Arc::new(session));

    state.game.players.store(Arc::new(HashMap::new()));
    state.game.local_player_name.store(Arc::new("".to_string()));
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
                state.flags.is_connected.store(false, Ordering::SeqCst);
                state.game.players.store(Arc::new(HashMap::new()));
                state.game.local_player_name.store(Arc::new("".to_string()));
            }
            Err(e) => {
                if format!("{}", e).contains("invalid HTTP version") {
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
                                    log::error!("{message}");
                                    update_connection_error(&state, message);
                                    break;
                                }
                            }
                        }
                        state.flags.is_connected.store(false, Ordering::SeqCst);
                        state.game.players.store(Arc::new(HashMap::new()));
                        state.game.local_player_name.store(Arc::new("".to_string()));
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
        "RoundStarted" | "ClockUpdatedSeconds" | "BallHit" | "GoalScored" | "StatfeedEvent" => {
            let current_session = state.game.session.load();
            if !current_session.round_started {
                let mut session = (**current_session).clone();
                session.handle_round_started();
                state.game.session.store(Arc::new(session));
            }
        }
        "MatchEnded" => {
            let local_team = state.game.local_team.load(Ordering::SeqCst);
            let local_team_hint = (local_team != crate::state::NO_TEAM).then_some(local_team);
            let mut session = (**state.game.session.load()).clone();
            let matches_before = session.matches_played;
            session.handle_match_ended(&parsed_event.data, local_team_hint);
            if session.matches_played > matches_before {
                update_result_diagnostics(state, &session, "match_ended");
                crate::history::record_completed_match(state, &session);
            }
            state.game.session.store(Arc::new(session));

            handle_match_reset(state, false);

            let state_clone = state.clone();
            tokio::spawn(async move {
                let config = state_clone.system.config.load();
                if config.auto_gg {
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    log::info!("Auto-GG: sending configured key sequence...");
                    simulate_sequence(&config.auto_gg_sequence, "Auto-GG", 125).await;
                }

                if config.auto_freeplay {
                    tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
                    log::info!("Auto-Freeplay: Navigating to Free Play...");
                    simulate_sequence(&config.auto_freeplay_sequence, "Auto-Freeplay", 0).await;
                }
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
    let has_known_local_name = !state.game.local_player_name.load().trim().is_empty();
    apply_local_player_update(state, parsed_event);

    if !has_known_local_name
        && parsed_event.local_player_hint.is_none()
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

    update_match_guid_diagnostics(state, parsed_event.match_guid.as_deref().unwrap_or(""));

    if !parsed_event.players.is_empty() {
        // println!("State Updated: {} players in lobby", new_players.len());
    }
    let roster_changed = store_players_preserving_mmr(state, parsed_event.players.clone());

    update_session_from_event(state, parsed_event, roster_changed);
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
        || (session_mode != SessionMode::Unknown && current_session.active_mode != session_mode)
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

fn players_have_round_stats(parsed_event: &StatsApiEvent) -> bool {
    parsed_event.players.values().any(|player| {
        player.score > 0
            || player.touches > 0
            || player.goals > 0
            || player.saves > 0
            || player.demos > 0
    })
}

fn update_transport(state: &Arc<AppState>, transport: StatsApiTransport) {
    let mut diagnostics = (**state.system.network_diagnostics.load()).clone();
    diagnostics.transport = transport;
    diagnostics.last_connection_error.clear();
    state
        .system
        .network_diagnostics
        .store(Arc::new(diagnostics));
}

fn update_last_event(state: &Arc<AppState>, event: &str) {
    let current = state.system.network_diagnostics.load();
    let now = crate::stats_api::now_ms();
    let elapsed_ms = now.saturating_sub(current.last_event_unix_ms);
    if current.last_event == event && current.last_parse_error.is_empty() && elapsed_ms < 1000 {
        return;
    }

    let mut diagnostics = (**current).clone();
    diagnostics.last_event = event.to_string();
    diagnostics.last_event_unix_ms = now;
    diagnostics.last_event_rate_estimate = if elapsed_ms > 0 && elapsed_ms < 10_000 {
        format!("{:.1}/s", 1000.0 / elapsed_ms as f64)
    } else {
        String::new()
    };
    diagnostics.last_parse_error.clear();
    state
        .system
        .network_diagnostics
        .store(Arc::new(diagnostics));
}

fn update_roster_change_diagnostics(state: &Arc<AppState>) {
    let mut diagnostics = (**state.system.network_diagnostics.load()).clone();
    diagnostics.last_roster_signature_change_unix_ms = crate::stats_api::now_ms();
    state
        .system
        .network_diagnostics
        .store(Arc::new(diagnostics));
}

fn update_match_guid_diagnostics(state: &Arc<AppState>, match_guid: &str) {
    let mut diagnostics = (**state.system.network_diagnostics.load()).clone();
    diagnostics.last_match_guid = match_guid.to_string();
    state
        .system
        .network_diagnostics
        .store(Arc::new(diagnostics));
}

fn update_result_diagnostics(
    state: &Arc<AppState>,
    session: &crate::session::SessionState,
    source: &str,
) {
    let mut diagnostics = (**state.system.network_diagnostics.load()).clone();
    diagnostics.last_result_signature = format!(
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
    diagnostics.last_duplicate_result_suppression_reason.clear();
    state
        .system
        .network_diagnostics
        .store(Arc::new(diagnostics));
}

fn update_duplicate_result_diagnostics(state: &Arc<AppState>, reason: &str) {
    let mut diagnostics = (**state.system.network_diagnostics.load()).clone();
    diagnostics.last_duplicate_result_suppression_reason = reason.to_string();
    state
        .system
        .network_diagnostics
        .store(Arc::new(diagnostics));
}

fn update_parse_error(state: &Arc<AppState>, error: String) {
    let mut diagnostics = (**state.system.network_diagnostics.load()).clone();
    diagnostics.last_parse_error = error;
    state
        .system
        .network_diagnostics
        .store(Arc::new(diagnostics));
}

fn update_connection_error(state: &Arc<AppState>, error: String) {
    let mut diagnostics = (**state.system.network_diagnostics.load()).clone();
    diagnostics.last_connection_error = error;
    state
        .system
        .network_diagnostics
        .store(Arc::new(diagnostics));
}

#[cfg(test)]
mod tests {
    use super::*;
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
                    "bReplay": true,
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
}
