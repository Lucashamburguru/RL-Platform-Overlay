//! A bounded, typed feed of events actually supplied by the live Stats API.
use crate::json_utils::string_field;
use crate::session::SessionState;
use crate::stats_api_parser::StatsApiEvent;
use serde_json::Value;

const MAX_EVENTS: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub struct MatchEvent {
    pub kind: &'static str,
    pub player: String,
    pub other: Option<String>,
    pub goal_speed: Option<f64>,
    pub seconds_remaining: Option<u32>,
    pub overtime: bool,
    key: String,
}

impl MatchEvent {
    pub fn detail(&self) -> String {
        let mut detail = self.player.clone();
        if let Some(other) = &self.other {
            detail.push_str(&format!(" · {other}"));
        }
        if let Some(seconds) = self.seconds_remaining {
            detail.push_str(&format!(
                " · {}{}:{:02}",
                if self.overtime { "+" } else { "" },
                seconds / 60,
                seconds % 60
            ));
        }
        detail
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MatchEvents {
    pub match_id: String,
    pub events: std::collections::VecDeque<MatchEvent>,
}

fn player_name(data: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| data.get(*key))
        .and_then(|player| string_field(player, &["Name", "name"]))
        .map(|name| {
            name.chars()
                .filter(|c| !c.is_control())
                .take(128)
                .collect::<String>()
        })
        .filter(|name| !name.is_empty())
}

fn decimal_field(data: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .find_map(|key| data.get(*key))
        .and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        })
        .filter(|value| value.is_finite())
}

impl MatchEvents {
    pub fn observe(&mut self, event: &StatsApiEvent, session: &SessionState) -> bool {
        if event.is_replay || session.is_watching_replay || session.is_goal_replay {
            return false;
        }
        let id = event
            .match_guid
            .as_deref()
            .filter(|id| !id.is_empty())
            .unwrap_or(&session.active_match_id);
        if id.is_empty() {
            return false;
        }
        // Late events from the previous match must not resurrect its feed.
        if event.event_name != "UpdateState"
            && !session.active_match_id.is_empty()
            && session.active_match_id != id
        {
            return false;
        }
        let data = &event.data;
        let mut entry = match event.event_name.as_str() {
            "GoalScored" => {
                let Some(player) = player_name(data, &["Scorer", "scorer"]) else {
                    return false;
                };
                // Preserve the payload value. Do not label/convert units until checked against a live goal display.
                let goal_speed = decimal_field(data, &["GoalSpeed", "goalSpeed", "goal_speed"])
                    .filter(|speed| speed.is_finite() && *speed >= 0.0 && *speed <= 100_000.0);
                MatchEvent {
                    kind: "Goal",
                    player,
                    other: player_name(data, &["Assister", "assister"])
                        .map(|name| format!("assist: {name}")),
                    goal_speed,
                    seconds_remaining: session.time_seconds,
                    overtime: session.overtime,
                    key: String::new(),
                }
            }
            "StatfeedEvent" => {
                let kind = match string_field(data, &["StatName", "statName", "stat_name"])
                    .unwrap_or_default()
                {
                    "Demolish" | "Demolition" => "Demo",
                    "Save" | "EpicSave" => "Save",
                    _ => return false,
                };
                let Some(player) = player_name(data, &["MainTarget", "mainTarget", "main_target"])
                else {
                    return false;
                };
                MatchEvent {
                    kind,
                    player,
                    other: player_name(
                        data,
                        &["SecondaryTarget", "secondaryTarget", "secondary_target"],
                    ),
                    goal_speed: None,
                    seconds_remaining: session.time_seconds,
                    overtime: session.overtime,
                    key: String::new(),
                }
            }
            "UpdateState" if self.match_id != id => {
                self.match_id = id.into();
                self.events.clear();
                return true;
            }
            _ => return false,
        };
        if self.match_id != id {
            self.match_id = id.into();
            self.events.clear();
        }
        let timestamp = decimal_field(
            data,
            &[
                "GoalTime",
                "goalTime",
                "goal_time",
                "TimeSeconds",
                "timeSeconds",
            ],
        )
        .filter(|time| time.is_finite());
        // GoalTime is the previous round's duration, not a match-unique timestamp.
        // Include the match clock, but not the score (which can advance before a
        // duplicate event arrives).
        entry.key = format!(
            "{}|{}|{:?}|{:?}|{:?}|{:?}|{}",
            entry.kind,
            entry.player,
            entry.other,
            timestamp,
            entry.goal_speed,
            session.time_seconds,
            session.overtime
        );
        if self.events.iter().any(|old| old.key == entry.key) {
            return false;
        }
        self.events.push_back(entry);
        while self.events.len() > MAX_EVENTS {
            self.events.pop_front();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn goal_feed_deduplicates_after_score_changes_and_ignores_replay() {
        let mut session = SessionState::default();
        session.active_match_id = "match".into();
        let event = StatsApiEvent {
            event_name: "GoalScored".into(),
            match_guid: Some("match".into()),
            data: json!({"Scorer":{"Name":"Alex"}, "Assister":{"Name":"Sam"}, "GoalSpeed":2000.0,"GoalTime":32.5}),
            ..Default::default()
        };
        let mut feed = MatchEvents::default();
        assert!(feed.observe(&event, &session));
        assert_eq!(feed.events[0].goal_speed, Some(2000.0));
        session.blue_score = 1;
        assert!(!feed.observe(&event, &session));
        session.is_goal_replay = true;
        let mut second = event.clone();
        second.data["GoalTime"] = json!(60.0);
        assert!(!feed.observe(&second, &session));
        assert_eq!(feed.events.len(), 1);
    }

    #[test]
    fn new_match_clears_feed_and_late_goal_does_not_restore_old_match() {
        let mut feed = MatchEvents::default();
        let mut session = SessionState::default();
        session.active_match_id = "old".into();
        let goal = StatsApiEvent {
            event_name: "GoalScored".into(),
            match_guid: Some("old".into()),
            data: json!({"Scorer":{"Name":"Alex"}}),
            ..Default::default()
        };
        feed.observe(&goal, &session);
        let update = StatsApiEvent {
            event_name: "UpdateState".into(),
            match_guid: Some("new".into()),
            ..Default::default()
        };
        assert!(feed.observe(&update, &session));
        assert!(feed.events.is_empty());
        session.active_match_id = "new".into();
        assert!(!feed.observe(&goal, &session));
        assert_eq!(feed.match_id, "new");
    }
}
