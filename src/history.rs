use crate::session::{MatchResult, SessionMode, SessionState};
use crate::state::{AppState, NO_TEAM, PlayerInfo, config_dir};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(thiserror::Error, Debug)]
pub enum HistoryError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Config directory error: {0}")]
    ConfigDir(String),
    #[error("Mutex lock poisoned: {0}")]
    MutexPoisoned(String),
}

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

fn with_connection<F, T>(state: &AppState, f: F) -> Result<T, HistoryError>
where
    F: FnOnce(&Connection) -> Result<T, HistoryError>,
{
    let mut guard = state
        .history
        .conn
        .lock()
        .map_err(|e| HistoryError::MutexPoisoned(e.to_string()))?;
    if guard.is_none() {
        match initialize_database() {
            Ok(conn) => {
                *guard = Some(conn);
            }
            Err(e) => return Err(e),
        }
    }
    if let Some(conn) = guard.as_ref() {
        f(conn)
    } else {
        Err(HistoryError::ConfigDir(
            "Database connection is missing".to_string(),
        ))
    }
}

fn with_connection_mut<F, T>(state: &AppState, f: F) -> Result<T, HistoryError>
where
    F: FnOnce(&mut Connection) -> Result<T, HistoryError>,
{
    let mut guard = state
        .history
        .conn
        .lock()
        .map_err(|e| HistoryError::MutexPoisoned(e.to_string()))?;
    if guard.is_none() {
        match initialize_database() {
            Ok(conn) => {
                *guard = Some(conn);
            }
            Err(e) => return Err(e),
        }
    }
    if let Some(conn) = guard.as_mut() {
        f(conn)
    } else {
        Err(HistoryError::ConfigDir(
            "Database connection is missing".to_string(),
        ))
    }
}

pub fn initialize_database() -> Result<Connection, HistoryError> {
    let conn = open_connection()?;
    init_schema(&conn)?;
    migrate_database_platforms(&conn)?;
    cleanup_local_history_rows(&conn)?;
    Ok(conn)
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

    let state_clone = state.clone();
    let run = move || match load_summaries(&state_clone, &keys) {
        Ok(summaries) => {
            state_clone
                .history
                .player_summaries
                .store(Arc::new(summaries));
            set_status(&state_clone, "History ready.");
        }
        Err(error) => set_status(&state_clone, &format!("History error: {error}")),
    };
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn_blocking(run);
    } else {
        run();
    }
}

pub fn refresh_totals(state: &Arc<AppState>) {
    if !state.system.config.load().history_enabled {
        state
            .history
            .totals
            .store(Arc::new(HistoryTotals::default()));
        set_status(state, "History disabled.");
        return;
    }

    let state_clone = state.clone();
    let run = move || match load_totals(&state_clone) {
        Ok(totals) => {
            state_clone.history.totals.store(Arc::new(totals));
            set_status(&state_clone, "History ready.");
        }
        Err(error) => set_status(&state_clone, &format!("History error: {error}")),
    };
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn_blocking(run);
    } else {
        run();
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

    let match_roster = state.game.match_roster.load();
    let live_players = state.game.players.load();
    let players = if match_roster.is_empty() {
        live_players.values().cloned().collect::<Vec<_>>()
    } else {
        match_roster.values().cloned().collect::<Vec<_>>()
    };

    let state_clone = state.clone();
    let session_clone = session.clone();

    let run = move || match insert_completed_match(&state_clone, &session_clone, players.iter()) {
        Ok(inserted) => {
            if inserted {
                set_status(&state_clone, "Match saved to history.");
                state_clone.history.revision.fetch_add(1, Ordering::SeqCst);
                refresh_totals(&state_clone);
                refresh_lobby_history(&state_clone);
            }
        }
        Err(error) => set_status(&state_clone, &format!("History save failed: {error}")),
    };
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn_blocking(run);
    } else {
        run();
    }
}

pub fn load_all_player_summaries(
    state: &AppState,
) -> Result<Vec<PlayerHistorySummary>, HistoryError> {
    with_connection(state, load_all_player_summaries_on_conn)
}

fn load_all_player_summaries_on_conn(
    conn: &Connection,
) -> Result<Vec<PlayerHistorySummary>, HistoryError> {
    let mut stmt = conn
        .prepare(
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
             JOIN match_players mp ON mp.player_id = p.id
                AND mp.role IN ('teammate', 'opponent')
             LEFT JOIN matches m ON m.id = mp.match_id
             GROUP BY p.id
             ORDER BY p.last_seen_unix_ms DESC, p.latest_name COLLATE NOCASE",
        )?;

    let rows = stmt.query_map([], |row| {
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
    })?;

    let mut summaries = Vec::new();
    for row in rows {
        summaries.push(row?);
    }

    Ok(summaries)
}

