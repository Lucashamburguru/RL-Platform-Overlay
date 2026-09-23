use crate::state::{
    AppState, PlayerMap, ReplayTouchOffset, ReplayTouchOffsetState, TeammateBumpTouch,
    TouchCounterDebounce,
};
use crate::stats_api_parser::StatsApiEvent;
use std::time::{Duration, Instant};

const BALL_TOUCH_DEBOUNCE_MS: u64 = 200;
const CAR_TOUCH_DEBOUNCE_MS: u64 = 450;

pub(super) fn debounce_touch_counters(state: &AppState, players: &mut PlayerMap, now: Instant) {
    let Ok(mut debounce) = state.game.touch_counter_debounce.lock() else {
        return;
    };
    debounce.retain(|key, _| players.contains_key(key));

    for (key, player) in players.iter_mut() {
        let entry = match debounce.entry(key.clone()) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let mut state = TouchCounterDebounce::new(player);
                if state.accepted_touches > 0 {
                    state.last_touch_increment_at = Some(now);
                }
                if state.accepted_car_touches > 0 {
                    state.last_car_touch_increment_at = Some(now);
                }
                entry.insert(state)
            }
        };

        player.touches = debounced_touch_value(
            player.touches,
            &mut entry.accepted_touches,
            &mut entry.last_touch_increment_at,
            Duration::from_millis(BALL_TOUCH_DEBOUNCE_MS),
            now,
        );
        player.car_touches = debounced_touch_value(
            player.car_touches,
            &mut entry.accepted_car_touches,
            &mut entry.last_car_touch_increment_at,
            Duration::from_millis(CAR_TOUCH_DEBOUNCE_MS),
            now,
        );
    }
}

pub(super) fn estimate_teammate_bumps(
    state: &AppState,
    previous_players: &PlayerMap,
    players: &PlayerMap,
    now: Instant,
) {
    let accepted_increments = accepted_car_touch_increments(previous_players, players);
    let Ok(mut estimator) = state.game.teammate_bump_estimator.lock() else {
        return;
    };
    let window = Duration::from_millis(CAR_TOUCH_DEBOUNCE_MS);
    estimator
        .pending
        .retain(|_, touch| now.duration_since(touch.at) <= window);

    for (key, team) in accepted_increments {
        if team <= 1 {
            estimator
                .pending
                .insert(key, TeammateBumpTouch { team, at: now });
        }
    }

    let pending: Vec<_> = estimator
        .pending
        .iter()
        .map(|(key, touch)| (key.clone(), *touch))
        .collect();
    if pending.len() != 2 {
        return;
    }

    let team = pending[0].1.team;
    if pending[1].1.team != team || team > 1 {
        return;
    }

    estimator.team_bumps[usize::from(team)] += 1;
    for (key, _) in pending {
        estimator.pending.remove(&key);
    }
}

pub(super) fn clear_teammate_bump_estimator(state: &AppState) {
    if let Ok(mut estimator) = state.game.teammate_bump_estimator.lock() {
        estimator.pending.clear();
        estimator.team_bumps = [0, 0];
    }
}

pub(super) fn clear_replay_touch_offsets(state: &AppState) {
    if let Ok(mut offsets) = state.game.replay_touch_offsets.lock() {
        *offsets = ReplayTouchOffsetState::default();
    }
}

pub(super) fn record_replay_touch_offsets(state: &AppState, event: &StatsApiEvent) {
    let Some(match_guid) = event
        .match_guid
        .as_deref()
        .map(str::trim)
        .filter(|guid| !guid.is_empty())
    else {
        clear_replay_touch_offsets(state);
        return;
    };

    let previous_players = state.game.players.load();
    let player_offsets = event
        .players
        .iter()
        .filter_map(|(key, player)| {
            let previous = previous_players.get(key)?;
            let touches = player.touches.saturating_sub(previous.touches);
            let car_touches = player.car_touches.saturating_sub(previous.car_touches);
            (touches > 0 || car_touches > 0).then_some((
                key.clone(),
                ReplayTouchOffset {
                    touches,
                    car_touches,
                },
            ))
        })
        .collect();
    drop(previous_players);

    if let Ok(mut offsets) = state.game.replay_touch_offsets.lock() {
        *offsets = ReplayTouchOffsetState {
            match_guid: match_guid.to_string(),
            player_offsets,
        };
    }
}

