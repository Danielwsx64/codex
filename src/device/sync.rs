use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use thiserror::Error;

use crate::catalog::books as catalog_books;
use crate::catalog::devices::{self, SyncedFile};
use crate::device::books::{self, Presence};
use crate::fingerprint;
use crate::matching::{normalize_key, MatchIndex};

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Catalog(#[from] catalog_books::Error),
    #[error(transparent)]
    Books(#[from] books::Error),
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
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Push,
    Pull,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Push => "push",
            Direction::Pull => "pull",
        }
    }
}

// Why a book is queued to push. `NotOnDevice` is a fresh copy; `Modified`/`Missing`
// re-push a tracked file that changed or vanished on the device (sync never deletes,
// so a gone file is restored, not removed from the catalog).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushReason {
    NotOnDevice,
    Modified,
    Missing,
}

impl PushReason {
    pub fn as_str(self) -> &'static str {
        match self {
            PushReason::NotOnDevice => "not_on_device",
            PushReason::Modified => "modified",
            PushReason::Missing => "missing",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncItem {
    pub direction: Direction,
    // Set for every push item and for pulls that already match a catalog book.
    pub book_id: Option<i64>,
    pub title: String,
    // Mount-relative; empty for a not-on-device push (the path is derived from the
    // catalog filename when the push actually runs).
    pub device_path: PathBuf,
    pub push_reason: Option<PushReason>,
    pub bytes: Option<u64>,
}

// A device file whose title+author matches two or more catalog books. Real
// ambiguity is never resolved automatically — it is surfaced for a manual call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub device_path: PathBuf,
    pub title: String,
    pub candidates: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPlan {
    pub items: Vec<SyncItem>,
    pub conflicts: Vec<Conflict>,
}

impl SyncPlan {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.conflicts.is_empty()
    }
}

// Compute a bidirectional sync plan between the catalog and a connected device.
// Composes the existing presence logic (`device::books::list`, which already does
// metadata matching) with the exact sync state (`device_books`): device-only files
// become pulls, catalog-only books become pushes, and tracked files that changed or
// vanished become re-pushes. `verify` re-hashes files that pass the size+mtime
// fast-path (USB is slow, so the full hash is opt-in).
pub fn diff(conn: &Connection, serial: &str, mount: &Path, verify: bool) -> Result<SyncPlan> {
    let catalog = catalog_books::handle_ls(conn)?;
    let titles: std::collections::HashMap<i64, String> =
        catalog.iter().map(|b| (b.id, b.title.clone())).collect();
    let match_index = MatchIndex::build(&catalog);

    let device_books = books::list(conn, serial, mount)?;
    let synced = devices::synced_state(conn, serial)?;

    // Books the device already holds (by exact sync state or metadata match): never
    // a fresh push. Books with a recorded sync row: handled by the modified/missing
    // pass below, so they are excluded from the not-on-device push pass too.
    let on_device: HashSet<i64> = device_books
        .iter()
        .filter_map(|b| b.matched_book_id)
        .collect();
    let synced_ids: HashSet<i64> = synced.iter().map(|s| s.book_id).collect();

    let mut items: Vec<SyncItem> = Vec::new();
    let mut conflicts: Vec<Conflict> = Vec::new();

    // device -> catalog: pull device-only files, flag match conflicts.
    for db in &device_books {
        match db.presence {
            Presence::DeviceOnly => items.push(SyncItem {
                direction: Direction::Pull,
                book_id: None,
                title: label_for(db.title.as_deref(), &db.device_path),
                device_path: db.device_path.clone(),
                push_reason: None,
                bytes: file_len(&mount.join(&db.device_path)),
            }),
            Presence::Conflict => {
                let key = normalize_key(db.title.as_deref().unwrap_or(""), db.author.as_deref());
                conflicts.push(Conflict {
                    device_path: db.device_path.clone(),
                    title: label_for(db.title.as_deref(), &db.device_path),
                    candidates: match_index.lookup(&key).to_vec(),
                });
            }
            Presence::Both => {}
        }
    }

    // exact sync state: re-push files that changed (modified) or vanished (missing).
    for s in &synced {
        let abs = mount.join(&s.device_path);
        let Some(reason) = classify(&abs, s, verify)? else {
            continue; // still in sync, nothing to do
        };
        items.push(SyncItem {
            direction: Direction::Push,
            book_id: Some(s.book_id),
            title: titles
                .get(&s.book_id)
                .cloned()
                .unwrap_or_else(|| s.device_path.display().to_string()),
            device_path: s.device_path.clone(),
            push_reason: Some(reason),
            bytes: Some(s.size as u64),
        });
    }

    // catalog -> device: push books the device does not have at all.
    for book in &catalog {
        if on_device.contains(&book.id) || synced_ids.contains(&book.id) {
            continue;
        }
        items.push(SyncItem {
            direction: Direction::Push,
            book_id: Some(book.id),
            title: book.title.clone(),
            device_path: PathBuf::new(),
            push_reason: Some(PushReason::NotOnDevice),
            bytes: None,
        });
    }

    sort_items(&mut items);
    conflicts.sort_by(|a, b| a.device_path.cmp(&b.device_path));
    Ok(SyncPlan { items, conflicts })
}

// Whether a tracked device file diverged from its recorded state. `None` means it
// is still in sync. The fast-path compares size then mtime (FAT mtime is 2s-coarse
// but push records exactly what it wrote, so an untouched file matches); `--verify`
// re-hashes only when the fast-path already looks unchanged.
fn classify(abs: &Path, s: &SyncedFile, verify: bool) -> Result<Option<PushReason>> {
    let meta = match std::fs::metadata(abs) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Some(PushReason::Missing));
        }
        Err(source) => {
            return Err(Error::Io {
                path: abs.to_path_buf(),
                source,
            })
        }
    };
    if meta.len() as i64 != s.size {
        return Ok(Some(PushReason::Modified));
    }
    let mtime = super::mtime_secs(abs).map_err(|source| Error::Io {
        path: abs.to_path_buf(),
        source,
    })?;
    if mtime != s.mtime {
        return Ok(Some(PushReason::Modified));
    }
    if verify && fingerprint::hash_full(abs)? != s.hash {
        return Ok(Some(PushReason::Modified));
    }
    Ok(None)
}

