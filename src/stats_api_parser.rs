use crate::json_utils::{bool_field, decode_json_string_value, number_field, string_field};
use crate::session::{MatchResult, SessionMode};
use crate::state::{LocalPlayerIdentity, PlayerInfo};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct StatsApiParseContext<'a> {
    pub current_local_name: &'a str,
    pub cached_identity: &'a LocalPlayerIdentity,
    pub previous_players: &'a HashMap<String, PlayerInfo>,
}

#[derive(Clone, Debug, Default)]
pub struct StatsApiEvent {
    pub event_name: String,
    pub data: Value,
    pub match_guid: Option<String>,
    pub game: Option<Value>,
    pub players: HashMap<String, PlayerInfo>,
    pub player_count: Option<usize>,
    pub local_player_hint: Option<LocalPlayerHint>,
    pub session_mode_hint: Option<String>,
    pub winner_hint: Option<String>,
    pub winner_team_num: Option<u8>,
    pub has_winner: bool,
    pub target_name: Option<String>,
    pub target_team: Option<u8>,
    pub has_target: bool,
    pub score: ScoreSignature,
    pub mode: ModeSignature,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalPlayerHint {
    pub name: String,
    pub team: u8,
    pub identity: LocalPlayerIdentity,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RosterSignature(pub Vec<RosterPlayerSignature>);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RosterPlayerSignature {
    pub key: String,
    pub team: u8,
    pub is_local: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScoreSignature {
    pub match_guid: String,
    pub blue_score: u32,
    pub orange_score: u32,
    pub has_winner: bool,
    pub winner: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultSignature {
    pub mode: SessionMode,
    pub local_team: u8,
    pub blue_score: u32,
    pub orange_score: u32,
    pub result: MatchResult,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModeSignature {
    pub match_guid: String,
    pub mode_hint: Option<String>,
    pub player_count: Option<usize>,
}

pub fn parse_stats_api_event(raw: &Value, context: StatsApiParseContext<'_>) -> StatsApiEvent {
    let event_name = string_field(raw, &["Event", "event"])
        .unwrap_or("Unknown")
        .to_string();
    let data = raw
        .get("Data")
        .or_else(|| raw.get("data"))
        .map(|data| (*decode_json_string_value(data)).clone())
        .unwrap_or_else(|| (*decode_json_string_value(raw)).clone());
    parse_stats_api_data(event_name, data, context)
}

pub fn parse_stats_api_data(
    event_name: String,
    data: Value,
    context: StatsApiParseContext<'_>,
) -> StatsApiEvent {
    let game = data.get("Game").or_else(|| data.get("game")).cloned();
    let hints = game.as_ref().map(extract_game_hints).unwrap_or_default();
    let effective_local_name = hints
        .local_name
        .as_deref()
        .or_else(|| {
            let name = context.current_local_name.trim();
            (!name.is_empty()).then_some(name)
        })
        .or(hints.target_name.as_deref())
        .unwrap_or("");
    let players = parse_players(
        &data,
        effective_local_name,
        hints.has_target,
        context.cached_identity,
        context.previous_players,
    );
    let local_player_hint = players
        .values()
        .find(|player| player.is_local)
        .map(|player| LocalPlayerHint {
            name: player.name.clone(),
            team: player.team,
            identity: LocalPlayerIdentity {
                name: player.name.clone(),
                primary_id: player.primary_id.clone(),
                platform: player.platform.clone(),
            },
        });
    let player_count = data
        .get("Players")
        .or_else(|| data.get("players"))
        .and_then(Value::as_array)
        .map(|players| players.len());
    let match_guid = string_field(&data, &["MatchGuid", "matchGuid"]).map(str::to_string);
    let session_mode_hint = game
        .as_ref()
        .and_then(session_mode_hint_from_game)
        .map(str::to_string);
    let winner_hint = game
        .as_ref()
        .and_then(|game| string_field(game, &["Winner", "winner"]))
        .map(str::to_string);
    let winner_team_num = number_field(
        &data,
        &["WinnerTeamNum", "winnerTeamNum", "WinnerTeam", "winnerTeam"],
    )
    .map(|team| team as u8);
    let has_winner = game
        .as_ref()
        .and_then(|game| bool_field(game, &["bHasWinner", "hasWinner"]))
        .unwrap_or(false);
    let (blue_score, orange_score) = game.as_ref().map(team_scores).unwrap_or((0, 0));
    let score = ScoreSignature {
        match_guid: match_guid.clone().unwrap_or_default(),
        blue_score,
        orange_score,
        has_winner,
        winner: winner_hint.clone().unwrap_or_default(),
    };
    let mode = ModeSignature {
        match_guid: match_guid.clone().unwrap_or_default(),
        mode_hint: session_mode_hint.clone(),
        player_count,
    };

    StatsApiEvent {
        event_name,
        data,
        match_guid,
        game,
        players,
        player_count,
        local_player_hint,
        session_mode_hint,
        winner_hint,
        winner_team_num,
        has_winner,
        target_name: hints.target_name,
        target_team: hints.target_team,
        has_target: hints.has_target,
        score,
        mode,
    }
}

impl RosterSignature {
    pub fn from_players(players: &HashMap<String, PlayerInfo>) -> Self {
        let mut signature: Vec<_> = players
            .values()
            .filter_map(|player| {
                crate::history::player_key(player).map(|key| RosterPlayerSignature {
                    key: key.as_str().to_string(),
                    team: player.team,
                    is_local: player.is_local,
                })
            })
            .collect();
        signature.sort();
        Self(signature)
    }
}

impl ScoreSignature {
    pub fn from_event(event: &StatsApiEvent) -> Self {
        event.score.clone()
    }
}

impl ModeSignature {
    pub fn session_mode(&self) -> SessionMode {
        if self.match_guid.is_empty() && self.player_count == Some(1) {
            return SessionMode::Freeplay;
        }

        SessionMode::infer(self.mode_hint.as_deref(), self.player_count)
    }
}

pub fn result_signature(
    mode: SessionMode,
    local_team: Option<u8>,
    blue_score: u32,
    orange_score: u32,
    winner: &str,
) -> Option<ResultSignature> {
    let local_team = local_team?;
    let result = result_from_winner(winner, local_team)
        .or_else(|| result_from_score(blue_score, orange_score, local_team))?;

    Some(ResultSignature {
        mode,
        local_team,
        blue_score,
        orange_score,
        result,
    })
}

pub fn result_from_winner(winner: &str, local_team: u8) -> Option<MatchResult> {
    let normalized = winner.trim().to_lowercase();
    if normalized.is_empty() {
        return None;
    }
    let winner_team = match normalized.as_str() {
        "blue" => Some(0),
        "orange" => Some(1),
        _ => None,
    }?;
    Some(if winner_team == local_team {
        MatchResult::Win
    } else {
        MatchResult::Loss
    })
}

pub fn result_from_score(blue: u32, orange: u32, local_team: u8) -> Option<MatchResult> {
    if blue == orange {
        return None;
    }
    let local_won = (local_team == 0 && blue > orange) || (local_team == 1 && orange > blue);
    Some(if local_won {
        MatchResult::Win
    } else {
        MatchResult::Loss
    })
}

pub fn team_scores(game: &Value) -> (u32, u32) {
    let mut blue_score = 0;
    let mut orange_score = 0;
    if let Some(teams) = game
        .get("Teams")
        .or_else(|| game.get("teams"))
        .and_then(Value::as_array)
    {
        for team in teams {
            let team_num = number_field(team, &["TeamNum", "teamNum", "Team", "team"]);
            let score = number_field(team, &["Score", "score"]);
            match (team_num, score) {
                (Some(0), Some(score)) => blue_score = score as u32,
                (Some(1), Some(score)) => orange_score = score as u32,
                _ => {}
            }
        }
    }
    (blue_score, orange_score)
}

#[derive(Default)]
struct GameHints {
    local_name: Option<String>,
    target_name: Option<String>,
    target_team: Option<u8>,
    has_target: bool,
}

fn extract_game_hints(game: &Value) -> GameHints {
    let mut hints = GameHints {
        has_target: bool_field(game, &["bHasTarget", "hasTarget"]).unwrap_or(false),
        ..Default::default()
    };
    if let Some(client) = string_field(game, &["client", "Client"]) {
        hints.local_name = Some(client.to_string());
    } else if let Some(me) = string_field(game, &["me", "Me"]) {
        hints.local_name = Some(me.to_string());
    }

    if let Some(target) = game.get("target").or_else(|| game.get("Target")) {
        hints.target_name = string_field(target, &["Name", "name"]).map(str::to_string);
        hints.target_team =
            number_field(target, &["TeamNum", "teamNum", "Team", "team"]).map(|team| team as u8);
    }

    hints
}

fn parse_players(
    data: &Value,
    current_local_name: &str,
    has_target: bool,
    cached_identity: &LocalPlayerIdentity,
    previous_players: &HashMap<String, PlayerInfo>,
) -> HashMap<String, PlayerInfo> {
    let Some(players) = data
        .get("Players")
        .or_else(|| data.get("players"))
        .and_then(Value::as_array)
    else {
        return HashMap::new();
    };

    let mut parsed = HashMap::new();
    for player_payload in players {
        let Some(player) = parse_player_info(
            player_payload,
            current_local_name,
            has_target,
            cached_identity,
            previous_players,
        ) else {
            continue;
        };
        parsed.insert(player.name.clone(), player);
    }
    parsed
}

fn parse_player_info(
    player_payload: &Value,
    current_local_name: &str,
    has_target: bool,
    cached_identity: &LocalPlayerIdentity,
    previous_players: &HashMap<String, PlayerInfo>,
) -> Option<PlayerInfo> {
    let name = string_field(player_payload, &["Name", "name"])
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() {
        return None;
    }

    let primary_id =
        string_field(player_payload, &["PrimaryId", "primaryId", "primary_id"]).unwrap_or("");
    let (platform, is_bot) = parse_platform(primary_id);
    let player_identity = LocalPlayerIdentity {
        name: name.clone(),
        primary_id: primary_id.to_string(),
        platform: platform.clone(),
    };
    let mut is_local = bool_field(player_payload, &["IsLocalPlayer", "isLocalPlayer", "isMe"])
        .unwrap_or(false)
        || (!current_local_name.is_empty() && name.eq_ignore_ascii_case(current_local_name))
        || (cached_identity.is_known() && cached_identity.same_account(&player_identity));

    if has_target && cached_identity.is_known() && !name.eq_ignore_ascii_case(&cached_identity.name)
    {
        is_local = false;
    }

    let team =
        number_field(player_payload, &["TeamNum", "teamNum", "Team", "team"]).unwrap_or(0) as u8;

    Some(PlayerInfo {
        name: name.clone(),
        primary_id: primary_id.to_string(),
        platform,
        team,
        is_bot,
        is_local,
        boost: number_field(player_payload, &["Boost", "boost"]).unwrap_or(0) as u8,
        score: number_field(player_payload, &["Score", "score"]).unwrap_or(0) as u32,
        goals: number_field(player_payload, &["Goals", "goals"]).unwrap_or(0) as u32,
        saves: number_field(player_payload, &["Saves", "saves"]).unwrap_or(0) as u32,
        touches: number_field(player_payload, &["Touches", "touches"]).unwrap_or(0) as u32,
        car_touches: number_field(player_payload, &["CarTouches", "carTouches", "car_touches"])
            .unwrap_or(0) as u32,
        demos: number_field(player_payload, &["Demos", "demos"]).unwrap_or(0) as u32,
        mmr: previous_players
            .get(&name)
            .and_then(|prev| prev.mmr.clone()),
    })
}

pub fn session_mode_hint_from_game(game: &Value) -> Option<&str> {
    string_field(
        game,
        &[
            "Arena",
            "arena",
            "Map",
            "map",
            "MapName",
            "mapName",
            "GameMode",
            "gameMode",
            "GameInfo",
            "gameInfo",
            "Playlist",
            "playlist",
            "PlaylistName",
            "playlistName",
            "Mutator",
            "mutator",
            "MutatorName",
            "mutatorName",
            "Rules",
            "rules",
        ],
    )
}

pub fn parse_platform(id: &str) -> (String, bool) {
    if id.is_empty() {
        return ("Unknown".to_string(), false);
    }
    if id == "Unknown|0|0" {
        return ("BOT".to_string(), true);
    }
    let parts: Vec<&str> = id.split('|').collect();
    let platform = parts.first().copied().unwrap_or("Unknown");
    let platform_lower = platform.to_lowercase();
    match platform_lower.as_str() {
        "steam" => ("Steam".to_string(), false),
        "epic" => ("Epic".to_string(), false),
        "ps4" | "ps5" | "playstation" | "psn" => ("PSN".to_string(), false),
        "xbox" | "xboxone" | "xboxseries" | "xbl" => ("Xbox".to_string(), false),
        "switch" | "nintendo" => ("Switch".to_string(), false),
        "bot" => ("BOT".to_string(), true),
        _ => (platform.to_string(), false),
    }
}

pub fn format_platform(platform: &str) -> &str {
    let lower = platform.to_lowercase();
    if lower == "ps4" || lower == "ps5" || lower == "playstation" || lower == "psn" {
        "PSN"
    } else if lower == "xbox" || lower == "xboxone" || lower == "xboxseries" || lower == "xbl" {
        "Xbox"
    } else if lower == "steam" {
        "Steam"
    } else if lower == "epic" {
        "Epic"
    } else if lower == "switch" || lower == "nintendo" {
        "Switch"
    } else if lower == "bot" {
        "BOT"
    } else if lower == "unknown" {
        "Unknown"
    } else {
        platform
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn context<'a>(
        cached_identity: &'a LocalPlayerIdentity,
        previous_players: &'a HashMap<String, PlayerInfo>,
    ) -> StatsApiParseContext<'a> {
        StatsApiParseContext {
            current_local_name: "",
            cached_identity,
            previous_players,
        }
    }

    #[test]
    fn parses_data_object_and_json_string() {
        let identity = LocalPlayerIdentity::default();
        let previous = HashMap::new();
        let object = parse_stats_api_event(
            &json!({"Event": "UpdateState", "Data": {"MatchGuid": "abc"}}),
            context(&identity, &previous),
        );
        let encoded = parse_stats_api_event(
            &json!({"Event": "UpdateState", "Data": "{\"MatchGuid\":\"abc\"}"}),
            context(&identity, &previous),
        );

        assert_eq!(object.event_name, "UpdateState");
        assert_eq!(object.match_guid.as_deref(), Some("abc"));
        assert_eq!(encoded.match_guid.as_deref(), Some("abc"));
    }

    #[test]
    fn normalizes_common_update_state_fields() {
        let identity = LocalPlayerIdentity::default();
        let previous = HashMap::new();
        let event = parse_stats_api_event(
            &json!({
                "Event": "UpdateState",
                "Data": {
                    "MatchGuid": "guid123",
                    "Game": {
                        "client": "Me",
                        "Arena": "HoopsStadium_P",
                        "Teams": [{"TeamNum": 0, "Score": 2}, {"TeamNum": 1, "Score": 1}],
                        "bHasWinner": true,
                        "Winner": "Blue"
                    },
                    "players": [
                        {"name": "Me", "primaryId": "Steam|1|0", "team": 0, "boost": 82},
                        {"name": "Opponent", "primaryId": "Epic|2|0", "team": 1}
                    ]
                }
            }),
            context(&identity, &previous),
        );

        assert_eq!(event.match_guid.as_deref(), Some("guid123"));
        assert_eq!(event.session_mode_hint.as_deref(), Some("HoopsStadium_P"));
        assert_eq!(event.score.blue_score, 2);
        assert_eq!(event.score.orange_score, 1);
        assert_eq!(event.winner_hint.as_deref(), Some("Blue"));
        assert_eq!(event.local_player_hint.as_ref().unwrap().name, "Me");
        assert_eq!(event.players["Me"].boost, 82);
    }

    #[test]
    fn local_player_resolves_through_cached_identity_without_flag() {
        let identity = LocalPlayerIdentity {
            name: "CachedName".to_string(),
            primary_id: "Steam|1|0".to_string(),
            platform: "Steam".to_string(),
        };
        let previous = HashMap::new();
        let event = parse_stats_api_event(
            &json!({
                "Event": "UpdateState",
                "Data": {
                    "Players": [
                        {"Name": "Renamed", "PrimaryId": "Steam|1|0", "TeamNum": 1},
                        {"Name": "Opponent", "PrimaryId": "Epic|2|0", "TeamNum": 0}
                    ]
                }
            }),
            context(&identity, &previous),
        );

        assert!(event.players["Renamed"].is_local);
        assert_eq!(event.local_player_hint.as_ref().unwrap().team, 1);
    }

    #[test]
    fn freeplay_shape_maps_to_freeplay_mode() {
        let identity = LocalPlayerIdentity::default();
        let previous = HashMap::new();
        let event = parse_stats_api_event(
            &json!({
                "Event": "UpdateState",
                "Data": {
                    "MatchGuid": "",
                    "Players": [{"Name": "Me", "PrimaryId": "Steam|1|0", "TeamNum": 0}]
                }
            }),
            context(&identity, &previous),
        );

        assert_eq!(event.mode.session_mode(), SessionMode::Freeplay);
    }

    #[test]
    fn roster_signature_ignores_boost_and_stats() {
        let mut first = HashMap::new();
        first.insert(
            "Me".to_string(),
            PlayerInfo {
                name: "Me".to_string(),
                primary_id: "Steam|1|0".to_string(),
                platform: "Steam".to_string(),
                team: 0,
                is_local: true,
                boost: 12,
                score: 5,
                ..Default::default()
            },
        );
        let mut second = first.clone();
        second.get_mut("Me").unwrap().boost = 99;
        second.get_mut("Me").unwrap().score = 500;
        second.get_mut("Me").unwrap().touches = 40;

        assert_eq!(
            RosterSignature::from_players(&first),
            RosterSignature::from_players(&second)
        );
    }

    #[test]
    fn roster_signature_changes_on_identity_team_or_local_changes() {
        let mut first = HashMap::new();
        first.insert(
            "Me".to_string(),
            PlayerInfo {
                name: "Me".to_string(),
                primary_id: "Steam|1|0".to_string(),
                platform: "Steam".to_string(),
                team: 0,
                is_local: true,
                ..Default::default()
            },
        );
        let mut team_changed = first.clone();
        team_changed.get_mut("Me").unwrap().team = 1;
        let mut local_changed = first.clone();
        local_changed.get_mut("Me").unwrap().is_local = false;
        let mut identity_changed = HashMap::new();
        identity_changed.insert(
            "Other".to_string(),
            PlayerInfo {
                name: "Other".to_string(),
                primary_id: "Steam|2|0".to_string(),
                platform: "Steam".to_string(),
                team: 0,
                is_local: true,
                ..Default::default()
            },
        );

        let original = RosterSignature::from_players(&first);
        assert_ne!(original, RosterSignature::from_players(&team_changed));
        assert_ne!(original, RosterSignature::from_players(&local_changed));
        assert_ne!(original, RosterSignature::from_players(&identity_changed));
    }

    #[test]
    fn result_signature_matches_ready_up_duplicate_winner_frames() {
        let first = result_signature(SessionMode::Hoops, Some(0), 1, 0, "Blue");
        let ready_up = result_signature(SessionMode::Hoops, Some(0), 1, 0, "Blue");
        assert_eq!(first, ready_up);
    }
}
