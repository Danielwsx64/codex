use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no device matches `{target}`")]
    DeviceNotFound { target: String },
    #[error("alias `{alias}` is already used by device `{serial}`")]
    AliasTaken { alias: String, serial: String },
    #[error("alias must not be empty")]
    EmptyAlias,
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownDevice {
    pub serial: String,
    pub alias: Option<String>,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasOutcome {
    pub serial: String,
    pub alias: String,
    pub previous: Option<String>,
}

pub fn record_seen(conn: &Connection, serial: &str) -> rusqlite::Result<()> {
    // The upsert deliberately leaves `alias` untouched so a nickname
    // survives re-detection.
    let mut stmt = conn.prepare_cached(
        "INSERT INTO devices (serial, last_seen_at) VALUES (?1, datetime('now'))
         ON CONFLICT(serial) DO UPDATE SET last_seen_at = datetime('now')",
    )?;
    stmt.execute(params![serial])?;
    Ok(())
}

pub fn list(conn: &Connection) -> rusqlite::Result<Vec<KnownDevice>> {
    let mut stmt = conn.prepare(
        "SELECT serial, alias, last_seen_at FROM devices
         ORDER BY COALESCE(alias, serial) COLLATE NOCASE, serial",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(KnownDevice {
            serial: row.get(0)?,
            alias: row.get(1)?,
            last_seen_at: row.get(2)?,
        })
    })?;
    rows.collect()
}

// The exact (no-guessing) layer of device↔catalog identity: every file cdx has
// itself synced to this device, keyed by its on-device path. `cdx device books`
// trusts these over metadata matching.
pub fn synced_paths(conn: &Connection, serial: &str) -> rusqlite::Result<HashMap<PathBuf, i64>> {
    let mut stmt = conn
        .prepare_cached("SELECT device_path, book_id FROM device_books WHERE device_serial = ?1")?;
    let rows = stmt.query_map(params![serial], |row| {
        let path: String = row.get(0)?;
        let book_id: i64 = row.get(1)?;
        Ok((PathBuf::from(path), book_id))
    })?;
    rows.collect()
}

// The exact sync state for a device, one entry per file cdx synced there. Unlike
// `synced_paths`, this carries the recorded size/mtime/hash so the sync diff can
// detect a file that changed on the device (fast-path size+mtime, `--verify` hash)
// or vanished from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncedFile {
    pub book_id: i64,
    pub device_path: PathBuf,
    pub hash: String,
    pub size: i64,
    pub mtime: i64,
}

pub fn synced_state(conn: &Connection, serial: &str) -> rusqlite::Result<Vec<SyncedFile>> {
    let mut stmt = conn.prepare_cached(
        "SELECT book_id, device_path, hash, size, mtime FROM device_books WHERE device_serial = ?1",
    )?;
    let rows = stmt.query_map(params![serial], |row| {
        Ok(SyncedFile {
            book_id: row.get(0)?,
            device_path: PathBuf::from(row.get::<_, String>(1)?),
            hash: row.get(2)?,
            size: row.get(3)?,
            mtime: row.get(4)?,
        })
    })?;
    rows.collect()
}

// Write the exact sync state for a file cdx put on the device. Keyed by
// (serial, book_id), so re-pushing the same book updates the single row rather
// than duplicating it. `device_path` is stored relative to the mount root
// (e.g. `documents/Author_-_Title.epub`), matching how `synced_paths` reads it.
pub fn record_sync(
    conn: &Connection,
    serial: &str,
    book_id: i64,
    device_path: &Path,
    hash: &str,
    size: i64,
    mtime: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO device_books
            (device_serial, book_id, device_path, hash, size, mtime, synced_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))
         ON CONFLICT(device_serial, book_id) DO UPDATE SET
            device_path = excluded.device_path,
            hash        = excluded.hash,
            size        = excluded.size,
            mtime       = excluded.mtime,
            synced_at   = excluded.synced_at",
        params![
            serial,
            book_id,
            device_path.to_string_lossy(),
            hash,
            size,
            mtime
        ],
    )?;
    Ok(())
}

