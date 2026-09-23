use super::*;
use crate::automation::{SequenceStep, parse_sequence};
use crate::stats_api_parser::parse_platform;
use serde_json::json;
use std::sync::atomic::Ordering;

fn test_player_key(name: &str) -> crate::state::PlayerKey {
    crate::state::PlayerKey::for_match(
        &PlayerInfo {
            name: name.to_string(),
            ..Default::default()
        },
        "test-match",
        None,
        0,
    )
}

fn player_named<'a>(players: &'a PlayerMap, name: &str) -> &'a PlayerInfo {
    players
        .values()
        .find(|player| player.name == name)
        .unwrap_or_else(|| panic!("missing player {name}"))
}

fn handle_update_state_payload(state: &Arc<AppState>, data: &Value) {
    handle_event(state, &json!({ "Event": "UpdateState", "Data": data }));
}

// Parsing and configuration compatibility

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

// Player publication, dashboard snapshots, and replay touch integration

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
        test_player_key("Opponent"),
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
        test_player_key("Opponent"),
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
    let preserved = player_named(&players, "Opponent").mmr.as_ref().unwrap();
    assert_eq!(preserved.playlists[&13].rating, 1234);
    assert_eq!(player_named(&players, "Opponent").boost, 80);
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
    assert_eq!(player_named(&snapshot.players, "Me").score, 700);
    assert_eq!(player_named(&snapshot.players, "Opponent").score, 300);
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
    assert_eq!(player_named(&snapshot.players, "Opponent").score, 300);
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
    assert_eq!(player_named(&players, "Me").score, 300);
    assert_eq!(player_named(&players, "Me").touches, 4);
    assert_eq!(player_named(&players, "Me").car_touches, 2);
    assert_eq!(player_named(&players, "Mate").score, 100);
    assert_eq!(player_named(&players, "Mate").touches, 2);
    assert_eq!(player_named(&players, "Mate").car_touches, 1);
    drop(players);

    let snapshot = state.game.dashboard_match_snapshot.load();
    assert_eq!(player_named(&snapshot.players, "Me").score, 300);
    assert_eq!(player_named(&snapshot.players, "Me").touches, 4);
    assert_eq!(player_named(&snapshot.players, "Me").car_touches, 2);
    assert_eq!(snapshot.team_bumps, [0, 0]);
}

