use rusqlite::{params, Connection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownDevice {
    pub serial: String,
    pub alias: Option<String>,
    pub last_seen_at: String,
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
