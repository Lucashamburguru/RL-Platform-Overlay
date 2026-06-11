use crate::session::{MatchResult, SessionMode, SessionState};
use crate::state::{AppState, NO_TEAM, PlayerInfo, config_dir};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlayerHistorySummary {
    pub player_key: String,
    pub name: String,
    pub platform: String,
    pub games_with: u32,
    pub games_against: u32,
    pub wins_with: u32,
    pub losses_with: u32,
    pub wins_against: u32,
    pub losses_against: u32,
    pub last_seen_unix_ms: i64,
}

impl PlayerHistorySummary {
    pub fn total_games(&self) -> u32 {
        self.games_with + self.games_against
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryTotals {
    pub matches: u32,
    pub players: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerKey(String);

impl PlayerKey {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub fn player_key(player: &PlayerInfo) -> Option<PlayerKey> {
    if player.is_bot {
        return None;
    }

    let name = player.name.trim();
    let primary_id = player.primary_id.trim();
    let platform = player.platform.trim();
    if name.is_empty()
        || primary_id.is_empty()
        || primary_id.eq_ignore_ascii_case("Unknown|0|0")
        || platform.is_empty()
        || platform.eq_ignore_ascii_case("Unknown")
        || platform.eq_ignore_ascii_case("BOT")
    {
        return None;
    }

    Some(PlayerKey(format!(
        "{}:{}",
        platform.to_ascii_lowercase(),
        primary_id.to_ascii_lowercase()
    )))
}

pub fn refresh_lobby_history(state: &Arc<AppState>) {
    let config = state.system.config.load();
    if !config.history_enabled || !config.lobby_history_indicators_enabled {
        state
            .history
            .player_summaries
            .store(Arc::new(HashMap::new()));
        return;
    }
    drop(config);

    let players = state.game.players.load();
    let keys: Vec<String> = players
        .values()
        .filter_map(player_key)
        .map(|key| key.0)
        .collect();

    match load_summaries(&keys) {
        Ok(summaries) => {
            state.history.player_summaries.store(Arc::new(summaries));
            set_status(state, "History ready.");
        }
        Err(error) => set_status(state, &format!("History error: {error}")),
    }
}

pub fn refresh_totals(state: &Arc<AppState>) {
    if !state.system.config.load().history_enabled {
        state
            .history
            .totals
            .store(Arc::new(HistoryTotals::default()));
        return;
    }

    match load_totals() {
        Ok(totals) => state.history.totals.store(Arc::new(totals)),
        Err(error) => set_status(state, &format!("History error: {error}")),
    }
}

pub fn record_completed_match(state: &Arc<AppState>, session: &SessionState) {
    if !state.system.config.load().history_enabled {
        return;
    }
    if session.last_result == MatchResult::Unknown
        || session.active_match_id.trim().is_empty()
        || session.local_team.is_none()
    {
        return;
    }

    let players = state.game.players.load();
    match insert_completed_match(session, players.values()) {
        Ok(inserted) => {
            if inserted {
                set_status(state, "Match saved to history.");
                refresh_totals(state);
                refresh_lobby_history(state);
            }
        }
        Err(error) => set_status(state, &format!("History save failed: {error}")),
    }
}

pub fn load_all_player_summaries() -> Result<Vec<PlayerHistorySummary>, String> {
    let conn = open_connection()?;
    init_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            "SELECT player_key, latest_name, platform, first_seen_unix_ms, last_seen_unix_ms
             FROM players
             ORDER BY last_seen_unix_ms DESC, latest_name COLLATE NOCASE",
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            let key: String = row.get(0)?;
            Ok((
                key.clone(),
                PlayerHistorySummary {
                    player_key: key,
                    name: row.get(1)?,
                    platform: row.get(2)?,
                    games_with: 0,
                    games_against: 0,
                    wins_with: 0,
                    losses_with: 0,
                    wins_against: 0,
                    losses_against: 0,
                    last_seen_unix_ms: row.get(4)?,
                },
            ))
        })
        .map_err(|error| error.to_string())?;

    let mut summaries = Vec::new();
    for row in rows {
        let (key, mut summary) = row.map_err(|error| error.to_string())?;
        if let Some(aggregate) = query_summary(&conn, &key)? {
            summary.games_with = aggregate.games_with;
            summary.games_against = aggregate.games_against;
            summary.wins_with = aggregate.wins_with;
            summary.losses_with = aggregate.losses_with;
            summary.wins_against = aggregate.wins_against;
            summary.losses_against = aggregate.losses_against;
            summary.last_seen_unix_ms = aggregate.last_seen_unix_ms;
        }
        summaries.push(summary);
    }

    Ok(summaries)
}

pub fn load_totals() -> Result<HistoryTotals, String> {
    let conn = open_connection()?;
    init_schema(&conn)?;
    let matches = conn
        .query_row("SELECT COUNT(*) FROM matches", [], |row| {
            row.get::<_, u32>(0)
        })
        .map_err(|error| error.to_string())?;
    let players = conn
        .query_row("SELECT COUNT(*) FROM players", [], |row| {
            row.get::<_, u32>(0)
        })
        .map_err(|error| error.to_string())?;
    Ok(HistoryTotals { matches, players })
}

pub fn clear_history() -> Result<(), String> {
    let mut conn = open_connection()?;
    init_schema(&conn)?;
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    tx.execute("DELETE FROM match_players", [])
        .map_err(|error| error.to_string())?;
    tx.execute("DELETE FROM matches", [])
        .map_err(|error| error.to_string())?;
    tx.execute("DELETE FROM players", [])
        .map_err(|error| error.to_string())?;
    tx.commit().map_err(|error| error.to_string())
}

fn load_summaries(keys: &[String]) -> Result<HashMap<String, PlayerHistorySummary>, String> {
    let conn = open_connection()?;
    init_schema(&conn)?;

    let mut summaries = HashMap::new();
    for key in keys {
        if let Some(summary) = query_summary(&conn, key)? {
            summaries.insert(key.clone(), summary);
        }
    }
    Ok(summaries)
}

fn insert_completed_match<'a>(
    session: &SessionState,
    players: impl Iterator<Item = &'a PlayerInfo>,
) -> Result<bool, String> {
    let mut conn = open_connection()?;
    init_schema(&conn)?;
    insert_completed_match_on_conn(&mut conn, session, players, current_unix_ms())
}

fn insert_completed_match_on_conn<'a>(
    conn: &mut Connection,
    session: &SessionState,
    players: impl Iterator<Item = &'a PlayerInfo>,
    ended_unix_ms: i64,
) -> Result<bool, String> {
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let local_team = session.local_team.unwrap_or(NO_TEAM);

    let inserted = tx
        .execute(
            "INSERT OR IGNORE INTO matches
             (match_guid, mode, result, blue_score, orange_score, local_team, ended_unix_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                session.active_match_id,
                mode_key(session.active_mode),
                result_key(session.last_result),
                session.blue_score,
                session.orange_score,
                local_team,
                ended_unix_ms,
            ],
        )
        .map_err(|error| error.to_string())?;

    if inserted == 0 {
        return Ok(false);
    }

    let match_id = tx.last_insert_rowid();
    for player in players {
        let Some(key) = player_key(player) else {
            continue;
        };
        tx.execute(
            "INSERT INTO players
             (player_key, latest_name, platform, primary_id, first_seen_unix_ms, last_seen_unix_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(player_key) DO UPDATE SET
                latest_name=excluded.latest_name,
                platform=excluded.platform,
                primary_id=excluded.primary_id,
                last_seen_unix_ms=excluded.last_seen_unix_ms",
            params![
                key.as_str(),
                player.name.trim(),
                player.platform.trim(),
                player.primary_id.trim(),
                ended_unix_ms,
            ],
        )
        .map_err(|error| error.to_string())?;

        let player_id = tx
            .query_row(
                "SELECT id FROM players WHERE player_key = ?1",
                params![key.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| error.to_string())?;
        tx.execute(
            "INSERT OR IGNORE INTO match_players
             (match_id, player_id, team, role, score, goals, saves, touches, demos)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                match_id,
                player_id,
                player.team,
                player_role(player, local_team),
                player.score,
                player.goals,
                player.saves,
                player.touches,
                player.demos,
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    tx.commit().map_err(|error| error.to_string())?;
    Ok(true)
}

fn query_summary(
    conn: &Connection,
    player_key: &str,
) -> Result<Option<PlayerHistorySummary>, String> {
    conn.query_row(
        "SELECT
            p.player_key,
            p.latest_name,
            p.platform,
            COALESCE(SUM(CASE WHEN mp.role = 'teammate' THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN mp.role = 'opponent' THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN mp.role = 'teammate' AND m.result = 'win' THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN mp.role = 'teammate' AND m.result = 'loss' THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN mp.role = 'opponent' AND m.result = 'win' THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN mp.role = 'opponent' AND m.result = 'loss' THEN 1 ELSE 0 END), 0),
            p.last_seen_unix_ms
         FROM players p
         LEFT JOIN match_players mp ON mp.player_id = p.id
         LEFT JOIN matches m ON m.id = mp.match_id
         WHERE p.player_key = ?1
         GROUP BY p.id",
        params![player_key],
        |row| {
            Ok(PlayerHistorySummary {
                player_key: row.get(0)?,
                name: row.get(1)?,
                platform: row.get(2)?,
                games_with: row.get(3)?,
                games_against: row.get(4)?,
                wins_with: row.get(5)?,
                losses_with: row.get(6)?,
                wins_against: row.get(7)?,
                losses_against: row.get(8)?,
                last_seen_unix_ms: row.get(9)?,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}

fn open_connection() -> Result<Connection, String> {
    let path =
        history_db_path().ok_or_else(|| "Could not resolve config directory.".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    Connection::open(path).map_err(|error| error.to_string())
}

fn history_db_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("history.sqlite3"))
}

fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;
        CREATE TABLE IF NOT EXISTS players (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            player_key TEXT NOT NULL UNIQUE,
            latest_name TEXT NOT NULL,
            platform TEXT NOT NULL,
            primary_id TEXT NOT NULL,
            first_seen_unix_ms INTEGER NOT NULL,
            last_seen_unix_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS matches (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            match_guid TEXT NOT NULL UNIQUE,
            mode TEXT NOT NULL,
            result TEXT NOT NULL,
            blue_score INTEGER NOT NULL,
            orange_score INTEGER NOT NULL,
            local_team INTEGER NOT NULL,
            ended_unix_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS match_players (
            match_id INTEGER NOT NULL,
            player_id INTEGER NOT NULL,
            team INTEGER NOT NULL,
            role TEXT NOT NULL,
            score INTEGER NOT NULL,
            goals INTEGER NOT NULL,
            saves INTEGER NOT NULL,
            touches INTEGER NOT NULL,
            demos INTEGER NOT NULL,
            PRIMARY KEY (match_id, player_id),
            FOREIGN KEY(match_id) REFERENCES matches(id) ON DELETE CASCADE,
            FOREIGN KEY(player_id) REFERENCES players(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_match_players_player_id ON match_players(player_id);
        CREATE INDEX IF NOT EXISTS idx_players_last_seen ON players(last_seen_unix_ms);
        ",
    )
    .map_err(|error| error.to_string())
}

fn player_role(player: &PlayerInfo, local_team: u8) -> &'static str {
    if player.is_local {
        "local"
    } else if player.team == local_team {
        "teammate"
    } else {
        "opponent"
    }
}

fn mode_key(mode: SessionMode) -> &'static str {
    match mode {
        SessionMode::Ones => "ones",
        SessionMode::Twos => "twos",
        SessionMode::Threes => "threes",
        SessionMode::Hoops => "hoops",
        SessionMode::Dropshot => "dropshot",
        SessionMode::Knockout => "knockout",
        SessionMode::Freeplay => "freeplay",
        SessionMode::Unknown => "unknown",
    }
}

fn result_key(result: MatchResult) -> &'static str {
    match result {
        MatchResult::Win => "win",
        MatchResult::Loss => "loss",
        MatchResult::Unknown => "unknown",
    }
}

fn current_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn set_status(state: &Arc<AppState>, message: &str) {
    if let Ok(mut status) = state.history.status.lock() {
        *status = message.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{MatchResult, SessionMode, SessionState};
    use crate::state::PlayerInfo;

    #[test]
    fn player_key_skips_bots_and_unknown_players() {
        let bot = PlayerInfo {
            name: "Bot".to_string(),
            primary_id: "BOT|1|0".to_string(),
            platform: "BOT".to_string(),
            is_bot: true,
            ..Default::default()
        };
        let unknown = PlayerInfo {
            name: "Unknown".to_string(),
            primary_id: "Unknown|0|0".to_string(),
            platform: "Unknown".to_string(),
            ..Default::default()
        };
        let human = PlayerInfo {
            name: "Player".to_string(),
            primary_id: "Steam|ABC|0".to_string(),
            platform: "Steam".to_string(),
            ..Default::default()
        };

        assert!(player_key(&bot).is_none());
        assert!(player_key(&unknown).is_none());
        assert_eq!(player_key(&human).unwrap().as_str(), "steam:steam|abc|0");
    }

    #[test]
    fn inserts_match_and_summarizes_relationships() {
        let mut conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        let mut session = SessionState::default();
        session.active_match_id = "match-a".to_string();
        session.active_mode = SessionMode::Hoops;
        session.local_team = Some(0);
        session.blue_score = 2;
        session.orange_score = 1;
        session.last_result = MatchResult::Win;
        let players = [
            player("Me", "Steam|me|0", 0, true),
            player("Friend", "Steam|friend|0", 0, false),
            player("Opponent", "Steam|opponent|0", 1, false),
        ];

        assert!(insert_completed_match_on_conn(&mut conn, &session, players.iter(), 1000).unwrap());
        let friend = query_summary(&conn, "steam:steam|friend|0")
            .unwrap()
            .unwrap();
        let opponent = query_summary(&conn, "steam:steam|opponent|0")
            .unwrap()
            .unwrap();

        assert_eq!(friend.games_with, 1);
        assert_eq!(friend.wins_with, 1);
        assert_eq!(opponent.games_against, 1);
        assert_eq!(opponent.wins_against, 1);
    }

    #[test]
    fn duplicate_match_guid_is_ignored() {
        let mut conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        let mut session = SessionState::default();
        session.active_match_id = "match-a".to_string();
        session.active_mode = SessionMode::Twos;
        session.local_team = Some(0);
        session.blue_score = 1;
        session.orange_score = 0;
        session.last_result = MatchResult::Win;
        let players = [player("Me", "Steam|me|0", 0, true)];

        assert!(insert_completed_match_on_conn(&mut conn, &session, players.iter(), 1000).unwrap());
        assert!(
            !insert_completed_match_on_conn(&mut conn, &session, players.iter(), 1001).unwrap()
        );
        let count = conn
            .query_row("SELECT COUNT(*) FROM matches", [], |row| {
                row.get::<_, u32>(0)
            })
            .unwrap();
        assert_eq!(count, 1);
    }

    fn player(name: &str, primary_id: &str, team: u8, is_local: bool) -> PlayerInfo {
        PlayerInfo {
            name: name.to_string(),
            primary_id: primary_id.to_string(),
            platform: "Steam".to_string(),
            team,
            is_local,
            score: 100,
            ..Default::default()
        }
    }
}
