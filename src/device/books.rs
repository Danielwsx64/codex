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
    #[error("io error on `{}`: {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

// How a book relates to the device and the catalog. `Both`/`Modified`/`Conflict`
// describe a file on the device (`Modified` = its tracked copy diverged from the
// last push by size/mtime); `LocalOnly` is the catalog side of the same relation
// (a book the device does not have) and is only produced by the library-facing
// query — the device listing never yields it. `Conflict` is real ambiguity (the
// device file matches 2+ catalog books), which the listing never resolves itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    Both,
    DeviceOnly,
    LocalOnly,
    Modified,
    Conflict,
}

impl Presence {
    pub fn as_str(self) -> &'static str {
        match self {
            Presence::Both => "both",
            Presence::DeviceOnly => "device_only",
            Presence::LocalOnly => "local_only",
            Presence::Modified => "modified",
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
    // Format of the matched catalog book, when a single book matched. Lets the
    // renderer show both ends ("azw3 → epub") when the formats differ.
    pub local_format: Option<String>,
}

pub fn list(conn: &Connection, serial: &str, mount: &Path) -> Result<Vec<DeviceBook>> {
    let books = catalog_books::handle_ls(conn)?;
    let index = MatchIndex::build(&books);
    let titles: HashMap<i64, &str> = books.iter().map(|b| (b.id, b.title.as_str())).collect();
    let formats: HashMap<i64, &str> = books.iter().map(|b| (b.id, b.format.as_str())).collect();
    let synced_rows = devices::synced_state(conn, serial)?;
    let synced: HashMap<&Path, &devices::SyncedFile> = synced_rows
        .iter()
        .map(|s| (s.device_path.as_path(), s))
        .collect();

    let mut out = Vec::new();
    for path in super::ebook_files(mount) {
        let device_path = path
            .strip_prefix(mount)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| path.clone());
        let (title, author, format) = read_meta(&path);
        let (presence, matched_book_id) = resolve_presence(
            &device_path,
            &path,
            title.as_deref(),
            author.as_deref(),
            &synced,
            &index,
        )?;
        let matched_title = matched_book_id
            .and_then(|id| titles.get(&id))
            .map(|t| t.to_string());
        let local_format = matched_book_id
            .and_then(|id| formats.get(&id))
            .map(|f| f.to_string());
        out.push(DeviceBook {
            title,
            author,
            format,
            device_path,
            presence,
            matched_book_id,
            matched_title,
            local_format,
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
    abs: &Path,
    title: Option<&str>,
    author: Option<&str>,
    synced: &HashMap<&Path, &devices::SyncedFile>,
    index: &MatchIndex,
) -> Result<(Presence, Option<i64>)> {
    // Exact sync state wins: cdx put this file here, so there's nothing to guess.
    // The fast-path size/mtime check tells `Both` from `Modified`; the file is in
    // the listing so it exists, making `Missing` unreachable (treat it as `Both`).
    if let Some(s) = synced.get(device_path) {
        let presence =
            match super::divergence(abs, s.size, s.mtime).map_err(|source| Error::Io {
                path: abs.to_path_buf(),
                source,
            })? {
                super::Divergence::Modified => Presence::Modified,
                super::Divergence::Current | super::Divergence::Missing => Presence::Both,
            };
        return Ok((presence, Some(s.book_id)));
    }
    let Some(title) = title else {
        return Ok((Presence::DeviceOnly, None));
    };
    match index.lookup(&normalize_key(title, author)) {
        [] => Ok((Presence::DeviceOnly, None)),
        [one] => Ok((Presence::Both, Some(*one))),
        _ => Ok((Presence::Conflict, None)),
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

    // Record sync state matching what is actually on disk, so the size/mtime
    // fast-path reads `Both` (an unchanged tracked file).
    fn record_in_sync(conn: &Connection, mount: &Path, rel: &str, book_id: i64) {
        let abs = mount.join(rel);
        let size = fs::metadata(&abs).unwrap().len() as i64;
        let mtime = super::super::mtime_secs(&abs).unwrap();
        devices::record_sync(conn, "AAA", book_id, Path::new(rel), "h", size, mtime).unwrap();
    }

    #[test]
    fn sync_state_takes_priority_over_metadata() {
        let (_dir, conn) = fresh();
        devices::record_seen(&conn, "AAA").unwrap();
        // The catalog title deliberately does NOT match the device filename.
        let id = add_book(&conn, "Totally Different Title", None);
        let mount = make_mount(&["Dune.txt"]);
        record_in_sync(&conn, mount.path(), "documents/Dune.txt", id);

        let books = list(&conn, "AAA", mount.path()).unwrap();
        assert_eq!(books[0].presence, Presence::Both);
        assert_eq!(books[0].matched_book_id, Some(id));
        assert_eq!(
            books[0].matched_title.as_deref(),
            Some("Totally Different Title")
        );
    }

    #[test]
    fn diverged_tracked_file_is_modified() {
        let (_dir, conn) = fresh();
        devices::record_seen(&conn, "AAA").unwrap();
        let id = add_book(&conn, "Dune", None);
        let mount = make_mount(&["Dune.txt"]); // 1 byte on disk
                                               // Record a size that does not match disk → diverged.
        devices::record_sync(
            &conn,
            "AAA",
            id,
            Path::new("documents/Dune.txt"),
            "h",
            999,
            1,
        )
        .unwrap();

        let books = list(&conn, "AAA", mount.path()).unwrap();
        assert_eq!(books[0].presence, Presence::Modified);
        assert_eq!(books[0].matched_book_id, Some(id));
    }

    #[test]
    fn local_format_set_when_catalog_format_differs() {
        let (_dir, conn) = fresh();
        // Catalog book is 'txt' (add_book default); device file is an epub.
        let id = add_book(&conn, "Dune", None);
        let mount = make_mount(&["Dune.epub"]);

        let books = list(&conn, "AAA", mount.path()).unwrap();
        assert_eq!(books[0].matched_book_id, Some(id));
        assert_eq!(books[0].format, "epub");
        assert_eq!(books[0].local_format.as_deref(), Some("txt"));
    }

    #[test]
    fn empty_device_yields_no_rows() {
        let (_dir, conn) = fresh();
        let mount = make_mount(&[]);
        assert!(list(&conn, "AAA", mount.path()).unwrap().is_empty());
    }
}