pub fn load_totals(state: &AppState) -> Result<HistoryTotals, HistoryError> {
    with_connection(state, load_totals_on_conn)
}

fn load_totals_on_conn(conn: &Connection) -> Result<HistoryTotals, HistoryError> {
    let matches = conn.query_row("SELECT COUNT(*) FROM matches", [], |row| {
        row.get::<_, u32>(0)
    })?;
    let players = conn.query_row(
        "SELECT COUNT(DISTINCT p.id)
             FROM players p
             JOIN match_players mp ON mp.player_id = p.id
             WHERE mp.role IN ('teammate', 'opponent')",
        [],
        |row| row.get::<_, u32>(0),
    )?;
    Ok(HistoryTotals { matches, players })
}

pub fn clear_history(state: &AppState) -> Result<(), HistoryError> {
    with_connection_mut(state, |conn| {
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM match_players", [])?;
        tx.execute("DELETE FROM matches", [])?;
        tx.execute("DELETE FROM players", [])?;
        tx.commit()?;
        Ok(())
    })
}

fn load_summaries(
    state: &AppState,
    keys: &[String],
) -> Result<HashMap<String, PlayerHistorySummary>, HistoryError> {
    if keys.is_empty() {
        return Ok(HashMap::new());
    }

    with_connection(state, |conn| {
        let placeholders = vec!["?"; keys.len()].join(", ");
        let sql = format!(
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
             JOIN match_players mp ON mp.player_id = p.id
                AND mp.role IN ('teammate', 'opponent')
             LEFT JOIN matches m ON m.id = mp.match_id
             WHERE p.player_key IN ({placeholders})
             GROUP BY p.id"
        );

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(keys), |row| {
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
        })?;

        let mut summaries = HashMap::new();
        for row in rows {
            let summary = row?;
            summaries.insert(summary.player_key.clone(), summary);
        }
        Ok(summaries)
    })
}

fn insert_completed_match<'a>(
    state: &AppState,
    session: &SessionState,
    players: impl Iterator<Item = &'a PlayerInfo>,
) -> Result<bool, HistoryError> {
    with_connection_mut(state, |conn| {
        insert_completed_match_on_conn(conn, session, players, current_unix_ms())
    })
}

fn insert_completed_match_on_conn<'a>(
    conn: &mut Connection,
    session: &SessionState,
    players: impl Iterator<Item = &'a PlayerInfo>,
    ended_unix_ms: i64,
) -> Result<bool, HistoryError> {
    let tx = conn.transaction()?;
    let local_team = session.local_team.unwrap_or(NO_TEAM);

    let inserted = tx.execute(
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
    )?;

    if inserted == 0 {
        return Ok(false);
    }

    let match_id = tx.last_insert_rowid();
    for player in players {
        if player.is_local {
            continue;
        }
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
        )?;

        let player_id = tx.query_row(
            "SELECT id FROM players WHERE player_key = ?1",
            params![key.as_str()],
            |row| row.get::<_, i64>(0),
        )?;
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
        )?;
    }

    tx.commit()?;
    Ok(true)
}

fn open_connection() -> Result<Connection, HistoryError> {
    let path = history_db_path().ok_or_else(|| {
        HistoryError::ConfigDir("Could not resolve config directory.".to_string())
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    Ok(conn)
}

fn history_db_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("history.sqlite3"))
}

fn init_schema(conn: &Connection) -> Result<(), HistoryError> {
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
    )?;
    Ok(())
}

