use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use thiserror::Error;

use crate::catalog::{books_path, tags, BOOKS_DIRNAME};
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
    #[error("invalid value for {field}: {reason}")]
    Validation { field: &'static str, reason: String },
    #[error("failed to rename `{}` to `{}`: {source}", .from.display(), .to.display())]
    RenameFailed {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("book id {id} has unknown format `{format}`")]
    UnknownFormat { id: i64, format: String },
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedStatus {
    Pending,
    Synced,
    Unsupported,
}

impl EmbedStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            EmbedStatus::Pending => "pending",
            EmbedStatus::Synced => "synced",
            EmbedStatus::Unsupported => "unsupported",
        }
    }

    pub fn parse_label(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(EmbedStatus::Pending),
            "synced" => Some(EmbedStatus::Synced),
            "unsupported" => Some(EmbedStatus::Unsupported),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Book {
    pub id: i64,
    pub title: String,
    pub author: Option<String>,
    pub format: String,
    pub file_path: String,
    pub added_at: String,
    pub description: Option<String>,
    pub series_name: Option<String>,
    pub series_index: Option<f64>,
    pub rating: Option<u8>,
    pub isbn: Option<String>,
    pub publisher: Option<String>,
    pub language: Option<String>,
    pub published_date: Option<String>,
    pub tags: Vec<String>,
    pub embed_status: EmbedStatus,
    pub embed_synced_at: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct BookUpdate {
    pub title: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub series_name: Option<String>,
    pub series_index: Option<f64>,
    pub rating: Option<u8>,
    pub isbn: Option<String>,
    pub publisher: Option<String>,
    pub language: Option<String>,
    pub published_date: Option<String>,
    pub tags: Vec<String>,
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
        "INSERT INTO books (
            title, author, format, file_path,
            description, series_name, series_index,
            isbn, publisher, language, published_date
         ) VALUES (?1, ?2, ?3, '', ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            title,
            author,
            format_label,
            metadata.description,
            metadata.series_name,
            metadata.series_index,
            metadata.isbn,
            metadata.publisher,
            metadata.language,
            metadata.published_date,
        ],
    )?;
    let id = tx.last_insert_rowid();

    if !metadata.tags.is_empty() {
        tags::sync(&tx, id, &metadata.tags)?;
    }

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
        "SELECT id, title, author, format, file_path, added_at,
                description, series_name, series_index, rating,
                isbn, publisher, language, published_date,
                embed_status, embed_synced_at
         FROM books
         ORDER BY LOWER(title), id",
    )?;
    let rows: rusqlite::Result<Vec<Book>> = stmt.query_map([], row_to_book)?.collect();
    let mut books = rows?;
    for b in &mut books {
        b.tags = tags::fetch_for_book(conn, b.id)?;
    }
    Ok(books)
}

pub fn fetch_pending(conn: &Connection) -> Result<Vec<Book>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, author, format, file_path, added_at,
                description, series_name, series_index, rating,
                isbn, publisher, language, published_date,
                embed_status, embed_synced_at
         FROM books
         WHERE embed_status = 'pending'
         ORDER BY id",
    )?;
    let rows: rusqlite::Result<Vec<Book>> = stmt.query_map([], row_to_book)?.collect();
    let mut books = rows?;
    for b in &mut books {
        b.tags = tags::fetch_for_book(conn, b.id)?;
    }
    Ok(books)
}