#[test]
fn in_game_replay_touch_increments_are_subtracted_after_replay_ends() {
    let state = AppState::new();

    handle_update_state_payload(
        &state,
        &json!({
            "MatchGuid": "match-one",
            "Game": {
                "Teams": [
                    {"TeamNum": 0, "Score": 1},
                    {"TeamNum": 1, "Score": 0}
                ],
                "bReplay": false
            },
            "Players": [
                {
                    "Name": "Me",
                    "PrimaryId": "Steam|1|0",
                    "TeamNum": 0,
                    "Touches": 4,
                    "CarTouches": 2,
                    "IsLocalPlayer": true
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
                "bReplay": true
            },
            "Players": [
                {
                    "Name": "Me",
                    "PrimaryId": "Steam|1|0",
                    "TeamNum": 0,
                    "Touches": 10,
                    "CarTouches": 8,
                    "IsLocalPlayer": true
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
                "bReplay": false
            },
            "Players": [
                {
                    "Name": "Me",
                    "PrimaryId": "Steam|1|0",
                    "TeamNum": 0,
                    "Touches": 12,
                    "CarTouches": 9,
                    "IsLocalPlayer": true
                }
            ]
        }),
    );

    let players = state.game.players.load();
    assert_eq!(player_named(&players, "Me").touches, 4);
    assert_eq!(player_named(&players, "Me").car_touches, 2);
    drop(players);

    handle_update_state_payload(
        &state,
        &json!({
            "MatchGuid": "match-one",
            "Game": {
                "Teams": [
                    {"TeamNum": 0, "Score": 1},
                    {"TeamNum": 1, "Score": 0}
                ],
                "bReplay": false
            },
            "Players": [
                {
                    "Name": "Me",
                    "PrimaryId": "Steam|1|0",
                    "TeamNum": 0,
                    "Touches": 13,
                    "CarTouches": 10,
                    "IsLocalPlayer": true
                }
            ]
        }),
    );

    let players = state.game.players.load();
    assert_eq!(player_named(&players, "Me").touches, 5);
    assert_eq!(player_named(&players, "Me").car_touches, 3);
}

// Local-player and team resolution

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
    assert!(player_named(&players, "Me").is_local);
    assert_eq!(player_named(&players, "Me").team, 1);
    assert_eq!(player_named(&players, "Mate").boost, 88);
    assert!(!player_named(&players, "Opponent").is_local);
}

#[test]
fn partial_local_player_frame_preserves_team_in_live_state() {
    let state = AppState::new();
    handle_update_state_payload(
        &state,
        &json!({
            "MatchGuid": "guid123",
            "Players": [
                {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 1, "IsLocalPlayer": true}
            ]
        }),
    );
    handle_update_state_payload(
        &state,
        &json!({
            "MatchGuid": "guid123",
            "Players": [
                {"Name": "Me", "PrimaryId": "Steam|1|0", "IsLocalPlayer": true}
            ]
        }),
    );

    assert_eq!(state.game.local_team.load(Ordering::SeqCst), 1);
    assert_eq!(player_named(&state.game.players.load(), "Me").team, 1);
}

#[test]
fn partial_new_match_frame_does_not_preserve_previous_local_team() {
    let state = AppState::new();
    handle_update_state_payload(
        &state,
        &json!({
            "MatchGuid": "first-guid",
            "Players": [
                {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 1, "IsLocalPlayer": true}
            ]
        }),
    );
    handle_update_state_payload(
        &state,
        &json!({
            "MatchGuid": "second-guid",
            "Players": [
                {"Name": "Me", "PrimaryId": "Steam|1|0", "IsLocalPlayer": true}
            ]
        }),
    );

    assert_eq!(
        state.game.local_team.load(Ordering::SeqCst),
        crate::state::NO_TEAM
    );
    assert_eq!(
        player_named(&state.game.players.load(), "Me").team,
        crate::state::NO_TEAM
    );
    assert_eq!(state.game.session.load().local_team, None);
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
    assert!(player_named(&players, "cyberPeng").is_local);
    assert_eq!(player_named(&players, "C-Block").team, 0);
    assert_eq!(player_named(&players, "C-Block").boost, 88);
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
    assert!(player_named(&players, "CachedName").is_local);
    assert_eq!(state.game.local_team.load(Ordering::SeqCst), 1);
    assert!(!player_named(&players, "Opponent").is_local);
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
    assert!(!player_named(&players, "SpectatedPlayer").is_local);
    // The local player identity should still be MyRealName
    assert_eq!(state.game.local_player_identity.load().name, "MyRealName");
    assert_eq!(**state.game.local_player_name.load(), "MyRealName");
}

// Lobby reset and early-leave handling

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

// Session mode inference and correction

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

    let session = state.game.session.load();
    assert_eq!(session.active_mode, crate::session::SessionMode::Twos);
    assert_eq!(
        session.active_mode_source,
        crate::session::SessionModeSource::PlayerCount
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
fn test_scored_match_keeps_mode_when_roster_temporarily_grows() {
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
                "Arena": "Stadium_P",
                "Teams": [
                    {"TeamNum": 0, "Score": 1},
                    {"TeamNum": 1, "Score": 0}
                ]
            }
        }),
    );

    let session = state.game.session.load();
    assert_eq!(session.active_mode, crate::session::SessionMode::Twos);
    assert_eq!(session.blue_score, 1);
    drop(session);

    handle_update_state_payload(
        &state,
        &json!({
            "MatchGuid": "guid123",
            "Players": [
                {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true},
                {"Name": "Mate", "PrimaryId": "Epic|2|0", "TeamNum": 0},
                {"Name": "NewBlue", "PrimaryId": "Steam|5|0", "TeamNum": 0},
                {"Name": "Opp1", "PrimaryId": "Xbox|3|0", "TeamNum": 1},
                {"Name": "Opp2", "PrimaryId": "Ps4|4|0", "TeamNum": 1},
                {"Name": "NewOrange", "PrimaryId": "Epic|6|0", "TeamNum": 1}
            ],
            "Game": {
                "Arena": "Stadium_P",
                "Teams": [
                    {"TeamNum": 0, "Score": 1},
                    {"TeamNum": 1, "Score": 0}
                ]
            }
        }),
    );

    let session = state.game.session.load();
    assert_eq!(session.active_mode, crate::session::SessionMode::Twos);
    assert_eq!(
        session.active_mode_source,
        crate::session::SessionModeSource::PreviousLocked
    );
}

#[test]
fn test_late_join_uses_active_players_for_session_mode_before_score() {
    let state = AppState::new();
    handle_update_state_payload(
        &state,
        &json!({
            "MatchGuid": "guid123",
            "Players": [
                {"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0, "IsLocalPlayer": true, "bHasCar": true},
                {"Name": "Mate", "PrimaryId": "Epic|2|0", "TeamNum": 0, "bHasCar": true},
                {"Name": "Opp1", "PrimaryId": "Xbox|3|0", "TeamNum": 1, "bHasCar": true},
                {"Name": "Opp2", "PrimaryId": "PS4|4|0", "TeamNum": 1, "bHasCar": true},
                {"Name": "ReplacedBot", "PrimaryId": "Unknown|0|0", "TeamNum": 1, "bHasCar": false}
            ],
            "Game": {
                "Arena": "Stadium_P",
                "Teams": [
                    {"TeamNum": 0, "Score": 0},
                    {"TeamNum": 1, "Score": 0}
                ]
            }
        }),
    );

    let session = state.game.session.load();
    assert_eq!(session.active_mode, crate::session::SessionMode::Twos);
    assert_eq!(
        session.active_mode_source,
        crate::session::SessionModeSource::ActivePlayerCount
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

    let players = state.game.players.load();
    assert_eq!(&**state.game.local_player_name.load(), "cyberPeng");
    assert_eq!(state.game.local_team.load(Ordering::SeqCst), 0);
    assert!(player_named(&players, "cyberPeng").is_local);
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
    assert!(player_named(&players, "WindowsPlayer").is_local);
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
        session.active_mode_source,
        crate::session::SessionModeSource::MapIdentifier
    );
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

// Match results, history, and duplicate suppression

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
    assert_eq!(player_named(&players, "Me").boost, 99);
    assert_eq!(player_named(&players, "Me").score, 300);
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
    state.replace_config(config);

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

// Replay isolation and detection

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
