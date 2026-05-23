use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use thiserror::Error;

use crate::catalog::{books_path, BOOKS_DIRNAME};
use crate::import;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Catalog(#[from] crate::catalog::Error),
    #[error(transparent)]
    Import(#[from] import::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error("io error on {}: {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("no book matches `{target}` in the current catalog")]
    NotFound { target: String },
    #[error("multiple books ({count}) match `{target}`; disambiguate by id (e.g. {})", .sample_ids.iter().map(i64::to_string).collect::<Vec<_>>().join(", "))]
    Ambiguous {
        target: String,
        count: usize,
        sample_ids: Vec<i64>,
    },
    #[error("file `{}` is registered but missing from disk", .path.display())]
    FileMissing { path: PathBuf },
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub struct Book {
    pub id: i64,
    pub title: String,
    pub author: Option<String>,
    pub format: String,
    pub file_path: String,
    pub added_at: String,
}

#[derive(Debug)]
pub enum AddStatus {
    Imported,
    Failed { reason: String },
}

#[derive(Debug)]
pub struct AddRow {
    pub source: PathBuf,
    pub status: AddStatus,
    pub book_id: Option<i64>,
    pub stored_path: Option<PathBuf>,
}

#[derive(Debug)]
pub struct AddOutcome {
    pub rows: Vec<AddRow>,
}

impl AddOutcome {
    pub fn any_imported(&self) -> bool {
        self.rows
            .iter()
            .any(|r| matches!(r.status, AddStatus::Imported))
    }

    pub fn any_failed(&self) -> bool {
        self.rows
            .iter()
            .any(|r| matches!(r.status, AddStatus::Failed { .. }))
    }
}

#[derive(Debug)]
pub struct RmOutcome {
    pub book: Book,
    pub kept_at: Option<PathBuf>,
}

pub fn handle_add(conn: &mut Connection, catalog_dir: &Path, paths: &[PathBuf]) -> AddOutcome {
    let mut rows = Vec::with_capacity(paths.len());
    for src in paths {
        let row = import_one(conn, catalog_dir, src);
        rows.push(row);
    }
    AddOutcome { rows }
}

fn import_one(conn: &mut Connection, catalog_dir: &Path, src: &Path) -> AddRow {
    match import_inner(conn, catalog_dir, src) {
        Ok((id, stored)) => AddRow {
            source: src.to_path_buf(),
            status: AddStatus::Imported,
            book_id: Some(id),
            stored_path: Some(stored),
        },
        Err(e) => AddRow {
            source: src.to_path_buf(),
            status: AddStatus::Failed {
                reason: e.to_string(),
            },
            book_id: None,
            stored_path: None,
        },
    }
}

fn import_inner(conn: &mut Connection, catalog_dir: &Path, src: &Path) -> Result<(i64, PathBuf)> {
    let format = import::detect(src)?;
    let metadata = import::extract(src, format).unwrap_or_default();

    let fallback_stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("book");
    let filename = import::build_filename(&metadata, format, fallback_stem);

    let title = metadata
        .title
        .clone()
        .unwrap_or_else(|| fallback_stem.to_string());
    let author = metadata.author.clone();
    let format_label = format.label();

    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO books (title, author, format, file_path) VALUES (?1, ?2, ?3, '')",
        params![title, author, format_label],
    )?;
    let id = tx.last_insert_rowid();

    let book_dir = books_path(catalog_dir).join(id.to_string());
    fs::create_dir_all(&book_dir).map_err(|source| Error::Io {
        path: book_dir.clone(),
        source,
    })?;
    let dest_abs = book_dir.join(&filename);
    if let Err(source) = fs::copy(src, &dest_abs) {
        let _ = fs::remove_dir_all(&book_dir);
        return Err(Error::Io {
            path: src.to_path_buf(),
            source,
        });
    }

    let rel = format!("{BOOKS_DIRNAME}/{id}/{filename}");
    tx.execute(
        "UPDATE books SET file_path = ?1 WHERE id = ?2",
        params![rel, id],
    )?;
    tx.commit()?;
    Ok((id, dest_abs))
}

pub fn handle_ls(conn: &Connection) -> Result<Vec<Book>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, author, format, file_path, added_at
         FROM books
         ORDER BY LOWER(title), id",
    )?;
    let rows = stmt.query_map([], row_to_book)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn handle_inspect(conn: &Connection, target: &str) -> Result<Book> {
    let id = resolve_target(conn, target)?;
    fetch_by_id(conn, id)
}

pub fn handle_rm(
    conn: &mut Connection,
    catalog_dir: &Path,
    target: &str,
    keep: bool,
) -> Result<RmOutcome> {
    let id = resolve_target(conn, target)?;
    let book = fetch_by_id(conn, id)?;
    let abs_src = catalog_dir.join(&book.file_path);
    let book_dir = books_path(catalog_dir).join(id.to_string());

    let kept_at = if keep {
        if !abs_src.exists() {
            return Err(Error::FileMissing { path: abs_src });
        }
        let dest = pick_keep_destination(&book.file_path)?;
        fs::copy(&abs_src, &dest).map_err(|source| Error::Io {
            path: dest.clone(),
            source,
        })?;
        Some(dest)
    } else {
        None
    };

    conn.execute("DELETE FROM books WHERE id = ?1", params![id])?;

    if book_dir.exists() {
        fs::remove_dir_all(&book_dir).map_err(|source| Error::Io {
            path: book_dir,
            source,
        })?;
    }

    Ok(RmOutcome { book, kept_at })
}

