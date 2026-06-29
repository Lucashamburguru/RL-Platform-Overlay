use rl_platform_overlay::network::start_network_task_with_addr;
use rl_platform_overlay::state::AppState;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

async fn send_payload(socket: &mut tokio::net::TcpStream, payload: &str) {
    socket.write_all(payload.as_bytes()).await.unwrap();
    socket.write_all(b"\n").await.unwrap();
    socket.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
}

#[tokio::test]
async fn test_stats_api_integration() {
    // 1. Bind TCP listener on a dynamic port
    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("Skipping stats API integration test: loopback TCP bind is not permitted");
            return;
        }
        Err(error) => panic!("Failed to bind mock Stats API server: {error}"),
    };
    let addr = listener.local_addr().unwrap();
    let addr_str = addr.to_string();

    // 2. Initialize AppState
    let state = AppState::new_with_debug(true);

    // 3. Spawn the mock stats API server
    tokio::spawn(async move {
        // First connection: Client will attempt a WebSocket handshake.
        // We want to force it to switch to raw TCP mode by returning raw non-HTTP bytes.
        if let Ok((mut socket, _)) = listener.accept().await {
            let _ = socket.write_all(b"invalid HTTP version\n").await;
            let _ = socket.shutdown().await;
        }

        // Second connection: Client will reconnect in TCP mode.
        if let Ok((mut socket, _)) = listener.accept().await {
            // Sequence Step 1: Start Match / UpdateState
            let update_state_match_start = r#"{"Event": "UpdateState", "Data": {"MatchGuid": "guid123", "Game": {"client": "TestLocalPlayer", "Arena": "Stadium_P"}, "players": [{"name": "TestLocalPlayer", "primaryId": "Steam|123456789|0", "isLocalPlayer": true, "team": 0, "boost": 82, "score": 0, "goals": 0, "saves": 0, "demos": 0}, {"name": "OpponentPlayer", "primaryId": "Steam|987654321|0", "isLocalPlayer": false, "team": 1, "boost": 45, "score": 0, "goals": 0, "saves": 0, "demos": 0}]}}"#;
            send_payload(&mut socket, update_state_match_start).await;

            // Sequence Step 2: RoundStarted
            send_payload(&mut socket, r#"{"Event": "RoundStarted"}"#).await;

            // Sequence Step 3: Update scores / stats
            let update_state_mid_match = r#"{"Event": "UpdateState", "Data": {"MatchGuid": "guid123", "Game": {"client": "TestLocalPlayer", "Arena": "Stadium_P", "Teams": [{"TeamNum": 0, "Score": 2}, {"TeamNum": 1, "Score": 1}]}, "players": [{"name": "TestLocalPlayer", "primaryId": "Steam|123456789|0", "isLocalPlayer": true, "team": 0, "boost": 100, "score": 150, "goals": 2, "saves": 0, "demos": 0}, {"name": "OpponentPlayer", "primaryId": "Steam|987654321|0", "isLocalPlayer": false, "team": 1, "boost": 45, "score": 80, "goals": 1, "saves": 1, "demos": 0}]}}"#;
            send_payload(&mut socket, update_state_mid_match).await;

            // Sequence Step 4: MatchEnded (Win)
            let match_ended_payload =
                r#"{"Event": "MatchEnded", "Data": {"MatchGuid": "guid123", "WinnerTeamNum": 0}}"#;
            send_payload(&mut socket, match_ended_payload).await;

            // Sequence Step 5: LobbyEntered
            send_payload(&mut socket, r#"{"Event": "LobbyEntered"}"#).await;

            // Keep socket open a bit longer
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    // 4. Start network task pointing to the mock server address
    let task_state = state.clone();
    let task_addr = addr_str.clone();
    tokio::spawn(async move {
        start_network_task_with_addr(task_state, &task_addr).await;
    });

    // 5. Assert the sequence states by polling with safety timeouts
    let start_time = std::time::Instant::now();
    let timeout = Duration::from_secs(5);
    let mut step = 1;

    while start_time.elapsed() < timeout {
        tokio::time::sleep(Duration::from_millis(50)).await;

        match step {
            1 => {
                // Verify match initialization
                let session = state.game.session.load();
                if session.active_match_id == "guid123"
                    && session.active_mode == rl_platform_overlay::session::SessionMode::Ones
                    && session.local_team == Some(0)
                {
                    step = 2;
                }
            }
            2 => {
                // Verify round started
                let session = state.game.session.load();
                if session.round_started {
                    step = 3;
                }
            }
            3 => {
                // Verify scores are updated mid match
                let session = state.game.session.load();
                if session.blue_score == 2 && session.orange_score == 1 {
                    step = 4;
                }
            }
            4 => {
                // Verify match ended records a Win
                let session = state.game.session.load();
                if session.wins == 1
                    && session.matches_played == 1
                    && session.last_result == rl_platform_overlay::session::MatchResult::Win
                {
                    step = 5;
                }
            }
            5 => {
                // Verify LobbyEntered cleanup
                let players = state.game.players.load();
                let local_name = state.game.local_player_name.load();
                if players.is_empty() && local_name.is_empty() {
                    break;
                }
            }
            _ => break,
        }
    }

    assert_eq!(
        step, 5,
        "Failed at sequence step {}: AppState did not transition or verify correctly.",
        step
    );
}
