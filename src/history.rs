use crate::session::{MatchResult, SessionMode, SessionState};
use crate::state::{AppState, NO_TEAM, PlayerInfo, PlayerKey, config_dir};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    pub name_normalized: String,
    pub platform_normalized: String,
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

fn player_history_summary_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<PlayerHistorySummary> {
    let name: String = row.get(1)?;
    let platform: String = row.get(2)?;
    Ok(PlayerHistorySummary {
        player_key: row.get(0)?,
        name_normalized: name.to_ascii_lowercase(),
        platform_normalized: crate::stats_api_parser::format_platform(&platform)
            .to_ascii_lowercase(),
        name,
        platform,
        games_with: row.get(3)?,
        games_against: row.get(4)?,
        wins_with: row.get(5)?,
        losses_with: row.get(6)?,
        wins_against: row.get(7)?,
        losses_against: row.get(8)?,
        last_seen_unix_ms: row.get(9)?,
    })
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryTotals {
    pub matches: u32,
    pub players: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryPlayersSnapshot {
    pub players: Vec<PlayerHistorySummary>,
    pub revision: u64,
    pub refreshing: bool,
    pub loaded: bool,
    pub error: String,
}

pub fn player_key(player: &PlayerInfo) -> Option<PlayerKey> {
    PlayerKey::from_account(player)
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
        match initialize_database_at_with_recovery(state.paths.config_dir.clone()) {
            Ok((conn, recovery_message)) => {
                *guard = Some(conn);
                if let Some(message) = recovery_message
                    && let Ok(mut status) = state.history.status.lock()
                {
                    *status = message;
                }
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
        match initialize_database_at_with_recovery(state.paths.config_dir.clone()) {
            Ok((conn, recovery_message)) => {
                *guard = Some(conn);
                if let Some(message) = recovery_message
                    && let Ok(mut status) = state.history.status.lock()
                {
                    *status = message;
                }
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
    let config_dir = config_dir().ok_or_else(|| {
        HistoryError::ConfigDir("Could not resolve config directory.".to_string())
    })?;
    initialize_database_at(config_dir)
}

pub fn initialize_database_at(config_dir: PathBuf) -> Result<Connection, HistoryError> {
    initialize_database_at_with_recovery(config_dir).map(|(conn, _)| conn)
}

pub fn initialize_database_at_with_recovery(
    config_dir: PathBuf,
) -> Result<(Connection, Option<String>), HistoryError> {
    let path = config_dir.join("history.sqlite3");
    match initialize_database_path(&path) {
        Ok(conn) => Ok((conn, None)),
        Err(error) if is_corruption_error(&error) && path.exists() => {
            let corrupt_path = move_corrupt_database(&path)?;
            let conn = initialize_database_path(&path)?;
            Ok((
                conn,
                Some(format!(
                    "History database was corrupt and was moved to {}. A fresh database was created.",
                    corrupt_path.display()
                )),
            ))
        }
        Err(error) => Err(error),
    }
}

fn initialize_database_path(path: &std::path::Path) -> Result<Connection, HistoryError> {
    let mut conn = open_connection_path(path.to_path_buf())?;
    init_schema(&conn)?;
    run_versioned_migrations(&mut conn)?;
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
    let local_identity = state.game.local_player_identity.load();
    let local_player_name = state.game.local_player_name.load();
    let keys: Vec<String> = players
        .values()
        .filter(|player| !is_local_history_player(player, &local_identity, &local_player_name))
        .filter_map(player_key)
        .map(|key| key.as_str().to_string())
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

fn is_local_history_player(
    player: &PlayerInfo,
    local_identity: &crate::state::LocalPlayerIdentity,
    local_player_name: &str,
) -> bool {
    if player.is_local {
        return true;
    }

    let player_name = player.name.trim();
    if !local_player_name.trim().is_empty()
        && player_name.eq_ignore_ascii_case(local_player_name.trim())
    {
        return true;
    }

    if !local_identity.is_known() {
        return false;
    }

    let same_account = !player.primary_id.trim().is_empty()
        && !player.platform.trim().is_empty()
        && local_identity
            .primary_id
            .eq_ignore_ascii_case(player.primary_id.trim())
        && local_identity
            .platform
            .eq_ignore_ascii_case(player.platform.trim());

    same_account || player_name.eq_ignore_ascii_case(local_identity.name.trim())
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

pub fn request_all_player_history_refresh(state: &Arc<AppState>, force: bool) {
    if !state.system.config.load().history_enabled {
        state
            .history
            .all_players_snapshot
            .store(Arc::new(HistoryPlayersSnapshot::default()));
        return;
    }

    let revision = state.history.revision.load(Ordering::SeqCst);
    let current = state.history.all_players_snapshot.load();
    if !force && current.loaded && current.revision == revision {
        return;
    }
    if state
        .history
        .all_players_refresh_running
        .swap(true, Ordering::SeqCst)
    {
        return;
    }

    state
        .history
        .all_players_snapshot
        .store(Arc::new(HistoryPlayersSnapshot {
            refreshing: true,
            ..(**current).clone()
        }));

    let state_clone = state.clone();
    let run = move || {
        let snapshot = match load_all_player_summaries(&state_clone) {
            Ok(players) => {
                set_status(&state_clone, "History ready.");
                HistoryPlayersSnapshot {
                    players,
                    revision,
                    refreshing: false,
                    loaded: true,
                    error: String::new(),
                }
            }
            Err(error) => {
                let previous = state_clone.history.all_players_snapshot.load();
                set_status(&state_clone, &format!("History error: {error}"));
                HistoryPlayersSnapshot {
                    refreshing: false,
                    error: error.to_string(),
                    ..(**previous).clone()
                }
            }
        };
        state_clone
            .history
            .all_players_snapshot
            .store(Arc::new(snapshot));
        state_clone
            .history
            .all_players_refresh_running
            .store(false, Ordering::SeqCst);
        refresh_totals(&state_clone);
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

    let rows = stmt.query_map([], player_history_summary_from_row)?;

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
        let rows = stmt.query_map(
            rusqlite::params_from_iter(keys),
            player_history_summary_from_row,
        )?;

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

fn open_connection_path(path: PathBuf) -> Result<Connection, HistoryError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    Ok(conn)
}

fn is_corruption_error(error: &HistoryError) -> bool {
    match error {
        HistoryError::Database(rusqlite::Error::SqliteFailure(sqlite_error, message)) => {
            matches!(
                sqlite_error.code,
                rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase
            ) || message
                .as_deref()
                .is_some_and(|message| message.to_ascii_lowercase().contains("not a database"))
        }
        _ => false,
    }
}

fn move_corrupt_database(path: &std::path::Path) -> Result<PathBuf, HistoryError> {
    let parent = path
        .parent()
        .ok_or_else(|| HistoryError::ConfigDir("History database path is invalid.".to_string()))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let corrupt_path = parent.join(format!("history.corrupt-{timestamp}.sqlite3"));
    std::fs::rename(path, &corrupt_path)?;
    Ok(corrupt_path)
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

fn run_versioned_migrations(conn: &mut Connection) -> Result<(), HistoryError> {
    let version: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version < 1 {
        migrate_database_platforms(conn)?;
        cleanup_local_history_rows(conn)?;
        conn.pragma_update(None, "user_version", 1_u32)?;
    }
    Ok(())
}

fn migrate_database_platforms(conn: &mut Connection) -> Result<(), HistoryError> {
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

    let tx = conn.transaction()?;
    for (id, new_key, norm_platform) in updates {
        let existing_id: Option<i64> = tx
            .query_row(
                "SELECT id FROM players WHERE player_key = ?1 AND id != ?2",
                params![&new_key, id],
                |r| r.get(0),
            )
            .optional()?;

        if let Some(target_id) = existing_id {
            // Collision! Merge 'id' into 'target_id'.
            tx.execute(
                "DELETE FROM match_players 
                 WHERE player_id = ?1 
                   AND match_id IN (SELECT match_id FROM match_players WHERE player_id = ?2)",
                params![id, target_id],
            )?;

            tx.execute(
                "UPDATE match_players SET player_id = ?1 WHERE player_id = ?2",
                params![target_id, id],
            )?;

            tx.execute("DELETE FROM players WHERE id = ?1", params![id])?;
        } else {
            tx.execute(
                "UPDATE players SET player_key = ?1, platform = ?2 WHERE id = ?3",
                params![&new_key, &norm_platform, id],
            )?;
        }
    }
    tx.commit()?;

    Ok(())
}

fn cleanup_local_history_rows(conn: &mut Connection) -> Result<(), HistoryError> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM match_players WHERE role = 'local'", [])?;
    tx.execute(
        "DELETE FROM players
         WHERE NOT EXISTS (
            SELECT 1 FROM match_players mp WHERE mp.player_id = players.id
         )",
        [],
    )?;
    tx.commit()?;
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
        .map(duration_millis_i64)
        .unwrap_or_default()
}

fn duration_millis_i64(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
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
            player_history_summary_from_row,
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
    fn duration_millis_i64_saturates_when_duration_exceeds_i64() {
        assert_eq!(duration_millis_i64(Duration::from_millis(123)), 123);
        assert_eq!(
            duration_millis_i64(
                Duration::from_millis(i64::MAX as u64).saturating_add(Duration::from_millis(1))
            ),
            i64::MAX
        );
    }

    #[test]
    fn local_history_player_matches_flag_identity_or_local_name() {
        let identity = crate::state::LocalPlayerIdentity {
            name: "CachedName".to_string(),
            primary_id: "Steam|123|0".to_string(),
            platform: "Steam".to_string(),
        };
        let local_by_flag = PlayerInfo {
            is_local: true,
            ..PlayerInfo::default()
        };
        let local_by_identity = PlayerInfo {
            name: "Renamed".to_string(),
            primary_id: "steam|123|0".to_string(),
            platform: "steam".to_string(),
            ..Default::default()
        };
        let local_by_name = PlayerInfo {
            name: "CurrentName".to_string(),
            primary_id: "Epic|999|0".to_string(),
            platform: "Epic".to_string(),
            ..Default::default()
        };
        let opponent = PlayerInfo {
            name: "Opponent".to_string(),
            primary_id: "Steam|999|0".to_string(),
            platform: "Steam".to_string(),
            ..Default::default()
        };

        assert!(is_local_history_player(&local_by_flag, &identity, ""));
        assert!(is_local_history_player(&local_by_identity, &identity, ""));
        assert!(is_local_history_player(
            &local_by_name,
            &crate::state::LocalPlayerIdentity::default(),
            "currentname"
        ));
        assert!(!is_local_history_player(
            &opponent,
            &identity,
            "CurrentName"
        ));
    }

    #[test]
    fn corrupt_history_database_is_moved_and_recreated() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("history.sqlite3");
        std::fs::write(&db_path, b"not sqlite").unwrap();

        let (_conn, recovery) =
            initialize_database_at_with_recovery(temp.path().to_path_buf()).unwrap();

        let message = recovery.expect("expected recovery message");
        assert!(message.contains("was corrupt"));
        assert!(message.contains(".corrupt-"));
        assert!(db_path.exists());
        assert_eq!(
            std::fs::read_dir(temp.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().contains(".corrupt-"))
                .count(),
            1
        );
    }

    #[test]
    fn non_corrupt_history_open_failure_is_not_moved() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("history.sqlite3");
        std::fs::create_dir(&db_path).unwrap();

        let error = initialize_database_at_with_recovery(temp.path().to_path_buf()).unwrap_err();

        assert!(!is_corruption_error(&error));
        assert!(db_path.is_dir());
        assert_eq!(
            std::fs::read_dir(temp.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().contains(".corrupt-"))
                .count(),
            0
        );
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
        cleanup_local_history_rows(&mut conn).unwrap();

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
        let mut conn = Connection::open_in_memory().unwrap();
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
        migrate_database_platforms(&mut conn).unwrap();

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