fn pick_keep_destination(rel_file_path: &str) -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|source| Error::Io {
        path: PathBuf::from("."),
        source,
    })?;
    let filename = Path::new(rel_file_path)
        .file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("book"));
    let mut candidate = cwd.join(&filename);
    if !candidate.exists() {
        return Ok(candidate);
    }
    let stem = Path::new(&filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("book");
    let ext = Path::new(&filename)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    for n in 1..1000 {
        let name = if ext.is_empty() {
            format!("{stem}.{n}")
        } else {
            format!("{stem}.{n}.{ext}")
        };
        candidate = cwd.join(name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(Error::Io {
        path: cwd,
        source: std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not find a free filename in cwd after 1000 attempts",
        ),
    })
}

fn resolve_target(conn: &Connection, target: &str) -> Result<i64> {
    if let Ok(id) = target.parse::<i64>() {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM books WHERE id = ?1)",
            params![id],
            |r| r.get(0),
        )?;
        if exists {
            return Ok(id);
        }
        return Err(Error::NotFound {
            target: target.to_string(),
        });
    }
    let mut stmt =
        conn.prepare("SELECT id FROM books WHERE title = ?1 COLLATE NOCASE ORDER BY id")?;
    let ids: Vec<i64> = stmt
        .query_map(params![target], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<_>>()?;
    match ids.len() {
        0 => Err(Error::NotFound {
            target: target.to_string(),
        }),
        1 => Ok(ids[0]),
        n => {
            let sample = ids.into_iter().take(5).collect();
            Err(Error::Ambiguous {
                target: target.to_string(),
                count: n,
                sample_ids: sample,
            })
        }
    }
}

fn fetch_by_id(conn: &Connection, id: i64) -> Result<Book> {
    conn.query_row(
        "SELECT id, title, author, format, file_path, added_at
         FROM books
         WHERE id = ?1",
        params![id],
        row_to_book,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Error::NotFound {
            target: id.to_string(),
        },
        other => Error::Sqlite(other),
    })
}

fn row_to_book(row: &rusqlite::Row<'_>) -> rusqlite::Result<Book> {
    Ok(Book {
        id: row.get(0)?,
        title: row.get(1)?,
        author: row.get(2)?,
        format: row.get(3)?,
        file_path: row.get(4)?,
        added_at: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;
    use tempfile::tempdir;

    fn open_fresh(dir: &Path) -> Connection {
        catalog::init(dir).unwrap()
    }

    fn insert_book(conn: &Connection, title: &str, author: Option<&str>) -> i64 {
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES (?1, ?2, 'epub', '')",
            params![title, author],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn resolve_target_by_id() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        let conn = open_fresh(&cat);
        let id = insert_book(&conn, "Book", None);
        assert_eq!(resolve_target(&conn, &id.to_string()).unwrap(), id);
    }

    #[test]
    fn resolve_target_by_unique_title() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        let conn = open_fresh(&cat);
        let id = insert_book(&conn, "Unique Title", None);
        assert_eq!(resolve_target(&conn, "Unique Title").unwrap(), id);
    }

    #[test]
    fn resolve_target_title_case_insensitive() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        let conn = open_fresh(&cat);
        let id = insert_book(&conn, "Hello World", None);
        assert_eq!(resolve_target(&conn, "hello world").unwrap(), id);
    }

    #[test]
    fn resolve_target_ambiguous_lists_ids() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        let conn = open_fresh(&cat);
        let a = insert_book(&conn, "Same", None);
        let b = insert_book(&conn, "Same", None);
        let err = resolve_target(&conn, "Same").unwrap_err();
        match err {
            Error::Ambiguous {
                count, sample_ids, ..
            } => {
                assert_eq!(count, 2);
                assert_eq!(sample_ids, vec![a, b]);
            }
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn resolve_target_unknown_id_is_not_found() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        let conn = open_fresh(&cat);
        let err = resolve_target(&conn, "9999").unwrap_err();
        assert!(matches!(err, Error::NotFound { .. }));
    }

    #[test]
    fn resolve_target_unknown_title_is_not_found() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        let conn = open_fresh(&cat);
        let err = resolve_target(&conn, "Ghost").unwrap_err();
        assert!(matches!(err, Error::NotFound { .. }));
    }

    #[test]
    fn handle_ls_orders_by_title() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        let conn = open_fresh(&cat);
        insert_book(&conn, "Charlie", None);
        insert_book(&conn, "alpha", None);
        insert_book(&conn, "Bravo", None);
        let rows = handle_ls(&conn).unwrap();
        let titles: Vec<_> = rows.iter().map(|b| b.title.as_str()).collect();
        assert_eq!(titles, vec!["alpha", "Bravo", "Charlie"]);
    }
}
