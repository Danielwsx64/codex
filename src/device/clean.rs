use std::path::{Path, PathBuf};

use rusqlite::Connection;
use thiserror::Error;

use crate::catalog::devices;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io error on `{}`: {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedFile {
    // Relative to the mount root, e.g. `documents/Author_-_Title.epub`.
    pub device_path: PathBuf,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CleanOutcome {
    pub removed: Vec<RemovedFile>,
    pub total_bytes: u64,
}

// Delete files from a device and drop their sync state. `device_paths` are
// relative to the mount root (as produced by `books::list`). This never reads or
// writes the `books` table, so the local catalog is untouched by construction —
// removal lives only on the device side. A file already gone from disk is
// tolerated (counted as 0 bytes, its sync row still cleared) so a re-run after a
// partial deletion converges instead of failing.
pub fn clean(
    conn: &Connection,
    serial: &str,
    mount: &Path,
    device_paths: &[PathBuf],
) -> Result<CleanOutcome> {
    let mut outcome = CleanOutcome::default();
    for device_path in device_paths {
        let abs = mount.join(device_path);
        let bytes = match std::fs::metadata(&abs) {
            Ok(meta) => meta.len(),
            // Already gone: nothing to free, but still clear any stale sync row.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
            Err(source) => return Err(Error::Io { path: abs, source }),
        };
        match std::fs::remove_file(&abs) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(Error::Io { path: abs, source }),
        }
        devices::delete_synced_path(conn, serial, device_path)?;
        outcome.total_bytes += bytes;
        outcome.removed.push(RemovedFile {
            device_path: device_path.clone(),
            bytes,
        });
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;
    use rusqlite::params;
    use std::fs;
    use tempfile::tempdir;

    fn fresh() -> (tempfile::TempDir, Connection) {
        let dir = tempdir().unwrap();
        let conn = catalog::init(&dir.path().join("cat")).unwrap();
        (dir, conn)
    }

    fn make_mount(files: &[&str]) -> tempfile::TempDir {
        let mount = tempdir().unwrap();
        let docs = mount.path().join("documents");
        fs::create_dir_all(&docs).unwrap();
        for name in files {
            fs::write(docs.join(name), b"book bytes").unwrap();
        }
        mount
    }

    fn record(conn: &Connection, serial: &str, rel: &str) {
        devices::record_seen(conn, serial).unwrap();
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES ('B', 'A', 'epub', '')",
            [],
        )
        .unwrap();
        let book_id = conn.last_insert_rowid();
        devices::record_sync(conn, serial, book_id, Path::new(rel), "h", 9, 1).unwrap();
    }

    #[test]
    fn removes_file_and_sync_row() {
        let (_dir, conn) = fresh();
        let mount = make_mount(&["Dune.epub"]);
        record(&conn, "AAA", "documents/Dune.epub");

        let outcome = clean(
            &conn,
            "AAA",
            mount.path(),
            &[PathBuf::from("documents/Dune.epub")],
        )
        .unwrap();

        assert_eq!(outcome.removed.len(), 1);
        assert_eq!(outcome.total_bytes, b"book bytes".len() as u64);
        assert!(!mount.path().join("documents/Dune.epub").exists());
        assert!(devices::synced_paths(&conn, "AAA").unwrap().is_empty());
    }

    #[test]
    fn device_only_file_is_removed_without_a_row() {
        let (_dir, conn) = fresh();
        devices::record_seen(&conn, "AAA").unwrap();
        let mount = make_mount(&["Stray.epub"]);

        let outcome = clean(
            &conn,
            "AAA",
            mount.path(),
            &[PathBuf::from("documents/Stray.epub")],
        )
        .unwrap();

        assert_eq!(outcome.removed.len(), 1);
        assert!(!mount.path().join("documents/Stray.epub").exists());
    }

    #[test]
    fn sums_bytes_across_files() {
        let (_dir, conn) = fresh();
        let mount = make_mount(&["a.epub", "b.epub"]);

        let outcome = clean(
            &conn,
            "AAA",
            mount.path(),
            &[
                PathBuf::from("documents/a.epub"),
                PathBuf::from("documents/b.epub"),
            ],
        )
        .unwrap();

        assert_eq!(outcome.removed.len(), 2);
        assert_eq!(outcome.total_bytes, 2 * b"book bytes".len() as u64);
    }

    #[test]
    fn missing_file_on_disk_is_tolerated() {
        let (_dir, conn) = fresh();
        let mount = make_mount(&[]);
        // Sync row exists but the file was already deleted off the device.
        devices::record_seen(&conn, "AAA").unwrap();
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES ('B', 'A', 'epub', '')",
            [],
        )
        .unwrap();
        let book_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO device_books (device_serial, book_id, device_path, hash, size, mtime)
             VALUES ('AAA', ?1, 'documents/Gone.epub', 'h', 9, 1)",
            params![book_id],
        )
        .unwrap();

        let outcome = clean(
            &conn,
            "AAA",
            mount.path(),
            &[PathBuf::from("documents/Gone.epub")],
        )
        .unwrap();

        assert_eq!(outcome.removed.len(), 1);
        assert_eq!(outcome.removed[0].bytes, 0);
        // The stale sync row is cleared even though the file was already gone.
        assert!(devices::synced_paths(&conn, "AAA").unwrap().is_empty());
    }
}
