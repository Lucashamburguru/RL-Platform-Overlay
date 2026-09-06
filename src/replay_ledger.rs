use rusqlite::{Connection, OptionalExtension, params};
use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum ReplayLedgerError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn initialize_database_at(config_dir: PathBuf) -> Result<Connection, ReplayLedgerError> {
    initialize_database_at_with_recovery(config_dir).map(|(conn, _)| conn)
}

pub fn initialize_database_at_with_recovery(
    config_dir: PathBuf,
) -> Result<(Connection, Option<String>), ReplayLedgerError> {
    std::fs::create_dir_all(&config_dir)?;
    let path = config_dir.join("replays.sqlite3");
    match initialize_database_path(&path) {
        Ok(conn) => Ok((conn, None)),
        Err(error) if is_corruption_error(&error) && path.exists() => {
            let corrupt_path = quarantine_corrupt_database(&path)?;
            let conn = initialize_database_path(&path)?;
            Ok((
                conn,
                Some(format!(
                    "Replay upload ledger was corrupt and moved to {}. A fresh ledger was created.",
                    corrupt_path.display()
                )),
            ))
        }
        Err(error) => Err(error),
    }
}

fn initialize_database_path(path: &std::path::Path) -> Result<Connection, ReplayLedgerError> {
    secure_database_file(path)?;
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;
        CREATE TABLE IF NOT EXISTS replay_uploads (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            filename TEXT NOT NULL,
            filename_normalized TEXT NOT NULL UNIQUE,
            content_hash TEXT,
            remote_replay_id TEXT,
            file_size INTEGER,
            modified_unix_ms INTEGER,
            status TEXT NOT NULL CHECK(status IN ('legacy', 'uploaded', 'remote')),
            uploaded_unix_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_replay_uploads_recent
            ON replay_uploads(uploaded_unix_ms DESC, id DESC);
        CREATE INDEX IF NOT EXISTS idx_replay_uploads_content_hash
            ON replay_uploads(content_hash) WHERE content_hash IS NOT NULL;
        PRAGMA user_version = 1;
        ",
    )?;
    Ok(conn)
}

