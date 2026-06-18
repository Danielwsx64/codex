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
}
