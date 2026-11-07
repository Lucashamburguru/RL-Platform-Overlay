use crate::state::AppState;
use eframe::egui;
use std::collections::HashSet;
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct EncounterNotice {
    match_id: String,
    started: Option<Instant>,
    shown: HashSet<String>,
    lines: Vec<String>,
    until: Option<Instant>,
}

impl EncounterNotice {
    fn update(
        &mut self,
        match_id: &str,
        eligible: bool,
        candidates: &[(String, String)],
        now: Instant,
    ) {
        if self.match_id != match_id {
            *self = Self {
                match_id: match_id.into(),
                started: Some(now),
                ..Default::default()
            };
        }
        if !eligible || match_id.is_empty() {
            self.lines.clear();
            self.until = None;
            return;
        }
        if self.until.is_some_and(|until| now < until) {
            return;
        }
        self.lines.clear();
        self.until = None;
        let elapsed = self
            .started
            .map(|at| now.saturating_duration_since(at))
            .unwrap_or_default();
        if !(Duration::from_millis(1500)..Duration::from_secs(30)).contains(&elapsed) {
            return;
        }
        for (id, line) in candidates {
            if self.shown.len() >= 64 {
                break;
            }
            if self.shown.insert(id.clone()) {
                self.lines.push(line.clone());
                if self.lines.len() == 3 {
                    break;
                }
            }
        }
        if !self.lines.is_empty() {
            self.until = Some(now + Duration::from_secs(6));
        }
    }

    pub(super) fn render(&mut self, ctx: &egui::Context, state: &AppState) {
        let config = state.system.config.load();
        let session = state.game.session.load();
        let eligible = config.history_enabled
            && config.encounter_notices_enabled
            && !session.is_watching_replay
            && !session.is_goal_replay;
        let mut candidates = Vec::new();
        if eligible {
            let players = state.game.players.load();
            let history = state.history.player_summaries.load();
            let identity = state.game.local_player_identity.load();
            for player in players.values().filter(|p| {
                !p.is_bot && !p.is_local && !p.primary_id.eq_ignore_ascii_case(&identity.primary_id)
            }) {
                let Some(key) = crate::history::player_key(player) else {
                    continue;
                };
                let Some(summary) = history.get(key.as_str()) else {
                    continue;
                };
                if summary.total_games() < config.encounter_notice_min_games.max(1) {
                    continue;
                }
                let name: String = player
                    .name
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(64)
                    .collect();
                let record = match (crate::state::standard_team(player.team), session.local_team) {
                    (Some(team), Some(local)) if team == local => format!(
                        " · {}W–{}L together",
                        summary.wins_with, summary.losses_with
                    ),
                    (Some(_), Some(_)) => format!(
                        " · {}W–{}L against",
                        summary.wins_against, summary.losses_against
                    ),
                    _ => String::new(),
                };
                candidates.push((
                    key.as_str().to_string(),
                    format!("{name} · {} past matches{record}", summary.total_games()),
                ));
            }
            candidates.sort_by(|a, b| a.0.cmp(&b.0));
        }
        self.update(
            &session.active_match_id,
            eligible,
            &candidates,
            Instant::now(),
        );
        if self.lines.is_empty() {
            return;
        }
        egui::Area::new("encounter_notice".into())
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0, -20.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgba_unmultiplied(20, 28, 38, 235))
                    .inner_margin(12.0)
                    .corner_radius(8.0)
                    .show(ui, |ui| {
                        ui.set_max_width(340.0);
                        ui.strong("Familiar players");
                        for line in &self.lines {
                            ui.label(line);
                        }
                    });
            });
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notices_wait_for_roster_settle_and_show_each_player_once_per_match() {
        let mut notice = EncounterNotice::default();
        let now = Instant::now();
        let players = vec![("id".into(), "Alex · 3 past matches".into())];
        notice.update("one", true, &players, now);
        assert!(notice.lines.is_empty());
        notice.update("one", true, &players, now + Duration::from_secs(2));
        assert_eq!(notice.lines.len(), 1);
        notice.update("one", true, &players, now + Duration::from_secs(10));
        assert!(notice.lines.is_empty());
        notice.update("two", true, &players, now + Duration::from_secs(11));
        notice.update("two", true, &players, now + Duration::from_secs(13));
        assert_eq!(notice.lines.len(), 1);
        notice.update("two", false, &players, now + Duration::from_secs(14));
        assert!(notice.lines.is_empty());
    }
}
