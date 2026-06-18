use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use thiserror::Error;

pub mod books;
pub mod columns;
pub mod devices;
pub mod groups;
pub mod handlers;
pub mod render;
pub mod schema;
pub mod settings;
pub mod tags;

pub const DB_FILENAME: &str = "catalog.db";
pub const BOOKS_DIRNAME: &str = "books";

#[derive(Debug, Error)]
pub enum Error {
    #[error("catalog directory `{}` does not exist", .path.display())]
    MissingOnDisk { path: PathBuf },
    #[error("catalog at `{}` is already initialized", .path.display())]
    AlreadyInitialized { path: PathBuf },
    #[error("catalog at `{}` is missing its database (`{}`)", .path.display(), DB_FILENAME)]
    NotACatalog { path: PathBuf },
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Migration(#[from] rusqlite_migration::Error),
    #[error("io error on {}: {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn db_path(catalog_dir: &Path) -> PathBuf {
    catalog_dir.join(DB_FILENAME)
}

pub fn books_path(catalog_dir: &Path) -> PathBuf {
    catalog_dir.join(BOOKS_DIRNAME)
}

pub fn is_initialized(catalog_dir: &Path) -> bool {
    db_path(catalog_dir).is_file()
}

pub fn init(catalog_dir: &Path) -> Result<Connection> {
    fs::create_dir_all(catalog_dir).map_err(|source| Error::Io {
        path: catalog_dir.to_path_buf(),
        source,
    })?;
    if is_initialized(catalog_dir) {
        return Err(Error::AlreadyInitialized {
            path: catalog_dir.to_path_buf(),
        });
    }
    let books = books_path(catalog_dir);
    fs::create_dir_all(&books).map_err(|source| Error::Io {
        path: books,
        source,
    })?;
    open(catalog_dir)
}

pub fn open(catalog_dir: &Path) -> Result<Connection> {
    if !catalog_dir.exists() {
        return Err(Error::MissingOnDisk {
            path: catalog_dir.to_path_buf(),
        });
    }
    let path = db_path(catalog_dir);
    let mut conn = Connection::open(&path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    schema::migrations().to_latest(&mut conn)?;
    Ok(conn)
}

pub fn open_existing(catalog_dir: &Path) -> Result<Connection> {
    if !catalog_dir.exists() {
        return Err(Error::MissingOnDisk {
            path: catalog_dir.to_path_buf(),
        });
    }
    if !is_initialized(catalog_dir) {
        return Err(Error::NotACatalog {
            path: catalog_dir.to_path_buf(),
        });
    }
    open(catalog_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn init_creates_db_and_books_dir() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("cat");
        let conn = init(&catalog).unwrap();
        drop(conn);
        assert!(db_path(&catalog).is_file());
        assert!(books_path(&catalog).is_dir());
    }

    #[test]
    fn init_refuses_if_already_initialized() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("cat");
        init(&catalog).unwrap();
        let err = init(&catalog).unwrap_err();
        assert!(matches!(err, Error::AlreadyInitialized { .. }));
    }

    #[test]
    fn init_runs_books_table_migration() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("cat");
        let conn = init(&catalog).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='books'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn open_is_idempotent() {
        let dir = tempdir().unwrap();
        let catalog = dir.path().join("cat");
        init(&catalog).unwrap();
        // Re-open does not blow up.
        let conn = open(&catalog).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='books'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn open_existing_errors_when_not_a_catalog() {
        let dir = tempdir().unwrap();
        let empty = dir.path().join("plain");
        fs::create_dir_all(&empty).unwrap();
        let err = open_existing(&empty).unwrap_err();
        assert!(matches!(err, Error::NotACatalog { .. }));
    }

    #[test]
    fn open_existing_errors_when_missing_on_disk() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("nope");
        let err = open_existing(&missing).unwrap_err();
        assert!(matches!(err, Error::MissingOnDisk { .. }));
    }
}
