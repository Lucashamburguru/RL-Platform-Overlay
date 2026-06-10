use rl_platform_overlay::network::start_network_task_with_addr;
use rl_platform_overlay::state::AppState;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

#[tokio::test]
async fn test_stats_api_integration() {
    unsafe {
        std::env::set_var("RL_OVERLAY_TEST", "1");
    }

    // 1. Bind TCP listener on a dynamic port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let addr_str = addr.to_string();

    // 2. Initialize AppState
    let state = AppState::new_with_debug(true);

    // 3. Spawn the mock stats API server
    tokio::spawn(async move {
        // First connection: Client will attempt a WebSocket handshake.
        // We want to force it to switch to raw TCP mode by returning raw non-HTTP bytes.
        if let Ok((mut socket, _)) = listener.accept().await {
            // Write some plain text. This triggers an invalid HTTP version error in the WebSocket client.
            let _ = socket.write_all(b"invalid HTTP version\n").await;
            let _ = socket.shutdown().await;
        }

        // Second connection: Client will reconnect in TCP mode.
        if let Ok((mut socket, _)) = listener.accept().await {
            // Send the JSON event payload, followed by a newline
            let json_payload = r#"{"Event": "UpdateState", "Data": {"game": {"client": "TestLocalPlayer"}, "players": [{"name": "TestLocalPlayer", "primaryId": "Steam|123456789|0", "isLocalPlayer": true, "team": 0, "boost": 82, "score": 150, "goals": 1, "saves": 0, "demos": 0}, {"name": "TeammatePlayer", "primaryId": "Steam|987654321|0", "isLocalPlayer": false, "team": 0, "boost": 45, "score": 80, "goals": 0, "saves": 1, "demos": 0}]}}"#;

            let _ = socket.write_all(json_payload.as_bytes()).await;
            let _ = socket.write_all(b"\n").await;
            let _ = socket.flush().await;

            // Keep the socket open for a bit so client processes the message
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });

    // 4. Start network task pointing to the mock server address
    let task_state = state.clone();
    let task_addr = addr_str.clone();
    tokio::spawn(async move {
        start_network_task_with_addr(task_state, &task_addr).await;
    });

    // 5. Poll and assert AppState updates within a timeout
    let start_time = std::time::Instant::now();
    let timeout = Duration::from_secs(5);
    let mut success = false;

    while start_time.elapsed() < timeout {
        tokio::time::sleep(Duration::from_millis(50)).await;

        let players = state.players.load();
        if players.len() == 2 {
            let local_player = players.get("TestLocalPlayer");
            let teammate_player = players.get("TeammatePlayer");

            if let (Some(lp), Some(tp)) = (local_player, teammate_player)
                && lp.boost == 82
                && lp.score == 150
                && tp.boost == 45
                && tp.score == 80
            {
                success = true;
                break;
            }
        }
    }

    assert!(
        success,
        "Failed to parse game state event and update AppState correctly."
    );
}