// Drop the exact sync state for a single file on a device, keyed by its
// on-device path. `cdx device clean` calls this after deleting the file: a synced
// file clears its row (returns 1), a file that arrived on the device by other
// means has no row (returns 0). Both are fine — the caller only removed a file.
pub fn delete_synced_path(
    conn: &Connection,
    serial: &str,
    device_path: &Path,
) -> rusqlite::Result<usize> {
    let mut stmt = conn
        .prepare_cached("DELETE FROM device_books WHERE device_serial = ?1 AND device_path = ?2")?;
    stmt.execute(params![serial, device_path.to_string_lossy()])
}

pub fn set_alias(conn: &Connection, serial: &str, alias: &str) -> rusqlite::Result<bool> {
    let n = conn.execute(
        "UPDATE devices SET alias = ?2 WHERE serial = ?1",
        params![serial, alias],
    )?;
    Ok(n == 1)
}

pub fn resolve_serial(conn: &Connection, target: &str) -> Result<String, Error> {
    // Serial is the stable identity, so an exact serial match wins over an
    // alias match (a device can't alias itself to another device's serial —
    // `handle_alias` enforces that).
    let by_serial: Option<String> = conn
        .query_row(
            "SELECT serial FROM devices WHERE serial = ?1",
            params![target],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(serial) = by_serial {
        return Ok(serial);
    }

    let by_alias: Option<String> = conn
        .query_row(
            "SELECT serial FROM devices WHERE alias = ?1 COLLATE NOCASE",
            params![target],
            |row| row.get(0),
        )
        .optional()?;
    by_alias.ok_or_else(|| Error::DeviceNotFound {
        target: target.to_string(),
    })
}

pub fn handle_alias(
    conn: &Connection,
    target: &str,
    new_alias: &str,
) -> Result<AliasOutcome, Error> {
    let alias = new_alias.trim();
    if alias.is_empty() {
        return Err(Error::EmptyAlias);
    }

    let serial = resolve_serial(conn, target)?;
    let previous: Option<String> = conn.query_row(
        "SELECT alias FROM devices WHERE serial = ?1",
        params![serial],
        |row| row.get(0),
    )?;

    // Reject collisions with a *different* device — either it already holds
    // this alias (case-insensitive) or the alias equals its serial. Keeping
    // the alias namespace unambiguous lets later `--device <alias>` selection
    // never have to guess. Re-setting the same device to the alias it already
    // holds is idempotent and allowed.
    let conflict: Option<String> = conn
        .query_row(
            "SELECT serial FROM devices
             WHERE serial != ?1 AND (alias = ?2 COLLATE NOCASE OR serial = ?2)",
            params![serial, alias],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(other) = conflict {
        return Err(Error::AliasTaken {
            alias: alias.to_string(),
            serial: other,
        });
    }

    if !set_alias(conn, &serial, alias)? {
        // `serial` came from a successful resolve, so the row must exist.
        return Err(Error::DeviceNotFound {
            target: target.to_string(),
        });
    }

    Ok(AliasOutcome {
        serial,
        alias: alias.to_string(),
        previous,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;
    use tempfile::tempdir;

    fn open_fresh() -> (tempfile::TempDir, Connection) {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        let conn = catalog::init(&cat).unwrap();
        (dir, conn)
    }

    fn count_devices(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM devices", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn record_seen_inserts_with_timestamp() {
        let (_dir, conn) = open_fresh();
        record_seen(&conn, "G000AA1234567890").unwrap();
        let devices = list(&conn).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].serial, "G000AA1234567890");
        assert_eq!(devices[0].alias, None);
        assert!(!devices[0].last_seen_at.is_empty());
    }

    #[test]
    fn record_seen_twice_keeps_one_row() {
        let (_dir, conn) = open_fresh();
        record_seen(&conn, "G000AA1234567890").unwrap();
        let first = list(&conn).unwrap().remove(0).last_seen_at;
        record_seen(&conn, "G000AA1234567890").unwrap();
        assert_eq!(count_devices(&conn), 1);
        let second = list(&conn).unwrap().remove(0).last_seen_at;
        assert!(second >= first);
    }

    #[test]
    fn record_seen_preserves_alias() {
        let (_dir, conn) = open_fresh();
        record_seen(&conn, "G000AA1234567890").unwrap();
        assert!(set_alias(&conn, "G000AA1234567890", "paperwhite").unwrap());
        record_seen(&conn, "G000AA1234567890").unwrap();
        let devices = list(&conn).unwrap();
        assert_eq!(devices[0].alias.as_deref(), Some("paperwhite"));
    }

    #[test]
    fn list_orders_by_alias_then_serial() {
        let (_dir, conn) = open_fresh();
        record_seen(&conn, "Z999").unwrap();
        record_seen(&conn, "A111").unwrap();
        record_seen(&conn, "M555").unwrap();
        assert!(set_alias(&conn, "Z999", "basement").unwrap());
        let names: Vec<String> = list(&conn)
            .unwrap()
            .into_iter()
            .map(|d| d.alias.unwrap_or(d.serial))
            .collect();
        assert_eq!(names, vec!["A111", "basement", "M555"]);
    }

    #[test]
    fn set_alias_returns_false_for_unknown_serial() {
        let (_dir, conn) = open_fresh();
        assert!(!set_alias(&conn, "nope", "x").unwrap());
    }

    #[test]
    fn resolve_serial_matches_serial() {
        let (_dir, conn) = open_fresh();
        record_seen(&conn, "G000AA1234567890").unwrap();
        assert_eq!(
            resolve_serial(&conn, "G000AA1234567890").unwrap(),
            "G000AA1234567890"
        );
    }

    #[test]
    fn resolve_serial_matches_alias_case_insensitive() {
        let (_dir, conn) = open_fresh();
        record_seen(&conn, "G000AA1234567890").unwrap();
        set_alias(&conn, "G000AA1234567890", "Paperwhite").unwrap();
        assert_eq!(
            resolve_serial(&conn, "paperwhite").unwrap(),
            "G000AA1234567890"
        );
    }

    #[test]
    fn resolve_serial_unknown_target_errors() {
        let (_dir, conn) = open_fresh();
        let err = resolve_serial(&conn, "nope").unwrap_err();
        assert!(matches!(err, Error::DeviceNotFound { target } if target == "nope"));
    }

    #[test]
    fn handle_alias_sets_and_reports_previous() {
        let (_dir, conn) = open_fresh();
        record_seen(&conn, "G000AA1234567890").unwrap();

        let first = handle_alias(&conn, "G000AA1234567890", "paperwhite").unwrap();
        assert_eq!(first.serial, "G000AA1234567890");
        assert_eq!(first.alias, "paperwhite");
        assert_eq!(first.previous, None);

        // Rename by existing alias; previous now carries the old nickname.
        let renamed = handle_alias(&conn, "paperwhite", "study").unwrap();
        assert_eq!(renamed.serial, "G000AA1234567890");
        assert_eq!(renamed.alias, "study");
        assert_eq!(renamed.previous.as_deref(), Some("paperwhite"));

        assert_eq!(list(&conn).unwrap()[0].alias.as_deref(), Some("study"));
    }

    #[test]
    fn handle_alias_trims_and_rejects_empty() {
        let (_dir, conn) = open_fresh();
        record_seen(&conn, "G000AA1234567890").unwrap();
        let err = handle_alias(&conn, "G000AA1234567890", "   ").unwrap_err();
        assert!(matches!(err, Error::EmptyAlias));
    }

    #[test]
    fn handle_alias_rejects_alias_taken_by_other_device() {
        let (_dir, conn) = open_fresh();
        record_seen(&conn, "AAA").unwrap();
        record_seen(&conn, "BBB").unwrap();
        handle_alias(&conn, "AAA", "paperwhite").unwrap();

        let err = handle_alias(&conn, "BBB", "Paperwhite").unwrap_err();
        assert!(matches!(err, Error::AliasTaken { serial, .. } if serial == "AAA"));
    }

    #[test]
    fn handle_alias_rejects_alias_colliding_with_other_serial() {
        let (_dir, conn) = open_fresh();
        record_seen(&conn, "AAA").unwrap();
        record_seen(&conn, "BBB").unwrap();

        let err = handle_alias(&conn, "AAA", "BBB").unwrap_err();
        assert!(matches!(err, Error::AliasTaken { serial, .. } if serial == "BBB"));
    }

    #[test]
    fn handle_alias_is_idempotent_for_same_device() {
        let (_dir, conn) = open_fresh();
        record_seen(&conn, "AAA").unwrap();
        handle_alias(&conn, "AAA", "paperwhite").unwrap();
        // Re-setting the same device to the alias it already holds is allowed.
        let again = handle_alias(&conn, "AAA", "paperwhite").unwrap();
        assert_eq!(again.alias, "paperwhite");
        assert_eq!(again.previous.as_deref(), Some("paperwhite"));
    }

    #[test]
    fn device_books_cascade_on_book_delete() {
        let (_dir, conn) = open_fresh();
        record_seen(&conn, "G000AA1234567890").unwrap();
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES ('B', 'A', 'epub', '')",
            [],
        )
        .unwrap();
        let book_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO device_books (device_serial, book_id, device_path, hash, size, mtime)
             VALUES ('G000AA1234567890', ?1, 'documents/B.epub', 'abc', 10, 1700000000)",
            params![book_id],
        )
        .unwrap();
        conn.execute("DELETE FROM books WHERE id = ?1", params![book_id])
            .unwrap();
        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM device_books", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0);
    }

    fn insert_book(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES ('B', 'A', 'epub', '')",
            [],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn record_sync_inserts_then_round_trips_through_synced_paths() {
        let (_dir, conn) = open_fresh();
        record_seen(&conn, "AAA").unwrap();
        let book_id = insert_book(&conn);

        record_sync(
            &conn,
            "AAA",
            book_id,
            Path::new("documents/B.epub"),
            "hash1",
            10,
            1_700_000_000,
        )
        .unwrap();

        let map = synced_paths(&conn, "AAA").unwrap();
        assert_eq!(map.get(&PathBuf::from("documents/B.epub")), Some(&book_id));
    }

    #[test]
    fn record_sync_upserts_on_repush() {
        let (_dir, conn) = open_fresh();
        record_seen(&conn, "AAA").unwrap();
        let book_id = insert_book(&conn);

        record_sync(
            &conn,
            "AAA",
            book_id,
            Path::new("documents/old.epub"),
            "hash1",
            10,
            1,
        )
        .unwrap();
        record_sync(
            &conn,
            "AAA",
            book_id,
            Path::new("documents/new.epub"),
            "hash2",
            20,
            2,
        )
        .unwrap();

        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM device_books", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1, "re-push must update the single row, not duplicate");

        let (path, hash, size): (String, String, i64) = conn
            .query_row(
                "SELECT device_path, hash, size FROM device_books WHERE book_id = ?1",
                params![book_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(path, "documents/new.epub");
        assert_eq!(hash, "hash2");
        assert_eq!(size, 20);
    }

    #[test]
    fn synced_paths_returns_path_to_book_map_for_serial() {
        let (_dir, conn) = open_fresh();
        record_seen(&conn, "AAA").unwrap();
        record_seen(&conn, "BBB").unwrap();
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES ('B', 'A', 'epub', '')",
            [],
        )
        .unwrap();
        let book_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO device_books (device_serial, book_id, device_path, hash, size, mtime)
             VALUES ('AAA', ?1, 'documents/B.epub', 'abc', 10, 1700000000)",
            params![book_id],
        )
        .unwrap();

        let map = synced_paths(&conn, "AAA").unwrap();
        assert_eq!(map.get(&PathBuf::from("documents/B.epub")), Some(&book_id));
        // A different serial sees none of AAA's rows.
        assert!(synced_paths(&conn, "BBB").unwrap().is_empty());
    }

    #[test]
    fn delete_synced_path_removes_the_matching_row() {
        let (_dir, conn) = open_fresh();
        record_seen(&conn, "AAA").unwrap();
        let book_id = insert_book(&conn);
        record_sync(
            &conn,
            "AAA",
            book_id,
            Path::new("documents/B.epub"),
            "h",
            1,
            1,
        )
        .unwrap();

        let removed = delete_synced_path(&conn, "AAA", Path::new("documents/B.epub")).unwrap();
        assert_eq!(removed, 1);
        assert!(synced_paths(&conn, "AAA").unwrap().is_empty());
    }

    #[test]
    fn delete_synced_path_for_unknown_path_is_zero() {
        let (_dir, conn) = open_fresh();
        record_seen(&conn, "AAA").unwrap();
        let removed = delete_synced_path(&conn, "AAA", Path::new("documents/ghost.epub")).unwrap();
        assert_eq!(removed, 0);
    }
}