pub(super) fn apply_replay_touch_offsets(
    state: &AppState,
    match_guid: Option<&str>,
    players: &mut PlayerMap,
) {
    let Some(match_guid) = match_guid.map(str::trim).filter(|guid| !guid.is_empty()) else {
        return;
    };

    let Ok(mut offsets) = state.game.replay_touch_offsets.lock() else {
        return;
    };
    if offsets.match_guid.is_empty() {
        return;
    }
    if offsets.match_guid != match_guid {
        *offsets = ReplayTouchOffsetState::default();
        return;
    }

    offsets
        .player_offsets
        .retain(|key, _| players.contains_key(key));
    for (key, player) in players.iter_mut() {
        let Some(offset) = offsets.player_offsets.get(key) else {
            continue;
        };
        player.touches = player.touches.saturating_sub(offset.touches);
        player.car_touches = player.car_touches.saturating_sub(offset.car_touches);
    }
}

fn accepted_car_touch_increments(
    previous_players: &PlayerMap,
    players: &PlayerMap,
) -> Vec<(crate::state::PlayerKey, u8)> {
    players
        .iter()
        .filter_map(|(key, player)| {
            let previous = previous_players.get(key)?.car_touches;
            (player.car_touches > previous).then_some((key.clone(), player.team))
        })
        .collect()
}

