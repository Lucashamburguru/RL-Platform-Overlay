use serde::{Deserialize, Serialize};
use serde_json::Value;

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

#[derive(Clone, Debug, Default)]
pub struct SessionState {
    pub wins: u32,
    pub losses: u32,
    pub matches_played: u32,
    pub streak: i32,
    pub last_result: MatchResult,
    pub active_match_id: String,
    pub local_team: Option<u8>,
    pub blue_score: u32,
    pub orange_score: u32,
    result_recorded_for_match: bool,
}

impl SessionState {
    pub fn handle_update_state(&mut self, data: &Value, local_team_hint: Option<u8>) {
        let real_data = if let Some(s) = data.as_str() {
            serde_json::from_str::<Value>(s).unwrap_or(data.clone())
        } else {
            data.clone()
        };

        if let Some(match_guid) = string_field(&real_data, &["MatchGuid", "matchGuid"]) {
            if self.active_match_id != match_guid {
                self.active_match_id = match_guid.to_string();
                self.result_recorded_for_match = false;
                self.last_result = MatchResult::Unknown;
            }
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

            let has_winner = bool_field(game, &["bHasWinner", "hasWinner"]).unwrap_or(false);
            if has_winner {
                let winner = string_field(game, &["Winner", "winner"]).unwrap_or("");
                self.record_result(winner);
            }
        }
    }

    pub fn handle_reset_event(&mut self) {
        self.active_match_id.clear();
        self.local_team = None;
        self.blue_score = 0;
        self.orange_score = 0;
        self.result_recorded_for_match = false;
    }

    fn record_result(&mut self, winner: &str) {
        if self.result_recorded_for_match {
            return;
        }

        let Some(local_team) = self.local_team else {
            self.last_result = MatchResult::Unknown;
            return;
        };

        let result = result_from_winner(winner, local_team)
            .or_else(|| result_from_score(self.blue_score, self.orange_score, local_team))
            .unwrap_or(MatchResult::Unknown);

        self.last_result = result;
        self.result_recorded_for_match = true;
        if result == MatchResult::Unknown {
            return;
        }

        self.matches_played += 1;
        match result {
            MatchResult::Win => {
                self.wins += 1;
                self.streak = if self.streak >= 0 { self.streak + 1 } else { 1 };
            }
            MatchResult::Loss => {
                self.losses += 1;
                self.streak = if self.streak <= 0 {
                    self.streak - 1
                } else {
                    -1
                };
            }
            MatchResult::Unknown => {}
        }
    }
}

fn result_from_winner(winner: &str, local_team: u8) -> Option<MatchResult> {
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

fn result_from_score(blue: u32, orange: u32, local_team: u8) -> Option<MatchResult> {
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

fn string_field<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| value[*key].as_str())
}

fn number_field(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| {
        value[*key]
            .as_u64()
            .or_else(|| value[*key].as_str()?.parse().ok())
    })
}

fn bool_field(value: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter().find_map(|key| value[*key].as_bool())
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
        session.handle_update_state(&data, Some(0));
        session.handle_update_state(&data, Some(0));
        assert_eq!(session.wins, 1);
        assert_eq!(session.matches_played, 1);
        assert_eq!(session.streak, 1);
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
        session.handle_update_state(&data, Some(0));
        assert_eq!(session.losses, 1);
        assert_eq!(session.last_result, MatchResult::Loss);
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
        session.handle_update_state(&data, None);
        assert_eq!(session.matches_played, 0);
        assert_eq!(session.last_result, MatchResult::Unknown);
    }
}
