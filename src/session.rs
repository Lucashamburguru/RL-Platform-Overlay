use crate::json_utils::{bool_field, decode_json_string_value, number_field, string_field};
use crate::stats_api_parser::{
    ResultSignature, result_from_score, result_from_winner, result_signature, team_scores,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum SessionOverlayDisplay {
    #[default]
    Compact,
    Expanded,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MatchResult {
    #[default]
    Unknown,
    Win,
    Loss,
}

impl MatchResult {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Win => "Win",
            Self::Loss => "Loss",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SessionMode {
    Ones,
    Twos,
    Threes,
    Hoops,
    Dropshot,
    Snowday,
    Knockout,
    Freeplay,
    #[default]
    Unknown,
}

impl SessionMode {
    pub fn infer(arena: Option<&str>, player_count: Option<usize>) -> Self {
        arena
            .and_then(Self::from_arena)
            .or_else(|| player_count.map(Self::from_player_count))
            .unwrap_or(Self::Unknown)
    }

    fn from_player_count(player_count: usize) -> Self {
        match player_count {
            0 | 1 => Self::Unknown,
            2 => Self::Ones,
            3..=4 => Self::Twos,
            _ => Self::Threes,
        }
    }

    fn from_arena(arena: &str) -> Option<Self> {
        let normalized = arena
            .trim()
            .to_ascii_lowercase()
            .replace(['-', ' ', '.'], "_");

        if normalized.contains("hoops")
            || normalized.contains("dunkhouse")
            || normalized.contains("basket")
        {
            return Some(Self::Hoops);
        }

        if normalized.contains("shattershot") || normalized.contains("core707") {
            return Some(Self::Dropshot);
        }

        if normalized.contains("hockey")
            || normalized.contains("snowy")
            || normalized.contains("snow_")
            || normalized.contains("winter")
            || normalized.contains("snowday")
        {
            return Some(Self::Snowday);
        }

        if normalized.starts_with("ko_")
            || normalized.contains("_ko_")
            || normalized.contains("knockout")
            || normalized.contains("calavera")
            || normalized.contains("quadron")
            || normalized.contains("carbon")
        {
            return Some(Self::Knockout);
        }

        None
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ones => "1v1",
            Self::Twos => "2v2",
            Self::Threes => "3v3",
            Self::Hoops => "Hoops",
            Self::Dropshot => "Dropshot",
            Self::Snowday => "Snow Day",
            Self::Knockout => "Knockout",
            Self::Freeplay => "Freeplay",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionModeRecord {
    pub wins: u32,
    pub losses: u32,
}

impl SessionModeRecord {
    pub fn matches_played(&self) -> u32 {
        self.wins + self.losses
    }
}

#[derive(Clone, Debug, Default)]
pub struct SessionState {
    pub wins: u32,
    pub losses: u32,
    pub matches_played: u32,
    pub streak: i32,
    pub best_win_streak: u32,
    pub worst_loss_streak: u32,
    pub last_result: MatchResult,
    pub active_match_id: String,
    pub active_mode: SessionMode,
    pub mode_records: BTreeMap<SessionMode, SessionModeRecord>,
    pub local_team: Option<u8>,
    pub blue_score: u32,
    pub orange_score: u32,
    pub round_started: bool,
    pub is_watching_replay: bool,
    result_recorded_for_match: bool,
    last_recorded_match_id: String,
    last_recorded_result: Option<ResultSignature>,
}

impl SessionState {
    pub fn would_change(
        &self,
        real_data: &Value,
        local_team_hint: Option<u8>,
        mode_hint: SessionMode,
    ) -> bool {
        if let Some(match_guid) = string_field(real_data, &["MatchGuid", "matchGuid"])
            && self.active_match_id != match_guid
        {
            return true;
        }

        if mode_hint != SessionMode::Unknown && self.active_mode != mode_hint {
            return true;
        }

        if local_team_hint.is_some() && self.local_team != local_team_hint {
            return true;
        }

        if let Some(game) = real_data.get("Game").or_else(|| real_data.get("game")) {
            if let Some(teams) = game
                .get("Teams")
                .or_else(|| game.get("teams"))
                .and_then(Value::as_array)
            {
                for team in teams {
                    let team_num = number_field(team, &["TeamNum", "teamNum", "Team", "team"]);
                    let score = number_field(team, &["Score", "score"]);
                    match (team_num, score) {
                        (Some(0), Some(score)) if self.blue_score != score as u32 => {
                            return true;
                        }
                        (Some(1), Some(score)) if self.orange_score != score as u32 => {
                            return true;
                        }
                        _ => {}
                    }
                }
            }

            let has_winner = bool_field(game, &["bHasWinner", "hasWinner"]).unwrap_or(false);
            if has_winner && !self.has_recorded_current_match() {
                return true;
            }
        }

        if !self.round_started
            && let Some(players) = real_data
                .get("Players")
                .or_else(|| real_data.get("players"))
                .and_then(Value::as_array)
        {
            for p in players {
                let score = number_field(p, &["Score", "score"]).unwrap_or(0);
                let touches = number_field(p, &["Touches", "touches"]).unwrap_or(0);
                let goals = number_field(p, &["Goals", "goals"]).unwrap_or(0);
                let saves = number_field(p, &["Saves", "saves"]).unwrap_or(0);
                let demos = number_field(p, &["Demos", "demos"]).unwrap_or(0);
                if score > 0 || touches > 0 || goals > 0 || saves > 0 || demos > 0 {
                    return true;
                }
            }
        }

        false
    }

    pub fn handle_update_state(
        &mut self,
        data: &Value,
        local_team_hint: Option<u8>,
        mode_hint: SessionMode,
    ) {
        let real_data = decode_json_string_value(data);
        let has_winner = real_data
            .get("Game")
            .or_else(|| real_data.get("game"))
            .and_then(|game| bool_field(game, &["bHasWinner", "hasWinner"]))
            .unwrap_or(false);

        if let Some(match_guid) = string_field(&real_data, &["MatchGuid", "matchGuid"])
            && self.active_match_id != match_guid
        {
            if has_winner
                && self.matches_last_recorded_result(&real_data, local_team_hint, mode_hint)
            {
                return;
            }

            self.active_match_id = match_guid.to_string();
            self.active_mode = SessionMode::Unknown;
            self.result_recorded_for_match = false;
            self.last_result = MatchResult::Unknown;

            let b_replay = real_data
                .get("Game")
                .or_else(|| real_data.get("game"))
                .and_then(|game| bool_field(game, &["bReplay", "b_replay", "replay"]))
                .unwrap_or(false);
            self.is_watching_replay = self.is_watching_replay || b_replay;
        }

        if mode_hint != SessionMode::Unknown
            && (!self.round_started || self.active_mode == SessionMode::Unknown)
        {
            self.active_mode = mode_hint;
        }

        if let Some(team) = local_team_hint {
            self.local_team = Some(team);
        }

        if let Some(game) = real_data.get("Game").or_else(|| real_data.get("game")) {
            if let Some(teams) = game
                .get("Teams")
                .or_else(|| game.get("teams"))
                .and_then(Value::as_array)
            {
                for team in teams {
                    let team_num = number_field(team, &["TeamNum", "teamNum", "Team", "team"]);
                    let score = number_field(team, &["Score", "score"]);
                    match (team_num, score) {
                        (Some(0), Some(score)) => self.blue_score = score as u32,
                        (Some(1), Some(score)) => self.orange_score = score as u32,
                        _ => {}
                    }
                }
            }

            if has_winner && !self.is_watching_replay {
                let winner = string_field(game, &["Winner", "winner"]).unwrap_or("");
                self.record_result(winner);
            }
        }

        if let Some(players) = real_data
            .get("Players")
            .or_else(|| real_data.get("players"))
            .and_then(Value::as_array)
        {
            for p in players {
                let score = number_field(p, &["Score", "score"]).unwrap_or(0);
                let touches = number_field(p, &["Touches", "touches"]).unwrap_or(0);
                let goals = number_field(p, &["Goals", "goals"]).unwrap_or(0);
                let saves = number_field(p, &["Saves", "saves"]).unwrap_or(0);
                let demos = number_field(p, &["Demos", "demos"]).unwrap_or(0);
                if score > 0 || touches > 0 || goals > 0 || saves > 0 || demos > 0 {
                    self.round_started = true;
                }
            }
        }
    }

    pub fn handle_round_started(&mut self) {
        self.round_started = true;
    }

    pub fn record_early_leave(&mut self) {
        if self.is_watching_replay
            || self.has_recorded_current_match()
            || self.active_match_id.is_empty()
            || !self.round_started
        {
            return;
        }

        let Some(local_team) = self.local_team else {
            return;
        };

        let result = result_from_score(self.blue_score, self.orange_score, local_team)
            .unwrap_or(MatchResult::Loss);

        self.apply_match_result(result);
    }

    pub fn handle_match_ended(&mut self, data: &Value, local_team_hint: Option<u8>) {
        if self.is_watching_replay {
            return;
        }
        let real_data = decode_json_string_value(data);

        if let Some(match_guid) = string_field(&real_data, &["MatchGuid", "matchGuid"]) {
            if self.active_match_id.is_empty() {
                self.active_match_id = match_guid.to_string();
            } else if self.active_match_id != match_guid {
                return;
            }
        }

        if self.has_recorded_current_match() {
            return;
        }

        if let Some(team) = local_team_hint {
            self.local_team = Some(team);
        }

        let Some(local_team) = self.local_team else {
            self.last_result = MatchResult::Unknown;
            return;
        };

        let result = number_field(
            &real_data,
            &["WinnerTeamNum", "winnerTeamNum", "WinnerTeam", "winnerTeam"],
        )
        .map(|winner_team| {
            if winner_team as u8 == local_team {
                MatchResult::Win
            } else {
                MatchResult::Loss
            }
        })
        .unwrap_or(MatchResult::Unknown);

        self.apply_match_result(result);
    }

    pub fn handle_reset_event(&mut self) {
        self.active_match_id.clear();
        self.active_mode = SessionMode::Unknown;
        self.local_team = None;
        self.blue_score = 0;
        self.orange_score = 0;
        self.result_recorded_for_match = false;
        self.round_started = false;
    }

    fn record_result(&mut self, winner: &str) {
        if self.has_recorded_current_match() {
            return;
        }

        let Some(local_team) = self.local_team else {
            self.last_result = MatchResult::Unknown;
            return;
        };

        let result = result_from_winner(winner, local_team)
            .or_else(|| result_from_score(self.blue_score, self.orange_score, local_team))
            .unwrap_or(MatchResult::Unknown);

        self.apply_match_result(result);
    }

    fn apply_match_result(&mut self, result: MatchResult) {
        self.last_result = result;
        if result == MatchResult::Unknown {
            return;
        }

        self.result_recorded_for_match = true;
        if !self.active_match_id.is_empty() {
            self.last_recorded_match_id = self.active_match_id.clone();
        }
        self.last_recorded_result = Some(ResultSignature {
            mode: self.active_mode,
            local_team: self.local_team.unwrap_or(255),
            blue_score: self.blue_score,
            orange_score: self.orange_score,
            result,
        });
        self.matches_played += 1;
        match result {
            MatchResult::Win => {
                self.wins += 1;
                self.streak = if self.streak >= 0 { self.streak + 1 } else { 1 };
                self.best_win_streak = self.best_win_streak.max(self.streak as u32);
            }
            MatchResult::Loss => {
                self.losses += 1;
                self.streak = if self.streak <= 0 {
                    self.streak - 1
                } else {
                    -1
                };
                self.worst_loss_streak = self.worst_loss_streak.max(self.streak.unsigned_abs());
            }
            MatchResult::Unknown => {}
        }

        if self.active_mode != SessionMode::Unknown {
            let record = self.mode_records.entry(self.active_mode).or_default();
            match result {
                MatchResult::Win => record.wins += 1,
                MatchResult::Loss => record.losses += 1,
                MatchResult::Unknown => {}
            }
        }
    }

    fn has_recorded_current_match(&self) -> bool {
        self.result_recorded_for_match
            || (!self.active_match_id.is_empty()
                && self.last_recorded_match_id == self.active_match_id)
    }

    pub fn matches_last_recorded_result(
        &self,
        real_data: &Value,
        local_team_hint: Option<u8>,
        mode_hint: SessionMode,
    ) -> bool {
        let Some(last_recorded) = &self.last_recorded_result else {
            return false;
        };

        self.result_fingerprint(real_data, local_team_hint, mode_hint)
            .as_ref()
            == Some(last_recorded)
    }

    pub fn result_fingerprint(
        &self,
        real_data: &Value,
        local_team_hint: Option<u8>,
        mode_hint: SessionMode,
    ) -> Option<ResultSignature> {
        let game = real_data.get("Game").or_else(|| real_data.get("game"))?;
        let local_team = local_team_hint.or(self.local_team)?;
        let mode = if mode_hint != SessionMode::Unknown {
            mode_hint
        } else {
            self.active_mode
        };

        let (payload_blue_score, payload_orange_score) = team_scores(game);
        let blue_score = if payload_blue_score == 0 && payload_orange_score == 0 {
            self.blue_score
        } else {
            payload_blue_score
        };
        let orange_score = if payload_blue_score == 0 && payload_orange_score == 0 {
            self.orange_score
        } else {
            payload_orange_score
        };

        let winner = string_field(game, &["Winner", "winner"]).unwrap_or("");
        result_signature(mode, Some(local_team), blue_score, orange_score, winner)
    }
}

pub fn format_win_rate(wins: u32, losses: u32) -> String {
    let total = wins + losses;
    if total == 0 {
        return "0%".to_string();
    }

    format!("{}%", (wins as u64 * 100) / total as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn records_win_once_per_match() {
        let mut session = SessionState::default();
        let data = json!({
            "MatchGuid": "abc",
            "Game": {
                "Teams": [
                    {"TeamNum": 0, "Score": 4},
                    {"TeamNum": 1, "Score": 2}
                ],
                "bHasWinner": true,
                "Winner": "Blue"
            }
        });
        session.handle_update_state(&data, Some(0), SessionMode::Twos);
        session.handle_update_state(&data, Some(0), SessionMode::Twos);
        assert_eq!(session.wins, 1);
        assert_eq!(session.matches_played, 1);
        assert_eq!(session.streak, 1);
        assert_eq!(session.best_win_streak, 1);
        assert_eq!(session.mode_records[&SessionMode::Twos].wins, 1);
    }

    #[test]
    fn ignores_stale_winner_update_after_reset() {
        let mut session = SessionState::default();
        let data = json!({
            "MatchGuid": "abc",
            "Game": {
                "Teams": [
                    {"TeamNum": 0, "Score": 4},
                    {"TeamNum": 1, "Score": 2}
                ],
                "bHasWinner": true,
                "Winner": "Blue"
            }
        });

        session.handle_update_state(&data, Some(0), SessionMode::Twos);
        session.handle_reset_event();
        session.handle_update_state(&data, Some(0), SessionMode::Twos);

        assert_eq!(session.wins, 1);
        assert_eq!(session.matches_played, 1);
        assert_eq!(session.mode_records[&SessionMode::Twos].wins, 1);
    }

    #[test]
    fn ignores_ready_up_winner_update_with_new_match_guid() {
        let mut session = SessionState::default();
        let first_result = json!({
            "MatchGuid": "old-guid",
            "Game": {
                "Teams": [
                    {"TeamNum": 0, "Score": 1},
                    {"TeamNum": 1, "Score": 0}
                ],
                "bHasWinner": true,
                "Winner": "Blue"
            }
        });
        let ready_up_result = json!({
            "MatchGuid": "new-guid",
            "Game": {
                "Teams": [
                    {"TeamNum": 0, "Score": 1},
                    {"TeamNum": 1, "Score": 0}
                ],
                "bHasWinner": true,
                "Winner": "Blue"
            }
        });

        session.handle_update_state(&first_result, Some(0), SessionMode::Hoops);
        session.handle_update_state(&ready_up_result, Some(0), SessionMode::Hoops);

        assert_eq!(session.wins, 1);
        assert_eq!(session.matches_played, 1);
        assert_eq!(session.active_match_id, "old-guid");
        assert_eq!(session.mode_records[&SessionMode::Hoops].wins, 1);
    }

    #[test]
    fn ignores_stale_match_ended_after_reset() {
        let mut session = SessionState {
            active_match_id: "abc".to_string(),
            active_mode: SessionMode::Hoops,
            local_team: Some(0),
            ..Default::default()
        };
        let data = json!({
            "MatchGuid": "abc",
            "WinnerTeamNum": 0
        });

        session.handle_match_ended(&data, None);
        session.handle_reset_event();
        session.handle_match_ended(&data, Some(0));

        assert_eq!(session.wins, 1);
        assert_eq!(session.matches_played, 1);
        assert_eq!(session.mode_records[&SessionMode::Hoops].wins, 1);
    }

    #[test]
    fn records_loss_from_score_when_winner_name_unknown() {
        let mut session = SessionState::default();
        let data = json!({
            "MatchGuid": "abc",
            "Game": {
                "Teams": [
                    {"TeamNum": 0, "Score": 1},
                    {"TeamNum": 1, "Score": 3}
                ],
                "bHasWinner": true,
                "Winner": ""
            }
        });
        session.handle_update_state(&data, Some(0), SessionMode::Ones);
        assert_eq!(session.losses, 1);
        assert_eq!(session.last_result, MatchResult::Loss);
        assert_eq!(session.worst_loss_streak, 1);
        assert_eq!(session.mode_records[&SessionMode::Ones].losses, 1);
    }

    #[test]
    fn unknown_result_does_not_increment_matches() {
        let mut session = SessionState::default();
        let data = json!({
            "MatchGuid": "abc",
            "Game": {
                "bHasWinner": true,
                "Winner": ""
            }
        });
        session.handle_update_state(&data, None, SessionMode::Unknown);
        assert_eq!(session.matches_played, 0);
        assert_eq!(session.last_result, MatchResult::Unknown);
        assert!(!session.result_recorded_for_match);
    }

    #[test]
    fn unknown_result_does_not_block_later_known_result() {
        let mut session = SessionState::default();
        let unknown = json!({
            "MatchGuid": "abc",
            "Game": {
                "bHasWinner": true,
                "Winner": ""
            }
        });
        let known = json!({
            "MatchGuid": "abc",
            "Game": {
                "Teams": [
                    {"TeamNum": 0, "Score": 1},
                    {"TeamNum": 1, "Score": 2}
                ],
                "bHasWinner": true,
                "Winner": "Orange"
            }
        });

        session.handle_update_state(&unknown, Some(0), SessionMode::Hoops);
        session.handle_update_state(&known, Some(0), SessionMode::Hoops);

        assert_eq!(session.losses, 1);
        assert_eq!(session.matches_played, 1);
        assert_eq!(session.last_result, MatchResult::Loss);
        assert_eq!(session.mode_records[&SessionMode::Hoops].losses, 1);
    }

    #[test]
    fn records_match_ended_winner_team_num() {
        let mut session = SessionState {
            active_match_id: "abc".to_string(),
            active_mode: SessionMode::Hoops,
            local_team: Some(0),
            ..Default::default()
        };

        session.handle_match_ended(
            &json!({
                "MatchGuid": "abc",
                "WinnerTeamNum": 1
            }),
            None,
        );

        assert_eq!(session.losses, 1);
        assert_eq!(session.matches_played, 1);
        assert_eq!(session.last_result, MatchResult::Loss);
        assert_eq!(session.mode_records[&SessionMode::Hoops].losses, 1);
    }

    #[test]
    fn test_early_leave_win() {
        let mut session = SessionState {
            active_match_id: "xyz".to_string(),
            active_mode: SessionMode::Threes,
            local_team: Some(0),
            blue_score: 3,
            orange_score: 1,
            round_started: true,
            ..Default::default()
        };
        session.record_early_leave();
        assert_eq!(session.wins, 1);
        assert_eq!(session.losses, 0);
        assert_eq!(session.matches_played, 1);
        assert_eq!(session.last_result, MatchResult::Win);
        assert_eq!(session.best_win_streak, 1);
        assert_eq!(session.mode_records[&SessionMode::Threes].wins, 1);
        assert!(session.result_recorded_for_match);
    }

    #[test]
    fn test_early_leave_loss() {
        let mut session = SessionState {
            active_match_id: "xyz".to_string(),
            active_mode: SessionMode::Twos,
            local_team: Some(0),
            blue_score: 1,
            orange_score: 3,
            round_started: true,
            ..Default::default()
        };
        session.record_early_leave();
        assert_eq!(session.wins, 0);
        assert_eq!(session.losses, 1);
        assert_eq!(session.matches_played, 1);
        assert_eq!(session.last_result, MatchResult::Loss);
        assert_eq!(session.worst_loss_streak, 1);
        assert_eq!(session.mode_records[&SessionMode::Twos].losses, 1);
        assert!(session.result_recorded_for_match);
    }

    #[test]
    fn test_early_leave_tie_defaults_to_loss() {
        let mut session = SessionState {
            active_match_id: "xyz".to_string(),
            local_team: Some(0),
            blue_score: 2,
            orange_score: 2,
            round_started: true,
            ..Default::default()
        };
        session.record_early_leave();
        assert_eq!(session.wins, 0);
        assert_eq!(session.losses, 1);
        assert_eq!(session.matches_played, 1);
        assert_eq!(session.last_result, MatchResult::Loss);
        assert!(session.result_recorded_for_match);
    }

    #[test]
    fn test_early_leave_unknown_team_ignored() {
        let mut session = SessionState {
            active_match_id: "xyz".to_string(),
            local_team: None,
            blue_score: 1,
            orange_score: 3,
            ..Default::default()
        };
        session.record_early_leave();
        assert_eq!(session.wins, 0);
        assert_eq!(session.losses, 0);
        assert_eq!(session.matches_played, 0);
        assert_eq!(session.last_result, MatchResult::Unknown);
        assert!(!session.result_recorded_for_match);
    }

    #[test]
    fn unknown_mode_updates_overall_only() {
        let mut session = SessionState {
            active_match_id: "xyz".to_string(),
            active_mode: SessionMode::Unknown,
            local_team: Some(0),
            blue_score: 0,
            orange_score: 1,
            round_started: true,
            ..Default::default()
        };
        session.record_early_leave();
        assert_eq!(session.losses, 1);
        assert!(session.mode_records.is_empty());
    }

    #[test]
    fn tracks_best_and_worst_streaks_across_results() {
        let mut session = SessionState {
            active_match_id: "a".to_string(),
            active_mode: SessionMode::Ones,
            local_team: Some(0),
            blue_score: 1,
            orange_score: 0,
            round_started: true,
            ..Default::default()
        };
        session.record_early_leave();
        session.active_match_id = "b".to_string();
        session.result_recorded_for_match = false;
        session.record_early_leave();
        session.active_match_id = "c".to_string();
        session.result_recorded_for_match = false;
        session.blue_score = 0;
        session.orange_score = 1;
        session.record_early_leave();

        assert_eq!(session.best_win_streak, 2);
        assert_eq!(session.worst_loss_streak, 1);
    }

    #[test]
    fn formats_win_rate() {
        assert_eq!(format_win_rate(0, 0), "0%");
        assert_eq!(format_win_rate(4, 0), "100%");
        assert_eq!(format_win_rate(0, 3), "0%");
        assert_eq!(format_win_rate(3, 2), "60%");
    }

    #[test]
    fn infers_extra_modes_from_arena_name() {
        assert_eq!(
            SessionMode::infer(Some("HoopsStadium_P"), Some(4)),
            SessionMode::Hoops
        );
        assert_eq!(
            SessionMode::infer(Some("GameInfo_Basketball"), Some(1)),
            SessionMode::Hoops
        );
        assert_eq!(
            SessionMode::infer(Some("BasketStreet_P"), Some(4)),
            SessionMode::Hoops
        );
        assert_eq!(
            SessionMode::infer(Some("ShatterShot_P"), Some(6)),
            SessionMode::Dropshot
        );
        assert_eq!(
            SessionMode::infer(Some("KO_Calavera_P"), Some(8)),
            SessionMode::Knockout
        );
        assert_eq!(
            SessionMode::infer(Some("ThrowbackHockey_P"), Some(6)),
            SessionMode::Snowday
        );
        assert_eq!(
            SessionMode::infer(Some("Park_Snowy_P"), Some(6)),
            SessionMode::Snowday
        );
        assert_eq!(
            SessionMode::infer(Some("Stadium_Winter_P"), Some(6)),
            SessionMode::Snowday
        );
        assert_eq!(
            SessionMode::infer(Some("UtopiaStadium_Snow_P"), Some(6)),
            SessionMode::Snowday
        );
    }

    #[test]
    fn unrecognized_arena_falls_back_to_player_count() {
        assert_eq!(
            SessionMode::infer(Some("Stadium_P"), Some(4)),
            SessionMode::Twos
        );
        assert_eq!(
            SessionMode::infer(Some("Stadium_P"), Some(1)),
            SessionMode::Unknown
        );
        assert_eq!(
            SessionMode::infer(Some("Stadium_P"), None),
            SessionMode::Unknown
        );
    }

    #[test]
    fn labels_freeplay_mode() {
        assert_eq!(SessionMode::Freeplay.label(), "Freeplay");
        assert_eq!(SessionMode::Snowday.label(), "Snow Day");
    }

    #[test]
    fn locks_gamemode_after_round_started() {
        let mut session = SessionState {
            active_match_id: "abc".to_string(),
            active_mode: SessionMode::Threes,
            round_started: true,
            ..Default::default()
        };
        let data = json!({
            "MatchGuid": "abc",
            "Game": {
                "bHasWinner": false,
            }
        });
        session.handle_update_state(&data, None, SessionMode::Twos);
        assert_eq!(session.active_mode, SessionMode::Threes);
    }

    #[test]
    fn test_watching_replay_bypasses_all_updates() {
        let mut session = SessionState {
            active_match_id: "xyz".to_string(),
            active_mode: SessionMode::Twos,
            local_team: Some(0),
            blue_score: 3,
            orange_score: 1,
            round_started: true,
            is_watching_replay: true,
            ..Default::default()
        };

        // 1. Verify early leave does nothing
        session.record_early_leave();
        assert_eq!(session.wins, 0);
        assert_eq!(session.losses, 0);
        assert_eq!(session.matches_played, 0);

        // 2. Verify match ended does nothing
        session.handle_match_ended(
            &json!({
                "MatchGuid": "xyz",
                "WinnerTeamNum": 0
            }),
            None,
        );
        assert_eq!(session.wins, 0);
        assert_eq!(session.losses, 0);
        assert_eq!(session.matches_played, 0);

        // 3. Verify handle_update_state with winner does nothing
        let data = json!({
            "MatchGuid": "xyz",
            "Game": {
                "Teams": [
                    {"TeamNum": 0, "Score": 3},
                    {"TeamNum": 1, "Score": 1}
                ],
                "bHasWinner": true,
                "Winner": "Blue"
            }
        });
        session.handle_update_state(&data, Some(0), SessionMode::Twos);
        assert_eq!(session.wins, 0);
        assert_eq!(session.losses, 0);
        assert_eq!(session.matches_played, 0);
    }
}