fn is_corruption_error(error: &ReplayLedgerError) -> bool {
    match error {
        ReplayLedgerError::Database(rusqlite::Error::SqliteFailure(sqlite_error, message)) => {
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

fn quarantine_corrupt_database(
    path: &std::path::Path,
) -> Result<std::path::PathBuf, ReplayLedgerError> {
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let corrupt_path = parent.join(format!("replays.corrupt-{timestamp}.sqlite3"));
    std::fs::rename(path, &corrupt_path)?;
    for suffix in ["-wal", "-shm"] {
        let sidecar = std::path::PathBuf::from(format!("{}{suffix}", path.display()));
        if sidecar.exists() {
            let quarantined_sidecar =
                std::path::PathBuf::from(format!("{}{suffix}", corrupt_path.display()));
            std::fs::rename(sidecar, quarantined_sidecar)?;
        }
    }
    Ok(corrupt_path)
}

#[cfg(unix)]
fn secure_database_file(path: &std::path::Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    if path.exists() {
        let permissions = std::fs::metadata(path)?.permissions();
        if permissions.mode() & 0o777 != 0o600 {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
    } else {
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(path)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn secure_database_file(_path: &std::path::Path) -> Result<(), std::io::Error> {
    Ok(())
}

pub fn import_legacy_filenames(
    conn: &mut Connection,
    filenames: &[String],
) -> Result<usize, ReplayLedgerError> {
    if filenames.is_empty() {
        return Ok(0);
    }
    let tx = conn.transaction()?;
    let mut added = 0;
    for filename in filenames {
        let filename = filename.trim();
        if filename.is_empty() {
            continue;
        }
        added += tx.execute(
            "INSERT OR IGNORE INTO replay_uploads
             (filename, filename_normalized, status, uploaded_unix_ms)
             VALUES (?1, ?2, 'legacy', ?3)",
            params![filename, filename.to_ascii_lowercase(), now_unix_ms()],
        )?;
    }
    tx.commit()?;
    Ok(added)
}

pub fn record_remote_batch(
    conn: &mut Connection,
    filenames: &[String],
) -> Result<usize, ReplayLedgerError> {
    let tx = conn.transaction()?;
    let mut added = 0;
    for filename in filenames {
        let filename = filename.trim();
        if filename.is_empty() {
            continue;
        }
        let existed = tx
            .query_row(
                "SELECT 1 FROM replay_uploads WHERE filename_normalized = ?1 LIMIT 1",
                [filename.to_ascii_lowercase()],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        tx.execute(
            "INSERT INTO replay_uploads
             (filename, filename_normalized, remote_replay_id, status, uploaded_unix_ms)
             VALUES (?1, ?2, ?3, 'remote', ?4)
             ON CONFLICT(filename_normalized) DO UPDATE SET
                filename = excluded.filename,
                remote_replay_id = excluded.remote_replay_id,
                status = 'remote',
                uploaded_unix_ms = excluded.uploaded_unix_ms",
            params![
                filename,
                filename.to_ascii_lowercase(),
                filename.strip_suffix(".replay"),
                now_unix_ms()
            ],
        )?;
        if !existed {
            added += 1;
        }
    }
    tx.commit()?;
    Ok(added)
}

pub fn contains_filename(conn: &Connection, filename: &str) -> Result<bool, ReplayLedgerError> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM replay_uploads WHERE filename_normalized = ?1 LIMIT 1",
            [filename.trim().to_ascii_lowercase()],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

pub fn contains_content_hash(
    conn: &Connection,
    content_hash: &str,
) -> Result<bool, ReplayLedgerError> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM replay_uploads WHERE content_hash = ?1 LIMIT 1",
            [content_hash],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

pub fn matches_uploaded_file(
    conn: &Connection,
    filename: &str,
    file_size: u64,
    modified_unix_ms: i64,
) -> Result<bool, ReplayLedgerError> {
    let file_size = i64::try_from(file_size).unwrap_or(i64::MAX);
    Ok(conn
        .query_row(
            "SELECT 1 FROM replay_uploads
             WHERE filename_normalized = ?1
               AND (status = 'remote'
                    OR (status = 'uploaded' AND file_size = ?2 AND modified_unix_ms = ?3))
             LIMIT 1",
            params![
                filename.trim().to_ascii_lowercase(),
                file_size,
                modified_unix_ms
            ],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

pub struct UploadedReplay<'a> {
    pub filename: &'a str,
    pub content_hash: Option<&'a str>,
    pub remote_replay_id: Option<&'a str>,
    pub file_size: Option<u64>,
    pub modified_unix_ms: Option<i64>,
    pub status: &'a str,
}

pub fn record_uploaded(
    conn: &mut Connection,
    replay: UploadedReplay<'_>,
) -> Result<bool, ReplayLedgerError> {
    let normalized = replay.filename.trim().to_ascii_lowercase();
    let size = replay.file_size.and_then(|value| i64::try_from(value).ok());
    let existed = contains_filename(conn, replay.filename)?;
    conn.execute(
        "INSERT INTO replay_uploads
         (filename, filename_normalized, content_hash, remote_replay_id, file_size,
          modified_unix_ms, status, uploaded_unix_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(filename_normalized) DO UPDATE SET
            filename = excluded.filename,
            content_hash = COALESCE(excluded.content_hash, replay_uploads.content_hash),
            remote_replay_id = COALESCE(excluded.remote_replay_id, replay_uploads.remote_replay_id),
            file_size = COALESCE(excluded.file_size, replay_uploads.file_size),
            modified_unix_ms = COALESCE(excluded.modified_unix_ms, replay_uploads.modified_unix_ms),
            status = excluded.status,
            uploaded_unix_ms = excluded.uploaded_unix_ms",
        params![
            replay.filename,
            normalized,
            replay.content_hash,
            replay.remote_replay_id,
            size,
            replay.modified_unix_ms,
            replay.status,
            now_unix_ms()
        ],
    )?;
    Ok(!existed)
}

pub fn filenames(conn: &Connection) -> Result<Vec<String>, ReplayLedgerError> {
    let mut statement =
        conn.prepare("SELECT filename FROM replay_uploads ORDER BY uploaded_unix_ms ASC, id ASC")?;
    let rows = statement.query_map([], |row| row.get(0))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn count(conn: &Connection) -> Result<usize, ReplayLedgerError> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM replay_uploads", [], |row| row.get(0))?;
    Ok(usize::try_from(count).unwrap_or(usize::MAX))
}

pub fn clear(conn: &Connection) -> Result<(), ReplayLedgerError> {
    conn.execute("DELETE FROM replay_uploads", [])?;
    Ok(())
}

fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_legacy_names_and_promotes_verified_uploads() {
        let temp = tempfile::tempdir().unwrap();
        let mut conn = initialize_database_at(temp.path().to_path_buf()).unwrap();
        assert_eq!(
            import_legacy_filenames(
                &mut conn,
                &["Example.replay".to_string(), "example.REPLAY".to_string()]
            )
            .unwrap(),
            1
        );
        assert!(contains_filename(&conn, "EXAMPLE.replay").unwrap());

        assert!(
            !record_uploaded(
                &mut conn,
                UploadedReplay {
                    filename: "Example.replay",
                    content_hash: Some("abc123"),
                    remote_replay_id: Some("remote-id"),
                    file_size: Some(42),
                    modified_unix_ms: Some(7),
                    status: "uploaded",
                }
            )
            .unwrap()
        );
        assert!(contains_content_hash(&conn, "abc123").unwrap());
        assert!(matches_uploaded_file(&conn, "example.replay", 42, 7).unwrap());
        assert!(!matches_uploaded_file(&conn, "example.replay", 43, 7).unwrap());
        assert_eq!(filenames(&conn).unwrap(), vec!["Example.replay"]);
    }

    #[test]
    fn clear_removes_the_ledger() {
        let temp = tempfile::tempdir().unwrap();
        let mut conn = initialize_database_at(temp.path().to_path_buf()).unwrap();
        import_legacy_filenames(&mut conn, &["one.replay".to_string()]).unwrap();
        clear(&conn).unwrap();
        assert_eq!(count(&conn).unwrap(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn ledger_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        initialize_database_at(temp.path().to_path_buf()).unwrap();
        let mode = std::fs::metadata(temp.path().join("replays.sqlite3"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn corrupt_ledger_is_quarantined_and_recreated() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("replays.sqlite3");
        std::fs::write(&path, b"not sqlite").unwrap();

        let (conn, recovery) =
            initialize_database_at_with_recovery(temp.path().to_path_buf()).unwrap();
        assert!(recovery.is_some());
        assert_eq!(count(&conn).unwrap(), 0);
        assert!(std::fs::read_dir(temp.path()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("replays.corrupt-")
        }));
    }
}