fn debounced_touch_value(
    raw_value: u32,
    accepted_value: &mut u32,
    last_increment_at: &mut Option<Instant>,
    debounce_window: Duration,
    now: Instant,
) -> u32 {
    if raw_value < *accepted_value {
        *accepted_value = raw_value;
        *last_increment_at = None;
        return raw_value;
    }
    if raw_value == *accepted_value {
        return raw_value;
    }

    if last_increment_at.is_some_and(|last| now.duration_since(last) < debounce_window) {
        return *accepted_value;
    }

    *accepted_value = raw_value;
    *last_increment_at = Some(now);
    raw_value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, PlayerInfo, PlayerKey};
    use std::collections::HashMap;

    fn test_player_key(name: &str) -> PlayerKey {
        PlayerKey::for_match(
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

    fn player_named_mut<'a>(players: &'a mut PlayerMap, name: &str) -> &'a mut PlayerInfo {
        players
            .values_mut()
            .find(|player| player.name == name)
            .unwrap_or_else(|| panic!("missing player {name}"))
    }

    #[test]
    fn touch_counter_debounce_suppresses_rapid_duplicate_increments() {
        let state = AppState::new();
        let start = Instant::now();
        let mut players = HashMap::new();
        players.insert(
            test_player_key("Me"),
            PlayerInfo {
                name: "Me".to_string(),
                primary_id: "Steam|1|0".to_string(),
                platform: "Steam".to_string(),
                touches: 1,
                car_touches: 1,
                ..Default::default()
            },
        );
        debounce_touch_counters(&state, &mut players, start);
        assert_eq!(player_named(&players, "Me").touches, 1);
        assert_eq!(player_named(&players, "Me").car_touches, 1);

        player_named_mut(&mut players, "Me").touches = 2;
        player_named_mut(&mut players, "Me").car_touches = 2;
        debounce_touch_counters(&state, &mut players, start + Duration::from_millis(199));
        assert_eq!(player_named(&players, "Me").touches, 1);
        assert_eq!(player_named(&players, "Me").car_touches, 1);

        player_named_mut(&mut players, "Me").touches = 2;
        player_named_mut(&mut players, "Me").car_touches = 2;
        debounce_touch_counters(&state, &mut players, start + Duration::from_millis(200));
        assert_eq!(player_named(&players, "Me").touches, 2);
        assert_eq!(player_named(&players, "Me").car_touches, 1);

        player_named_mut(&mut players, "Me").car_touches = 2;
        debounce_touch_counters(&state, &mut players, start + Duration::from_millis(450));
        assert_eq!(player_named(&players, "Me").car_touches, 2);
    }

    #[test]
    fn touch_counter_debounce_accepts_counter_resets() {
        let state = AppState::new();
        let start = Instant::now();
        let mut players = HashMap::new();
        players.insert(
            test_player_key("Me"),
            PlayerInfo {
                name: "Me".to_string(),
                primary_id: "Steam|1|0".to_string(),
                platform: "Steam".to_string(),
                touches: 4,
                car_touches: 3,
                ..Default::default()
            },
        );
        debounce_touch_counters(&state, &mut players, start);

        player_named_mut(&mut players, "Me").touches = 0;
        player_named_mut(&mut players, "Me").car_touches = 0;
        debounce_touch_counters(&state, &mut players, start + Duration::from_millis(20));

        assert_eq!(player_named(&players, "Me").touches, 0);
        assert_eq!(player_named(&players, "Me").car_touches, 0);
    }

    fn touch_player(name: &str, team: u8, car_touches: u32) -> PlayerInfo {
        PlayerInfo {
            name: name.to_string(),
            team,
            car_touches,
            ..Default::default()
        }
    }

    #[test]
    fn teammate_bump_estimator_counts_same_team_pair_once() {
        let state = AppState::new();
        let start = Instant::now();
        let mut previous = HashMap::new();
        previous.insert(test_player_key("BlueOne"), touch_player("BlueOne", 0, 2));
        previous.insert(test_player_key("BlueTwo"), touch_player("BlueTwo", 0, 4));

        let mut players = previous.clone();
        player_named_mut(&mut players, "BlueOne").car_touches = 3;
        estimate_teammate_bumps(&state, &previous, &players, start);
        assert_eq!(
            state
                .game
                .teammate_bump_estimator
                .lock()
                .unwrap()
                .team_bumps,
            [0, 0]
        );

        previous = players.clone();
        player_named_mut(&mut players, "BlueTwo").car_touches = 5;
        estimate_teammate_bumps(
            &state,
            &previous,
            &players,
            start + Duration::from_millis(300),
        );

        assert_eq!(
            state
                .game
                .teammate_bump_estimator
                .lock()
                .unwrap()
                .team_bumps,
            [1, 0]
        );
    }

    #[test]
    fn teammate_bump_estimator_ignores_window_with_opponent_increment() {
        let state = AppState::new();
        let start = Instant::now();
        let mut previous = HashMap::new();
        previous.insert(test_player_key("BlueOne"), touch_player("BlueOne", 0, 1));
        previous.insert(test_player_key("BlueTwo"), touch_player("BlueTwo", 0, 1));
        previous.insert(
            test_player_key("OrangeOne"),
            touch_player("OrangeOne", 1, 1),
        );

        let mut players = previous.clone();
        player_named_mut(&mut players, "BlueOne").car_touches = 2;
        player_named_mut(&mut players, "BlueTwo").car_touches = 2;
        player_named_mut(&mut players, "OrangeOne").car_touches = 2;
        estimate_teammate_bumps(&state, &previous, &players, start);

        assert_eq!(
            state
                .game
                .teammate_bump_estimator
                .lock()
                .unwrap()
                .team_bumps,
            [0, 0]
        );
    }

    #[test]
    fn teammate_bump_estimator_ignores_suppressed_debounce_increment() {
        let state = AppState::new();
        let start = Instant::now();
        let mut previous = HashMap::new();
        previous.insert(test_player_key("BlueOne"), touch_player("BlueOne", 0, 1));
        previous.insert(test_player_key("BlueTwo"), touch_player("BlueTwo", 0, 1));

        let players = previous.clone();
        estimate_teammate_bumps(&state, &previous, &players, start);

        assert_eq!(
            state
                .game
                .teammate_bump_estimator
                .lock()
                .unwrap()
                .team_bumps,
            [0, 0]
        );
    }
}