fn label_for(title: Option<&str>, device_path: &Path) -> String {
    title
        .map(str::to_string)
        .unwrap_or_else(|| device_path.display().to_string())
}

fn file_len(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| m.len())
}

// Stable order for humans and snapshots: pulls first, then pushes, each by title
// then path.
fn sort_items(items: &mut [SyncItem]) {
    items.sort_by(|a, b| {
        let rank = |d: Direction| match d {
            Direction::Pull => 0,
            Direction::Push => 1,
        };
        rank(a.direction)
            .cmp(&rank(b.direction))
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
            .then_with(|| a.device_path.cmp(&b.device_path))
    });
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

    fn push_items(plan: &SyncPlan) -> Vec<&SyncItem> {
        plan.items
            .iter()
            .filter(|i| i.direction == Direction::Push)
            .collect()
    }

    fn pull_items(plan: &SyncPlan) -> Vec<&SyncItem> {
        plan.items
            .iter()
            .filter(|i| i.direction == Direction::Pull)
            .collect()
    }

    #[test]
    fn catalog_only_book_is_a_push() {
        let (_dir, conn) = fresh();
        add_book(&conn, "Dune", None);
        let mount = make_mount(&[]);

        let plan = diff(&conn, "AAA", mount.path(), false).unwrap();
        let pushes = push_items(&plan);
        assert_eq!(pushes.len(), 1);
        assert_eq!(pushes[0].title, "Dune");
        assert_eq!(pushes[0].push_reason, Some(PushReason::NotOnDevice));
        assert!(pull_items(&plan).is_empty());
    }

    #[test]
    fn device_only_file_is_a_pull() {
        let (_dir, conn) = fresh();
        let mount = make_mount(&["Strange.txt"]);

        let plan = diff(&conn, "AAA", mount.path(), false).unwrap();
        let pulls = pull_items(&plan);
        assert_eq!(pulls.len(), 1);
        assert_eq!(pulls[0].device_path, PathBuf::from("documents/Strange.txt"));
        assert!(push_items(&plan).is_empty());
    }

    #[test]
    fn metadata_match_on_both_ends_is_in_sync() {
        let (_dir, conn) = fresh();
        add_book(&conn, "Dune", None);
        let mount = make_mount(&["Dune.txt"]);

        let plan = diff(&conn, "AAA", mount.path(), false).unwrap();
        assert!(plan.is_empty(), "matched book on both ends needs no action");
    }

    #[test]
    fn two_catalog_matches_is_a_conflict() {
        let (_dir, conn) = fresh();
        let a = add_book(&conn, "Dune", None);
        let b = add_book(&conn, "Dune", None);
        let mount = make_mount(&["Dune.txt"]);

        let plan = diff(&conn, "AAA", mount.path(), false).unwrap();
        assert_eq!(plan.conflicts.len(), 1);
        let mut candidates = plan.conflicts[0].candidates.clone();
        candidates.sort_unstable();
        assert_eq!(candidates, vec![a, b]);
        // A conflicted file is never queued as a plain pull.
        assert!(pull_items(&plan).is_empty());
    }

    #[test]
    fn tracked_file_gone_from_device_is_missing() {
        let (_dir, conn) = fresh();
        devices::record_seen(&conn, "AAA").unwrap();
        let id = add_book(&conn, "Dune", None);
        // Sync state points at a file that is not on the (empty) mount.
        devices::record_sync(
            &conn,
            "AAA",
            id,
            Path::new("documents/Dune.txt"),
            "hash",
            1,
            1,
        )
        .unwrap();
        let mount = make_mount(&[]);

        let plan = diff(&conn, "AAA", mount.path(), false).unwrap();
        let pushes = push_items(&plan);
        assert_eq!(pushes.len(), 1);
        assert_eq!(pushes[0].book_id, Some(id));
        assert_eq!(pushes[0].push_reason, Some(PushReason::Missing));
    }

    #[test]
    fn tracked_file_changed_size_is_modified() {
        let (_dir, conn) = fresh();
        devices::record_seen(&conn, "AAA").unwrap();
        let id = add_book(&conn, "Dune", None);
        let mount = make_mount(&["Dune.txt"]); // one byte on disk
        let mtime = super::super::mtime_secs(&mount.path().join("documents/Dune.txt")).unwrap();
        // Record a different size than what is on disk.
        devices::record_sync(
            &conn,
            "AAA",
            id,
            Path::new("documents/Dune.txt"),
            "hash",
            999,
            mtime,
        )
        .unwrap();

        let plan = diff(&conn, "AAA", mount.path(), false).unwrap();
        let pushes = push_items(&plan);
        assert_eq!(pushes.len(), 1);
        assert_eq!(pushes[0].push_reason, Some(PushReason::Modified));
    }

    #[test]
    fn unchanged_fast_path_is_in_sync_but_verify_catches_hash_drift() {
        let (_dir, conn) = fresh();
        devices::record_seen(&conn, "AAA").unwrap();
        let id = add_book(&conn, "Dune", None);
        let mount = make_mount(&["Dune.txt"]);
        let path = mount.path().join("documents/Dune.txt");
        let size = fs::metadata(&path).unwrap().len() as i64;
        let mtime = super::super::mtime_secs(&path).unwrap();
        // Size+mtime match disk, but the recorded hash does not.
        devices::record_sync(
            &conn,
            "AAA",
            id,
            Path::new("documents/Dune.txt"),
            "stale-hash",
            size,
            mtime,
        )
        .unwrap();

        // Fast-path trusts size+mtime: looks in sync.
        let fast = diff(&conn, "AAA", mount.path(), false).unwrap();
        assert!(push_items(&fast).is_empty());

        // --verify re-hashes and finds the drift.
        let verified = diff(&conn, "AAA", mount.path(), true).unwrap();
        let pushes = push_items(&verified);
        assert_eq!(pushes.len(), 1);
        assert_eq!(pushes[0].push_reason, Some(PushReason::Modified));
    }
}