fn migrate_database_platforms(conn: &Connection) -> Result<(), HistoryError> {
    let mut stmt = conn.prepare("SELECT id, player_key, platform, primary_id FROM players")?;
    let mut rows = stmt.query([])?;
    let mut updates = Vec::new();
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let key: String = row.get(1)?;
        let platform: String = row.get(2)?;
        let primary_id: String = row.get(3)?;

        let norm_platform = crate::stats_api_parser::format_platform(&platform);
        let norm_key = format!(
            "{}:{}",
            norm_platform.to_lowercase(),
            primary_id.to_lowercase()
        );

        if norm_platform != platform || norm_key != key {
            updates.push((id, norm_key, norm_platform.to_string()));
        }
    }
    drop(rows);
    drop(stmt);

    if updates.is_empty() {
        return Ok(());
    }

    log::info!(
        "Migrating {} history database players to normalized platforms...",
        updates.len()
    );

    for (id, new_key, norm_platform) in updates {
        let existing_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM players WHERE player_key = ?1 AND id != ?2",
                params![&new_key, id],
                |r| r.get(0),
            )
            .optional()?;

        if let Some(target_id) = existing_id {
            // Collision! Merge 'id' into 'target_id'.
            conn.execute(
                "DELETE FROM match_players 
                 WHERE player_id = ?1 
                   AND match_id IN (SELECT match_id FROM match_players WHERE player_id = ?2)",
                params![id, target_id],
            )?;

            conn.execute(
                "UPDATE match_players SET player_id = ?1 WHERE player_id = ?2",
                params![target_id, id],
            )?;

            conn.execute("DELETE FROM players WHERE id = ?1", params![id])?;
        } else {
            conn.execute(
                "UPDATE players SET player_key = ?1, platform = ?2 WHERE id = ?3",
                params![&new_key, &norm_platform, id],
            )?;
        }
    }

    Ok(())
}

