use std::path::{Path, PathBuf};

use rusqlite::Connection;
use thiserror::Error;

use crate::catalog::{books as catalog_books, devices};
use crate::fingerprint;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Catalog(#[from] catalog_books::Error),
    #[error(transparent)]
    Fingerprint(#[from] fingerprint::Error),
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
    #[error("io error on `{}`: {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("book `{target}` has no stored file to push")]
    NoFile { target: String },
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushOutcome {
    pub book_id: i64,
    pub title: String,
    // Relative to the mount root, e.g. `documents/Author_-_Title.epub`.
    pub device_path: PathBuf,
    pub bytes: u64,
}

// Copy a catalog book onto a connected device and record the exact sync state.
// The on-device name reuses the catalog's already-sanitized filename, and a
// pre-existing file at that path is overwritten (re-push): sync never deletes,
// push only copies. The recorded `device_books` row is keyed by (serial,
// book_id), so re-pushing updates in place rather than duplicating.
pub fn push(
    conn: &Connection,
    catalog_dir: &Path,
    serial: &str,
    mount: &Path,
    target: &str,
) -> Result<PushOutcome> {
    let book = catalog_books::handle_inspect(conn, target)?;

    let filename = Path::new(&book.file_path)
        .file_name()
        .ok_or_else(|| Error::NoFile {
            target: target.to_string(),
        })?;
    let src = catalog_dir.join(&book.file_path);
    if !src.is_file() {
        return Err(Error::Io {
            path: src,
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "stored file missing from disk",
            ),
        });
    }

    let documents = mount.join("documents");
    std::fs::create_dir_all(&documents).map_err(|source| Error::Io {
        path: documents.clone(),
        source,
    })?;
    let dest = documents.join(filename);
    let bytes = std::fs::copy(&src, &dest).map_err(|source| Error::Io {
        path: dest.clone(),
        source,
    })?;

    let hash = fingerprint::hash_full(&dest)?;
    let mtime = super::mtime_secs(&dest).map_err(|source| Error::Io {
        path: dest.clone(),
        source,
    })?;
    let device_path = Path::new("documents").join(filename);
    devices::record_sync(
        conn,
        serial,
        book.id,
        &device_path,
        &hash,
        bytes as i64,
        mtime,
    )?;

    Ok(PushOutcome {
        book_id: book.id,
        title: book.title,
        device_path,
        bytes,
    })
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

    // Register a book with a real stored file under `<catalog_dir>/books/<id>/`.
    fn add_book(conn: &Connection, catalog_dir: &Path, title: &str, filename: &str) -> i64 {
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES (?1, 'A', 'epub', '')",
            params![title],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        let rel = format!("books/{id}/{filename}");
        let abs = catalog_dir.join(&rel);
        fs::create_dir_all(abs.parent().unwrap()).unwrap();
        fs::write(&abs, b"book bytes").unwrap();
        conn.execute(
            "UPDATE books SET file_path = ?1 WHERE id = ?2",
            params![rel, id],
        )
        .unwrap();
        id
    }

    #[test]
    fn push_copies_file_and_records_sync_state() {
        let (dir, conn) = fresh();
        let cat = dir.path().join("cat");
        devices::record_seen(&conn, "AAA").unwrap();
        let id = add_book(&conn, &cat, "Dune", "Frank_Herbert_-_Dune.epub");

        let mount = tempdir().unwrap();
        let outcome = push(&conn, &cat, "AAA", mount.path(), "Dune").unwrap();

        assert_eq!(outcome.book_id, id);
        assert_eq!(outcome.title, "Dune");
        assert_eq!(
            outcome.device_path,
            PathBuf::from("documents/Frank_Herbert_-_Dune.epub")
        );
        assert_eq!(outcome.bytes, b"book bytes".len() as u64);

        let dest = mount.path().join("documents/Frank_Herbert_-_Dune.epub");
        assert!(dest.is_file(), "file must be copied under documents/");
        assert_eq!(fs::read(&dest).unwrap(), b"book bytes");

        // Exact sync state recorded, keyed by the relative device path.
        let synced = devices::synced_paths(&conn, "AAA").unwrap();
        assert_eq!(
            synced.get(&PathBuf::from("documents/Frank_Herbert_-_Dune.epub")),
            Some(&id)
        );
    }

    #[test]
    fn push_creates_documents_dir_when_absent() {
        let (dir, conn) = fresh();
        let cat = dir.path().join("cat");
        devices::record_seen(&conn, "AAA").unwrap();
        add_book(&conn, &cat, "Dune", "Dune.epub");

        let mount = tempdir().unwrap();
        assert!(!mount.path().join("documents").exists());
        push(&conn, &cat, "AAA", mount.path(), "Dune").unwrap();
        assert!(mount.path().join("documents").is_dir());
    }

    #[test]
    fn re_push_updates_the_single_sync_row() {
        let (dir, conn) = fresh();
        let cat = dir.path().join("cat");
        devices::record_seen(&conn, "AAA").unwrap();
        add_book(&conn, &cat, "Dune", "Dune.epub");

        let mount = tempdir().unwrap();
        push(&conn, &cat, "AAA", mount.path(), "Dune").unwrap();
        push(&conn, &cat, "AAA", mount.path(), "Dune").unwrap();

        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM device_books", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn push_unknown_target_surfaces_catalog_error() {
        let (dir, conn) = fresh();
        let cat = dir.path().join("cat");
        devices::record_seen(&conn, "AAA").unwrap();
        let mount = tempdir().unwrap();

        let err = push(&conn, &cat, "AAA", mount.path(), "ghost").unwrap_err();
        assert!(matches!(
            err,
            Error::Catalog(catalog_books::Error::NotFound { .. })
        ));
    }

    #[test]
    fn push_missing_source_file_errors() {
        let (dir, conn) = fresh();
        let cat = dir.path().join("cat");
        devices::record_seen(&conn, "AAA").unwrap();
        // Book row points at a file that was never written to disk.
        conn.execute(
            "INSERT INTO books (title, author, format, file_path)
             VALUES ('Ghost', 'A', 'epub', 'books/1/Ghost.epub')",
            [],
        )
        .unwrap();

        let mount = tempdir().unwrap();
        let err = push(&conn, &cat, "AAA", mount.path(), "Ghost").unwrap_err();
        assert!(matches!(err, Error::Io { .. }));
    }
}
