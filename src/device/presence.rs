use std::collections::HashMap;
use std::path::Path;

use rusqlite::Connection;

use crate::catalog::books as catalog_books;
use crate::device::books::{self, Presence};

// Presence of a catalog book relative to the connected device, for the Library
// view. `state` is `Both`/`Modified`/`LocalOnly` (never `DeviceOnly`/`Conflict`,
// which describe the device side). `device_format` is set only when the device
// copy's format differs from the catalog format, so the renderer can show both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryPresence {
    pub state: Presence,
    pub device_format: Option<String>,
}

// Map every catalog book id to its presence against `serial`'s device. Reuses
// `books::list` (a single device scan) rather than re-deriving the match: each
// book starts `LocalOnly` and is upgraded to `Both`/`Modified` when a device file
// matches it. Device-only files and ambiguous matches contribute no catalog id.
pub fn library_presence(
    conn: &Connection,
    serial: &str,
    mount: &Path,
) -> books::Result<HashMap<i64, LibraryPresence>> {
    let catalog = catalog_books::handle_ls(conn)?;
    let formats: HashMap<i64, &str> = catalog.iter().map(|b| (b.id, b.format.as_str())).collect();

    let mut out: HashMap<i64, LibraryPresence> = catalog
        .iter()
        .map(|b| {
            (
                b.id,
                LibraryPresence {
                    state: Presence::LocalOnly,
                    device_format: None,
                },
            )
        })
        .collect();

    for db in books::list(conn, serial, mount)? {
        let Some(id) = db.matched_book_id else {
            continue;
        };
        let state = match db.presence {
            Presence::Modified => Presence::Modified,
            // A matched device file is `Both`; `DeviceOnly`/`Conflict` carry no id
            // and are filtered above, so anything else collapses to `Both`.
            _ => Presence::Both,
        };
        let device_format = formats
            .get(&id)
            .filter(|local| !db.format.eq_ignore_ascii_case(local))
            .map(|_| db.format.clone());
        out.insert(
            id,
            LibraryPresence {
                state,
                device_format,
            },
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;
    use crate::catalog::devices;
    use rusqlite::params;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn fresh() -> (tempfile::TempDir, Connection) {
        let dir = tempdir().unwrap();
        let conn = catalog::init(&dir.path().join("cat")).unwrap();
        (dir, conn)
    }

    // Author is left NULL so the title-only metadata read from the stub device
    // files (which carry no author) matches by normalized key.
    fn add_book(conn: &Connection, title: &str, format: &str) -> i64 {
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES (?1, NULL, ?2, '')",
            params![title, format],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn make_mount(files: &[&str]) -> tempfile::TempDir {
        let mount = tempdir().unwrap();
        let docs = mount.path().join("documents");
        fs::create_dir_all(&docs).unwrap();
        for name in files {
            fs::write(docs.join(name), b"x").unwrap();
        }
        mount
    }

    #[test]
    fn catalog_only_book_is_local_only() {
        let (_dir, conn) = fresh();
        let id = add_book(&conn, "Dune", "epub");
        let mount = make_mount(&[]);

        let map = library_presence(&conn, "AAA", mount.path()).unwrap();
        assert_eq!(map[&id].state, Presence::LocalOnly);
        assert_eq!(map[&id].device_format, None);
    }

    #[test]
    fn matched_same_format_is_both_without_device_format() {
        let (_dir, conn) = fresh();
        let id = add_book(&conn, "Dune", "txt");
        let mount = make_mount(&["Dune.txt"]);

        let map = library_presence(&conn, "AAA", mount.path()).unwrap();
        assert_eq!(map[&id].state, Presence::Both);
        assert_eq!(map[&id].device_format, None);
    }

    #[test]
    fn matched_different_format_records_device_format() {
        let (_dir, conn) = fresh();
        let id = add_book(&conn, "Dune", "txt");
        let mount = make_mount(&["Dune.epub"]);

        let map = library_presence(&conn, "AAA", mount.path()).unwrap();
        assert_eq!(map[&id].state, Presence::Both);
        assert_eq!(map[&id].device_format.as_deref(), Some("epub"));
    }

    #[test]
    fn diverged_tracked_book_is_modified() {
        let (_dir, conn) = fresh();
        devices::record_seen(&conn, "AAA").unwrap();
        let id = add_book(&conn, "Dune", "txt");
        let mount = make_mount(&["Dune.txt"]);
        devices::record_sync(
            &conn,
            "AAA",
            id,
            &PathBuf::from("documents/Dune.txt"),
            "h",
            999, // size != on-disk → diverged
            1,
        )
        .unwrap();

        let map = library_presence(&conn, "AAA", mount.path()).unwrap();
        assert_eq!(map[&id].state, Presence::Modified);
    }

    #[test]
    fn device_only_file_does_not_downgrade_other_books() {
        let (_dir, conn) = fresh();
        let kept = add_book(&conn, "Dune", "epub");
        // A device file with no catalog match must not touch `kept`.
        let mount = make_mount(&["Strange.epub"]);

        let map = library_presence(&conn, "AAA", mount.path()).unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(map[&kept].state, Presence::LocalOnly);
    }
}
