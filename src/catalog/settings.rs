use rusqlite::{params, Connection};

use crate::catalog::columns::LibraryColumn;

const KEY_LIBRARY_COLUMNS: &str = "library.columns";

pub fn load_library_columns(conn: &Connection) -> rusqlite::Result<Vec<LibraryColumn>> {
    let raw = match get_value(conn, KEY_LIBRARY_COLUMNS)? {
        Some(v) => v,
        None => return Ok(LibraryColumn::DEFAULT.to_vec()),
    };
    let slugs: Vec<String> = match serde_json::from_str(&raw) {
        Ok(v) => v,
        // Treat malformed/legacy data as "no setting" — fall back to defaults
        // instead of failing at startup.
        Err(_) => return Ok(LibraryColumn::DEFAULT.to_vec()),
    };
    let mut cols: Vec<LibraryColumn> = slugs
        .iter()
        .filter_map(|s| LibraryColumn::from_slug(s))
        .collect();
    if cols.is_empty() {
        cols = LibraryColumn::DEFAULT.to_vec();
    }
    Ok(cols)
}

pub fn save_library_columns(conn: &Connection, cols: &[LibraryColumn]) -> rusqlite::Result<()> {
    let slugs: Vec<&str> = cols.iter().map(|c| c.slug()).collect();
    let value = serde_json::to_string(&slugs).expect("LibraryColumn slugs always serialize");
    set_value(conn, KEY_LIBRARY_COLUMNS, &value)
}

fn get_value(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    let mut stmt = conn.prepare_cached("SELECT value FROM settings WHERE key = ?1")?;
    let mut rows = stmt.query(params![key])?;
    match rows.next()? {
        Some(row) => row.get::<_, String>(0).map(Some),
        None => Ok(None),
    }
}

fn set_value(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
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

    #[test]
    fn load_returns_defaults_when_unset() {
        let (_dir, conn) = open_fresh();
        let cols = load_library_columns(&conn).unwrap();
        assert_eq!(cols, LibraryColumn::DEFAULT.to_vec());
    }

    #[test]
    fn save_then_load_round_trips_selection() {
        let (_dir, conn) = open_fresh();
        let chosen = vec![
            LibraryColumn::Id,
            LibraryColumn::Title,
            LibraryColumn::Rating,
            LibraryColumn::Series,
        ];
        save_library_columns(&conn, &chosen).unwrap();
        let loaded = load_library_columns(&conn).unwrap();
        assert_eq!(loaded, chosen);
    }

    #[test]
    fn save_overwrites_previous_setting() {
        let (_dir, conn) = open_fresh();
        save_library_columns(&conn, &[LibraryColumn::Id, LibraryColumn::Title]).unwrap();
        save_library_columns(&conn, &[LibraryColumn::Author, LibraryColumn::Tags]).unwrap();
        let loaded = load_library_columns(&conn).unwrap();
        assert_eq!(loaded, vec![LibraryColumn::Author, LibraryColumn::Tags]);
    }

    #[test]
    fn load_falls_back_to_defaults_on_malformed_json() {
        let (_dir, conn) = open_fresh();
        set_value(&conn, KEY_LIBRARY_COLUMNS, "not-json").unwrap();
        let cols = load_library_columns(&conn).unwrap();
        assert_eq!(cols, LibraryColumn::DEFAULT.to_vec());
    }

    #[test]
    fn load_ignores_unknown_slugs() {
        let (_dir, conn) = open_fresh();
        set_value(
            &conn,
            KEY_LIBRARY_COLUMNS,
            r#"["title", "made_up", "author"]"#,
        )
        .unwrap();
        let cols = load_library_columns(&conn).unwrap();
        assert_eq!(cols, vec![LibraryColumn::Title, LibraryColumn::Author]);
    }

    #[test]
    fn load_falls_back_to_defaults_when_all_slugs_unknown() {
        let (_dir, conn) = open_fresh();
        set_value(&conn, KEY_LIBRARY_COLUMNS, r#"["foo", "bar"]"#).unwrap();
        let cols = load_library_columns(&conn).unwrap();
        assert_eq!(cols, LibraryColumn::DEFAULT.to_vec());
    }
}