pub fn mark_embed_synced(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE books SET embed_status = 'synced', embed_synced_at = datetime('now') WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

pub fn mark_embed_unsupported(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE books SET embed_status = 'unsupported', embed_synced_at = NULL WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

pub fn handle_inspect(conn: &Connection, target: &str) -> Result<Book> {
    let id = resolve_target(conn, target)?;
    fetch_by_id(conn, id)
}

pub fn handle_update(
    conn: &mut Connection,
    catalog_dir: &Path,
    id: i64,
    update: BookUpdate,
) -> Result<Book> {
    let title = update.title.trim();
    if title.is_empty() {
        return Err(Error::Validation {
            field: "title",
            reason: "must not be empty".to_string(),
        });
    }
    if let Some(r) = update.rating {
        if r > 5 {
            return Err(Error::Validation {
                field: "rating",
                reason: format!("must be between 0 and 5 (got {r})"),
            });
        }
    }

    let current = fetch_by_id(conn, id)?;
    let format =
        import::Format::parse_label(&current.format).ok_or_else(|| Error::UnknownFormat {
            id,
            format: current.format.clone(),
        })?;

    let author = update
        .author
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let tx = conn.transaction()?;
    // Any metadata edit makes the file embed stale, so reset tracking back to
    // pending — `cdx embed sync` (or the TUI) will re-embed and flip it.
    tx.execute(
        "UPDATE books
         SET title = ?1, author = ?2, description = ?3,
             series_name = ?4, series_index = ?5, rating = ?6,
             isbn = ?7, publisher = ?8, language = ?9, published_date = ?10,
             embed_status = 'pending', embed_synced_at = NULL
         WHERE id = ?11",
        params![
            title,
            author,
            normalize_opt(&update.description),
            normalize_opt(&update.series_name),
            update.series_index,
            update.rating.map(|r| r as i64),
            normalize_opt(&update.isbn),
            normalize_opt(&update.publisher),
            normalize_opt(&update.language),
            normalize_opt(&update.published_date),
            id,
        ],
    )?;
    tags::sync(&tx, id, &update.tags)?;

    let new_meta = import::Metadata {
        title: Some(title.to_string()),
        author: author.clone(),
        ..import::Metadata::default()
    };
    let fallback = Path::new(&current.file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("book");
    let new_filename = import::build_filename(&new_meta, format, fallback);

    let old_filename = Path::new(&current.file_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if new_filename != old_filename {
        let old_abs = catalog_dir.join(&current.file_path);
        let new_rel = format!("{BOOKS_DIRNAME}/{id}/{new_filename}");
        let new_abs = catalog_dir.join(&new_rel);
        if old_abs.exists() {
            fs::rename(&old_abs, &new_abs).map_err(|source| Error::RenameFailed {
                from: old_abs,
                to: new_abs,
                source,
            })?;
            tx.execute(
                "UPDATE books SET file_path = ?1 WHERE id = ?2",
                params![new_rel, id],
            )?;
        }
    }

    tx.commit()?;
    fetch_by_id(conn, id)
}

fn normalize_opt(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
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

pub(crate) fn resolve_target(conn: &Connection, target: &str) -> Result<i64> {
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
    let mut book = conn
        .query_row(
            "SELECT id, title, author, format, file_path, added_at,
                    description, series_name, series_index, rating,
                    isbn, publisher, language, published_date,
                    embed_status, embed_synced_at
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
        })?;
    book.tags = tags::fetch_for_book(conn, id)?;
    Ok(book)
}

fn row_to_book(row: &rusqlite::Row<'_>) -> rusqlite::Result<Book> {
    let embed_status_raw: String = row.get(14)?;
    let embed_status = EmbedStatus::parse_label(&embed_status_raw).unwrap_or(EmbedStatus::Pending);
    Ok(Book {
        id: row.get(0)?,
        title: row.get(1)?,
        author: row.get(2)?,
        format: row.get(3)?,
        file_path: row.get(4)?,
        added_at: row.get(5)?,
        description: row.get(6)?,
        series_name: row.get(7)?,
        series_index: row.get(8)?,
        rating: row.get::<_, Option<i64>>(9)?.map(|n| n.clamp(0, 5) as u8),
        isbn: row.get(10)?,
        publisher: row.get(11)?,
        language: row.get(12)?,
        published_date: row.get(13)?,
        tags: Vec::new(),
        embed_status,
        embed_synced_at: row.get(15)?,
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

    fn seed_book_with_file(catalog_dir: &Path, title: &str, author: &str) -> (i64, PathBuf) {
        let conn = catalog::open_existing(catalog_dir).unwrap();
        let filename = format!(
            "{}_-_{}.epub",
            author.replace(' ', "_"),
            title.replace(' ', "_")
        );
        let rel = format!("{BOOKS_DIRNAME}/seed/{filename}");
        // dummy id will be assigned by AUTOINCREMENT
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES (?1, ?2, 'epub', ?3)",
            params![title, author, rel],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        // move into id-based directory and rewrite file_path to match cdx layout
        let final_rel = format!("{BOOKS_DIRNAME}/{id}/{filename}");
        let abs = catalog_dir.join(&final_rel);
        fs::create_dir_all(abs.parent().unwrap()).unwrap();
        fs::write(&abs, b"epub-stub").unwrap();
        conn.execute(
            "UPDATE books SET file_path = ?1 WHERE id = ?2",
            params![final_rel, id],
        )
        .unwrap();
        (id, abs)
    }

    #[test]
    fn handle_update_persists_all_fields_and_tags() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        let conn = open_fresh(&cat);
        drop(conn);
        let (id, _) = seed_book_with_file(&cat, "Old Title", "Old Author");
        let mut conn = catalog::open_existing(&cat).unwrap();

        let update = BookUpdate {
            title: "New Title".into(),
            author: Some("New Author".into()),
            description: Some("Long desc".into()),
            series_name: Some("Foundation".into()),
            series_index: Some(2.0),
            rating: Some(4),
            isbn: Some("9780000000000".into()),
            publisher: Some("Acme".into()),
            language: Some("en".into()),
            published_date: Some("2025-01-01".into()),
            tags: vec!["sci-fi".into(), "classic".into()],
        };
        let book = handle_update(&mut conn, &cat, id, update).unwrap();
        assert_eq!(book.title, "New Title");
        assert_eq!(book.author.as_deref(), Some("New Author"));
        assert_eq!(book.description.as_deref(), Some("Long desc"));
        assert_eq!(book.series_name.as_deref(), Some("Foundation"));
        assert_eq!(book.series_index, Some(2.0));
        assert_eq!(book.rating, Some(4));
        assert_eq!(book.isbn.as_deref(), Some("9780000000000"));
        assert_eq!(book.publisher.as_deref(), Some("Acme"));
        assert_eq!(book.language.as_deref(), Some("en"));
        assert_eq!(book.published_date.as_deref(), Some("2025-01-01"));
        assert_eq!(book.tags, vec!["classic", "sci-fi"]); // ORDER BY LOWER(name)
        assert_eq!(book.embed_status, EmbedStatus::Pending);
        assert!(book.embed_synced_at.is_none());
    }

    #[test]
    fn handle_update_resets_synced_to_pending() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        let conn = open_fresh(&cat);
        drop(conn);
        let (id, _) = seed_book_with_file(&cat, "T", "A");
        let mut conn = catalog::open_existing(&cat).unwrap();
        mark_embed_synced(&conn, id).unwrap();
        let before = fetch_by_id(&conn, id).unwrap();
        assert_eq!(before.embed_status, EmbedStatus::Synced);
        assert!(before.embed_synced_at.is_some());

        let update = BookUpdate {
            title: "T".into(),
            ..BookUpdate::default()
        };
        let after = handle_update(&mut conn, &cat, id, update).unwrap();
        assert_eq!(after.embed_status, EmbedStatus::Pending);
        assert!(after.embed_synced_at.is_none());
    }

    #[test]
    fn mark_embed_synced_sets_status_and_timestamp() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        let conn = open_fresh(&cat);
        let id = insert_book(&conn, "Book", None);
        mark_embed_synced(&conn, id).unwrap();
        let book = fetch_by_id(&conn, id).unwrap();
        assert_eq!(book.embed_status, EmbedStatus::Synced);
        assert!(book.embed_synced_at.is_some());
    }

    #[test]
    fn mark_embed_unsupported_sets_status_without_timestamp() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        let conn = open_fresh(&cat);
        let id = insert_book(&conn, "Book", None);
        mark_embed_unsupported(&conn, id).unwrap();
        let book = fetch_by_id(&conn, id).unwrap();
        assert_eq!(book.embed_status, EmbedStatus::Unsupported);
        assert!(book.embed_synced_at.is_none());
    }

    #[test]
    fn fetch_pending_filters_by_status() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        let conn = open_fresh(&cat);
        let a = insert_book(&conn, "A", None);
        let b = insert_book(&conn, "B", None);
        let c = insert_book(&conn, "C", None);
        mark_embed_synced(&conn, a).unwrap();
        mark_embed_unsupported(&conn, b).unwrap();
        // c stays pending (default).
        let rows = fetch_pending(&conn).unwrap();
        let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![c]);
    }

    #[test]
    fn fresh_book_starts_pending() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        let conn = open_fresh(&cat);
        let id = insert_book(&conn, "Book", None);
        let book = fetch_by_id(&conn, id).unwrap();
        assert_eq!(book.embed_status, EmbedStatus::Pending);
        assert!(book.embed_synced_at.is_none());
    }

    #[test]
    fn handle_update_renames_file_when_title_changes() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        let conn = open_fresh(&cat);
        drop(conn);
        let (id, old_abs) = seed_book_with_file(&cat, "Old", "Author");
        assert!(old_abs.exists());
        let mut conn = catalog::open_existing(&cat).unwrap();

        let update = BookUpdate {
            title: "Brand New".into(),
            author: Some("Author".into()),
            ..BookUpdate::default()
        };
        let book = handle_update(&mut conn, &cat, id, update).unwrap();
        assert!(!old_abs.exists(), "old path must be gone after rename");
        let new_abs = cat.join(&book.file_path);
        assert!(
            new_abs.exists(),
            "new path must exist: {}",
            new_abs.display()
        );
        assert!(book.file_path.ends_with("Author_-_Brand_New.epub"));
    }

    #[test]
    fn handle_update_rejects_empty_title() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        let conn = open_fresh(&cat);
        drop(conn);
        let (id, _) = seed_book_with_file(&cat, "T", "A");
        let mut conn = catalog::open_existing(&cat).unwrap();

        let update = BookUpdate {
            title: "   ".into(),
            ..BookUpdate::default()
        };
        let err = handle_update(&mut conn, &cat, id, update).unwrap_err();
        assert!(matches!(err, Error::Validation { field: "title", .. }));
    }

    #[test]
    fn handle_update_rejects_rating_above_5() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        let conn = open_fresh(&cat);
        drop(conn);
        let (id, _) = seed_book_with_file(&cat, "T", "A");
        let mut conn = catalog::open_existing(&cat).unwrap();

        let update = BookUpdate {
            title: "Valid".into(),
            rating: Some(9),
            ..BookUpdate::default()
        };
        let err = handle_update(&mut conn, &cat, id, update).unwrap_err();
        assert!(matches!(
            err,
            Error::Validation {
                field: "rating",
                ..
            }
        ));
    }

    #[test]
    fn handle_update_skips_rename_when_file_missing() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        let conn = open_fresh(&cat);
        drop(conn);
        let (id, old_abs) = seed_book_with_file(&cat, "Old", "Author");
        fs::remove_file(&old_abs).unwrap();
        let mut conn = catalog::open_existing(&cat).unwrap();

        let update = BookUpdate {
            title: "Whatever".into(),
            author: Some("Author".into()),
            ..BookUpdate::default()
        };
        let book = handle_update(&mut conn, &cat, id, update).unwrap();
        assert_eq!(book.title, "Whatever");
        // file_path was not touched because the file is gone.
        assert!(book.file_path.contains("Old"));
    }
}
