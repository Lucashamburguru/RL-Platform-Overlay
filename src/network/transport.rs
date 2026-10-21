#[cfg(test)]
use super::handle_event;
use super::{clear_lobby_state, handle_owned_event, update_diagnostics};
use crate::state::AppState;
use crate::stats_api::{StatsApiTransport, TcpJsonSplitter};
use futures_util::StreamExt;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio_tungstenite::connect_async;

const RECONNECT_DELAY: std::time::Duration = std::time::Duration::from_secs(5);

pub async fn start_network_task(state: Arc<AppState>) {
    let addr = format!("127.0.0.1:{}", crate::setup::DEFAULT_PORT);
    start_network_task_with_addr(state, &addr).await;
}

pub async fn start_network_task_with_addr(state: Arc<AppState>, addr: &str) {
    let url = format!("ws://{addr}");
    log::info!("Connecting to {url}...");

    loop {
        match connect_async(&url).await {
            Ok((mut ws_stream, _)) => {
                log::info!("Connected to Rocket League via WebSocket!");
                mark_connected(&state, StatsApiTransport::WebSocket);

                while let Some(message) = ws_stream.next().await {
                    match message {
                        Ok(message) => {
                            if let Ok(payload) = message.to_text() {
                                process_payload(&state, StatsApiTransport::WebSocket, payload);
                            }
                        }
                        Err(error) => {
                            log::error!("WebSocket stream error: {error}");
                            update_parse_error(&state, error.to_string());
                            break;
                        }
                    }
                }

                mark_disconnected(&state);
            }
            Err(error) if is_raw_tcp_handshake(&error) => {
                log::info!("Detected raw TCP traffic. Switching to TCP mode...");
                run_tcp_connection(&state, addr).await;
            }
            Err(error) => {
                state.flags.is_connected.store(false, Ordering::SeqCst);
                log::error!("Connection failed: {error}. Retrying in 5s...");
                update_connection_error(&state, error.to_string());
            }
        }

        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

async fn run_tcp_connection(state: &Arc<AppState>, addr: &str) {
    let Ok(mut stream) = TcpStream::connect(addr).await else {
        state.flags.is_connected.store(false, Ordering::SeqCst);
        update_connection_error(state, "Could not connect via TCP.".to_string());
        return;
    };

    log::info!("Connected to Rocket League via TCP!");
    mark_connected(state, StatsApiTransport::Tcp);

    let mut buffer = [0u8; 16384];
    let mut splitter = TcpJsonSplitter::default();
    loop {
        match stream.read(&mut buffer).await {
            Ok(0) => break,
            Ok(bytes_read) => {
                for payload in splitter.push(&buffer[..bytes_read]) {
                    process_payload(state, StatsApiTransport::Tcp, &payload);
                }
            }
            Err(error) => {
                let message = format!("TCP read error: {error}");
                log::error!("{message}");
                update_connection_error(state, message);
                break;
            }
        }
    }

    mark_disconnected(state);
}

fn process_payload(state: &Arc<AppState>, transport: StatsApiTransport, payload: &str) {
    match serde_json::from_str::<Value>(payload) {
        Ok(json) => {
            record_recent_api_payload(state, transport, &json, payload);
            handle_owned_event(state, json);
        }
        Err(error) => {
            record_recent_api_payload(state, transport, &Value::Null, payload);
            update_parse_error(state, error.to_string());
        }
    }
}

fn record_recent_api_payload(
    state: &AppState,
    transport: StatsApiTransport,
    json: &Value,
    payload: &str,
) {
    let event = json
        .get("Event")
        .and_then(Value::as_str)
        .unwrap_or("Unparseable");
    if let Ok(mut recent_log) = state.diagnostics.recent_stats_api_log.lock() {
        recent_log.record(transport, event, payload, crate::stats_api::now_ms());
    }
}

fn mark_connected(state: &AppState, transport: StatsApiTransport) {
    state.flags.is_connected.store(true, Ordering::SeqCst);
    update_diagnostics(state, |diagnostics| {
        diagnostics.transport = transport;
        diagnostics.last_connection_error.clear();
    });
    state.request_ui_repaint();
}

fn mark_disconnected(state: &AppState) {
    state.flags.is_connected.store(false, Ordering::SeqCst);
    state.game.session.rcu(|current| {
        let mut session = (**current).clone();
        session.handle_transport_disconnect();
        Arc::new(session)
    });
    state.flags.is_watching_replay.store(
        state.game.session.load().is_watching_replay,
        Ordering::SeqCst,
    );
    clear_lobby_state(state);
    state.request_ui_repaint();
}

fn is_raw_tcp_handshake(error: &tokio_tungstenite::tungstenite::Error) -> bool {
    matches!(
        error,
        tokio_tungstenite::tungstenite::Error::Protocol(
            tokio_tungstenite::tungstenite::error::ProtocolError::HttparseError(_)
        )
    )
}

fn update_parse_error(state: &AppState, error: String) {
    update_diagnostics(state, |diagnostics| {
        diagnostics.last_parse_error = error.clone();
    });
    state.request_ui_repaint();
}

fn update_connection_error(state: &AppState, error: String) {
    update_diagnostics(state, |diagnostics| {
        diagnostics.last_connection_error = error.clone();
    });
    state.request_ui_repaint();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn frame(state: &Arc<AppState>, guid: &str, game: Value) {
        handle_event(
            state,
            &json!({"Event": "UpdateState", "Data": {
                "MatchGuid": guid, "Game": game,
                "Players": [{"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0,
                    "Score": 100, "IsLocalPlayer": true}]
            }}),
        );
    }

    #[tokio::test]
    async fn reconnect_from_confirmed_replay_to_live_match_restores_players_and_results() {
        let state = AppState::new();
        handle_event(&state, &json!({"Event": "ReplayPlaybackStart"}));
        frame(&state, "old-replay", json!({"bReplay": false}));
        assert!(state.game.session.load().is_watching_replay);
        mark_disconnected(&state);
        mark_connected(&state, StatsApiTransport::Tcp);
        frame(&state, "new-live", json!({"bReplay": false}));
        assert!(!state.flags.is_watching_replay.load(Ordering::SeqCst));
        assert_eq!(state.game.players.load().len(), 1);
        frame(
            &state,
            "new-live",
            json!({"bReplay": false, "bHasWinner": true,
            "Winner": "Blue", "Teams": [{"TeamNum": 0, "Score": 3}, {"TeamNum": 1, "Score": 2}]}),
        );
        assert_eq!(state.game.session.load().wins, 1);
    }

    #[test]
    fn reconnect_to_same_confirmed_replay_ignores_misleading_live_flag() {
        let state = AppState::new();
        handle_event(&state, &json!({"Event": "ReplayCreated"}));
        frame(&state, "same-replay", json!({"bReplay": false}));
        mark_disconnected(&state);
        mark_connected(&state, StatsApiTransport::WebSocket);
        frame(
            &state,
            "same-replay",
            json!({"bReplay": false, "bHasWinner": true, "Winner": "Blue"}),
        );
        assert!(state.flags.is_watching_replay.load(Ordering::SeqCst));
        assert!(state.game.players.load().is_empty());
        assert_eq!(state.game.session.load().matches_played, 0);
    }

    #[tokio::test]
    async fn disconnect_discards_playback_start_without_a_match_frame() {
        let state = AppState::new();
        handle_event(&state, &json!({"Event": "ReplayPlaybackStart"}));
        mark_disconnected(&state);
        frame(
            &state,
            "live-after-unbound-replay",
            json!({"bReplay": false}),
        );
        assert!(!state.flags.is_watching_replay.load(Ordering::SeqCst));
        assert_eq!(state.game.players.load().len(), 1);
    }

    #[test]
    fn replay_frame_markers_confirm_playback_after_a_changed_guid() {
        let state = AppState::new();
        frame(&state, "replay-one", json!({"Frame": 1, "bReplay": true}));
        mark_disconnected(&state);
        frame(
            &state,
            "replay-two",
            json!({"Frame": 2, "bReplay": false,
            "bHasWinner": true, "Winner": "Blue"}),
        );
        assert!(state.flags.is_watching_replay.load(Ordering::SeqCst));
        assert!(state.game.players.load().is_empty());
        assert_eq!(state.game.session.load().matches_played, 0);
    }
}
