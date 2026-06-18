use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use thiserror::Error;

use crate::catalog::{books as catalog_books, devices};
use crate::import;
use crate::matching::{normalize_key, MatchIndex};

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Catalog(#[from] catalog_books::Error),
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

// Whether a file on the device is also in the catalog. `Conflict` is real
// ambiguity — the device file matches 2+ catalog books — which the listing
// never resolves on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    Both,
    DeviceOnly,
    Conflict,
}

impl Presence {
    pub fn as_str(self) -> &'static str {
        match self {
            Presence::Both => "both",
            Presence::DeviceOnly => "device_only",
            Presence::Conflict => "conflict",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceBook {
    pub title: Option<String>,
    pub author: Option<String>,
    pub format: String,
    // Relative to the mount root, e.g. `documents/Dune.epub`.
    pub device_path: PathBuf,
    pub presence: Presence,
    pub matched_book_id: Option<i64>,
    pub matched_title: Option<String>,
}

pub fn list(conn: &Connection, serial: &str, mount: &Path) -> Result<Vec<DeviceBook>> {
    let books = catalog_books::handle_ls(conn)?;
    let index = MatchIndex::build(&books);
    let titles: HashMap<i64, &str> = books.iter().map(|b| (b.id, b.title.as_str())).collect();
    let synced = devices::synced_paths(conn, serial)?;

    let mut out = Vec::new();
    for path in super::ebook_files(mount) {
        let device_path = path
            .strip_prefix(mount)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| path.clone());
        let (title, author, format) = read_meta(&path);
        let (presence, matched_book_id) = resolve_presence(
            &device_path,
            title.as_deref(),
            author.as_deref(),
            &synced,
            &index,
        );
        let matched_title = matched_book_id
            .and_then(|id| titles.get(&id))
            .map(|t| t.to_string());
        out.push(DeviceBook {
            title,
            author,
            format,
            device_path,
            presence,
            matched_book_id,
            matched_title,
        });
    }
    // Stable order for humans and snapshots; title-less files sort last.
    out.sort_by(|a, b| {
        let at = a.title.as_deref().unwrap_or("").to_lowercase();
        let bt = b.title.as_deref().unwrap_or("").to_lowercase();
        at.cmp(&bt).then_with(|| a.device_path.cmp(&b.device_path))
    });
    Ok(out)
}

// Read title/author from the file itself (not just the name). A file that can't
// be parsed must never abort the whole listing, so any error falls back to the
// filename stem as title with no author.
fn read_meta(path: &Path) -> (Option<String>, Option<String>, String) {
    let format = match import::detect(path) {
        Ok(format) => format,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "cannot determine device file format");
            return (stem_title(path), None, ext_label(path));
        }
    };
    match import::extract(path, format) {
        Ok(meta) => {
            let title = meta
                .title
                .filter(|s| !s.trim().is_empty())
                .or_else(|| stem_title(path));
            let author = meta.author.filter(|s| !s.trim().is_empty());
            (title, author, format.label().to_string())
        }
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "cannot read device file metadata; using filename");
            (stem_title(path), None, format.label().to_string())
        }
    }
}

fn resolve_presence(
    device_path: &Path,
    title: Option<&str>,
    author: Option<&str>,
    synced: &HashMap<PathBuf, i64>,
    index: &MatchIndex,
) -> (Presence, Option<i64>) {
    // Exact sync state wins: cdx put this file here, so there's nothing to guess.
    if let Some(&id) = synced.get(device_path) {
        return (Presence::Both, Some(id));
    }
    let Some(title) = title else {
        return (Presence::DeviceOnly, None);
    };
    match index.lookup(&normalize_key(title, author)) {
        [] => (Presence::DeviceOnly, None),
        [one] => (Presence::Both, Some(*one)),
        _ => (Presence::Conflict, None),
    }
}

fn stem_title(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

fn ext_label(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| "?".to_string())
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

    fn add_book(conn: &Connection, title: &str, author: Option<&str>) -> i64 {
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES (?1, ?2, 'txt', '')",
            params![title, author],
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
    fn matched_book_is_both_unmatched_is_device_only() {
        let (_dir, conn) = fresh();
        let dune = add_book(&conn, "Dune", None);
        let mount = make_mount(&["Dune.txt", "Strange.txt"]);

        let books = list(&conn, "AAA", mount.path()).unwrap();
        assert_eq!(books.len(), 2);
        // Sorted by title: "Dune" then "Strange".
        assert_eq!(books[0].title.as_deref(), Some("Dune"));
        assert_eq!(books[0].presence, Presence::Both);
        assert_eq!(books[0].matched_book_id, Some(dune));
        assert_eq!(books[0].matched_title.as_deref(), Some("Dune"));
        assert_eq!(books[0].device_path, PathBuf::from("documents/Dune.txt"));

        assert_eq!(books[1].title.as_deref(), Some("Strange"));
        assert_eq!(books[1].presence, Presence::DeviceOnly);
        assert_eq!(books[1].matched_book_id, None);
    }

    #[test]
    fn two_catalog_matches_is_a_conflict() {
        let (_dir, conn) = fresh();
        add_book(&conn, "Dune", None);
        add_book(&conn, "Dune", None);
        let mount = make_mount(&["Dune.txt"]);

        let books = list(&conn, "AAA", mount.path()).unwrap();
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].presence, Presence::Conflict);
        assert_eq!(books[0].matched_book_id, None);
    }

    #[test]
    fn sync_state_takes_priority_over_metadata() {
        let (_dir, conn) = fresh();
        devices::record_seen(&conn, "AAA").unwrap();
        // The catalog title deliberately does NOT match the device filename.
        let id = add_book(&conn, "Totally Different Title", None);
        conn.execute(
            "INSERT INTO device_books (device_serial, book_id, device_path, hash, size, mtime)
             VALUES ('AAA', ?1, 'documents/Dune.txt', 'h', 1, 1)",
            params![id],
        )
        .unwrap();
        let mount = make_mount(&["Dune.txt"]);

        let books = list(&conn, "AAA", mount.path()).unwrap();
        assert_eq!(books[0].presence, Presence::Both);
        assert_eq!(books[0].matched_book_id, Some(id));
        assert_eq!(
            books[0].matched_title.as_deref(),
            Some("Totally Different Title")
        );
    }

    #[test]
    fn empty_device_yields_no_rows() {
        let (_dir, conn) = fresh();
        let mount = make_mount(&[]);
        assert!(list(&conn, "AAA", mount.path()).unwrap().is_empty());
    }
}