fn cleanup_local_history_rows(conn: &Connection) -> Result<(), HistoryError> {
    conn.execute("DELETE FROM match_players WHERE role = 'local'", [])?;
    conn.execute(
        "DELETE FROM players
         WHERE NOT EXISTS (
            SELECT 1 FROM match_players mp WHERE mp.player_id = players.id
         )",
        [],
    )?;
    Ok(())
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
        SessionMode::Snowday => "snowday",
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
    use rusqlite::OptionalExtension;

    fn query_summary(
        conn: &Connection,
        player_key: &str,
    ) -> Result<Option<PlayerHistorySummary>, HistoryError> {
        let res = conn.query_row(
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
             JOIN match_players mp ON mp.player_id = p.id
                AND mp.role IN ('teammate', 'opponent')
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
        .optional()?;
        Ok(res)
    }

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
        assert!(query_summary(&conn, "steam:steam|me|0").unwrap().is_none());
    }

    #[test]
    fn all_player_summaries_and_totals_exclude_local_player() {
        let mut conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        let mut session = SessionState::default();
        session.active_match_id = "match-a".to_string();
        session.active_mode = SessionMode::Twos;
        session.local_team = Some(0);
        session.blue_score = 3;
        session.orange_score = 1;
        session.last_result = MatchResult::Win;
        let players = [
            player("Me", "Steam|me|0", 0, true),
            player("Friend", "Steam|friend|0", 0, false),
            player("Opponent", "Steam|opponent|0", 1, false),
        ];

        assert!(insert_completed_match_on_conn(&mut conn, &session, players.iter(), 1000).unwrap());
        let summaries = load_all_player_summaries_on_conn(&conn).unwrap();
        let totals = load_totals_on_conn(&conn).unwrap();

        assert_eq!(summaries.len(), 2);
        assert!(summaries.iter().all(|summary| summary.name != "Me"));
        assert_eq!(totals.matches, 1);
        assert_eq!(totals.players, 2);
        let friend = summaries
            .iter()
            .find(|summary| summary.name == "Friend")
            .unwrap();
        let opponent = summaries
            .iter()
            .find(|summary| summary.name == "Opponent")
            .unwrap();
        assert_eq!(friend.games_with, 1);
        assert_eq!(friend.wins_with, 1);
        assert_eq!(opponent.games_against, 1);
        assert_eq!(opponent.wins_against, 1);
    }

    #[test]
    fn cleanup_removes_legacy_local_rows_and_preserves_encounters() {
        let mut conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        let mut session = SessionState::default();
        session.active_match_id = "match-a".to_string();
        session.active_mode = SessionMode::Twos;
        session.local_team = Some(0);
        session.blue_score = 0;
        session.orange_score = 2;
        session.last_result = MatchResult::Loss;
        let players = [
            player("Me", "Steam|me|0", 0, true),
            player("Friend", "Steam|friend|0", 0, false),
            player("Opponent", "Steam|opponent|0", 1, false),
        ];

        insert_legacy_completed_match_with_local_rows(&mut conn, &session, players.iter(), 1000)
            .unwrap();
        cleanup_local_history_rows(&conn).unwrap();

        let summaries = load_all_player_summaries_on_conn(&conn).unwrap();
        let totals = load_totals_on_conn(&conn).unwrap();

        assert_eq!(summaries.len(), 2);
        assert!(summaries.iter().all(|summary| summary.name != "Me"));
        assert_eq!(totals.players, 2);
        let friend = query_summary(&conn, "steam:steam|friend|0")
            .unwrap()
            .unwrap();
        let opponent = query_summary(&conn, "steam:steam|opponent|0")
            .unwrap()
            .unwrap();
        assert_eq!(friend.games_with, 1);
        assert_eq!(friend.losses_with, 1);
        assert_eq!(opponent.games_against, 1);
        assert_eq!(opponent.losses_against, 1);
        assert!(query_summary(&conn, "steam:steam|me|0").unwrap().is_none());
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

    fn insert_legacy_completed_match_with_local_rows<'a>(
        conn: &mut Connection,
        session: &SessionState,
        players: impl Iterator<Item = &'a PlayerInfo>,
        ended_unix_ms: i64,
    ) -> Result<bool, HistoryError> {
        let tx = conn.transaction()?;
        let local_team = session.local_team.unwrap_or(NO_TEAM);
        let inserted = tx.execute(
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
        )?;

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
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![
                    key.as_str(),
                    player.name.trim(),
                    player.platform.trim(),
                    player.primary_id.trim(),
                    ended_unix_ms,
                ],
            )?;
            let player_id = tx.query_row(
                "SELECT id FROM players WHERE player_key = ?1",
                params![key.as_str()],
                |row| row.get::<_, i64>(0),
            )?;
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
            )?;
        }

        tx.commit()?;
        Ok(true)
    }

    #[test]
    fn test_database_platform_migration_and_merge() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();

        // 1. Insert a player on a legacy platform "PS4"
        conn.execute(
            "INSERT INTO players (player_key, latest_name, platform, primary_id, first_seen_unix_ms, last_seen_unix_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params!["ps4:123", "Bob", "PS4", "123", 1000],
        ).unwrap();
        let bob_ps4_id: i64 = conn
            .query_row(
                "SELECT id FROM players WHERE player_key = 'ps4:123'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        // 2. Insert another player on a newer normalized platform "PlayStation" with same primary ID (collision target)
        conn.execute(
            "INSERT INTO players (player_key, latest_name, platform, primary_id, first_seen_unix_ms, last_seen_unix_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params!["playstation:123", "Bob", "PlayStation", "123", 2000],
        ).unwrap();
        let _bob_playstation_id: i64 = conn
            .query_row(
                "SELECT id FROM players WHERE player_key = 'playstation:123'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        // 3. Insert a third player who does NOT collide (e.g. XboxOne which should become Xbox)
        conn.execute(
            "INSERT INTO players (player_key, latest_name, platform, primary_id, first_seen_unix_ms, last_seen_unix_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params!["xboxone:456", "Alice", "XboxOne", "456", 1500],
        ).unwrap();

        // Let's add matches/match_players for them
        conn.execute(
            "INSERT INTO matches (match_guid, mode, result, blue_score, orange_score, local_team, ended_unix_ms)
             VALUES ('match1', 'ones', 'win', 1, 0, 0, 1000)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO match_players (match_id, player_id, team, role, score, goals, saves, touches, demos)
             VALUES (1, ?1, 1, 'opponent', 100, 0, 0, 0, 0)",
            params![bob_ps4_id],
        ).unwrap();

        // Perform migration
        migrate_database_platforms(&conn).unwrap();

        // Assertions:
        // Alice should be updated to "Xbox" and key "xbox:456"
        let alice_platform: String = conn
            .query_row(
                "SELECT platform FROM players WHERE primary_id = '456'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let alice_key: String = conn
            .query_row(
                "SELECT player_key FROM players WHERE primary_id = '456'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(alice_platform, "Xbox");
        assert_eq!(alice_key, "xbox:456");

        // Bob should be merged into a single player row. One of the duplicate player IDs should be deleted.
        // Let's count how many Bobs exist.
        let bob_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM players WHERE primary_id = '123'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bob_count, 1);

        // The remaining Bob should have platform "PSN" and player_key "psn:123"
        let bob_platform: String = conn
            .query_row(
                "SELECT platform FROM players WHERE primary_id = '123'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let bob_key: String = conn
            .query_row(
                "SELECT player_key FROM players WHERE primary_id = '123'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bob_platform, "PSN");
        assert_eq!(bob_key, "psn:123");

        // The match_players references should have been updated to the remaining Bob's ID
        let remaining_bob_id: i64 = conn
            .query_row("SELECT id FROM players WHERE primary_id = '123'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let match_player_pid: i64 = conn
            .query_row(
                "SELECT player_id FROM match_players WHERE match_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(match_player_pid, remaining_bob_id);
    }
}
