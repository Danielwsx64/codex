use std::path::{Path, PathBuf};

use rusqlite::Connection;
use thiserror::Error;

use crate::catalog::books::{self as catalog_books, ImportResult};
use crate::catalog::devices;
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
    #[error("`{}` is outside the device mount `{}`", .path.display(), .mount.display())]
    OutsideMount { path: PathBuf, mount: PathBuf },
    #[error("`{}` is not a regular file", .path.display())]
    NotAFile { path: PathBuf },
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullOutcome {
    pub book_id: i64,
    pub title: String,
    // Relative to the mount root, e.g. `documents/Dune.epub`.
    pub device_path: PathBuf,
    // `true` for a fresh import, `false` when the content already existed in the
    // catalog (dedup hit) — the sync link is still recorded either way.
    pub imported: bool,
    pub bytes: u64,
}

// Import a book from a connected device into the catalog and record the exact
// sync state. Reuses the `cdx add` pipeline (format detection, metadata, hash
// dedup) so a file whose content is already catalogued is not re-imported; the
// device<->book link is recorded regardless, keyed by (serial, book_id).
pub fn pull(
    conn: &mut Connection,
    catalog_dir: &Path,
    serial: &str,
    mount: &Path,
    arg: &str,
    force: bool,
) -> Result<PullOutcome> {
    let (source, device_path) = resolve_source(mount, arg)?;

    let result = catalog_books::import_single(conn, catalog_dir, &source, force)?;
    let (book_id, imported) = match result {
        ImportResult::New { id, .. } => (id, true),
        ImportResult::Duplicate { existing_id } => (existing_id, false),
    };

    let title = catalog_books::handle_inspect(conn, &book_id.to_string())?.title;

    // Record the device-side file's state (mirrors push): the diff fast-path
    // compares size + mtime against this row, and `--verify` re-hashes.
    let hash = fingerprint::hash_full(&source)?;
    let bytes = std::fs::metadata(&source)
        .map_err(|source_err| Error::Io {
            path: source.clone(),
            source: source_err,
        })?
        .len();
    let mtime = super::mtime_secs(&source).map_err(|source_err| Error::Io {
        path: source.clone(),
        source: source_err,
    })?;
    devices::record_sync(
        conn,
        serial,
        book_id,
        &device_path,
        &hash,
        bytes as i64,
        mtime,
    )?;

    Ok(PullOutcome {
        book_id,
        title,
        device_path,
        imported,
        bytes,
    })
}

// Resolve `arg` (absolute, or relative to the mount as `cdx device books`
// prints it) to an existing regular file inside the mount, plus the mount-
// relative path used for sync state. Canonicalizing both sides confirms the
// file exists and blocks `../` escapes out of the device.
fn resolve_source(mount: &Path, arg: &str) -> Result<(PathBuf, PathBuf)> {
    let candidate = {
        let raw = Path::new(arg);
        if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            mount.join(raw)
        }
    };

    let canonical_mount = mount.canonicalize().map_err(|source| Error::Io {
        path: mount.to_path_buf(),
        source,
    })?;
    let source = candidate.canonicalize().map_err(|source| Error::Io {
        path: candidate.clone(),
        source,
    })?;

    let device_path = match source.strip_prefix(&canonical_mount) {
        Ok(rel) => rel.to_path_buf(),
        Err(_) => {
            return Err(Error::OutsideMount {
                path: source,
                mount: canonical_mount,
            })
        }
    };
    if !source.is_file() {
        return Err(Error::NotAFile { path: source });
    }
    Ok((source, device_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;
    use std::fs;
    use tempfile::{tempdir, TempDir};

    // A fresh catalog plus a separate mount directory with a `documents/` tree.
    fn fresh() -> (TempDir, PathBuf, Connection, TempDir) {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("cat");
        let conn = catalog::init(&cat).unwrap();
        devices::record_seen(&conn, "AAA").unwrap();
        let mount = tempdir().unwrap();
        fs::create_dir_all(mount.path().join("documents")).unwrap();
        (dir, cat, conn, mount)
    }

    fn write_doc(mount: &Path, name: &str, content: &[u8]) -> PathBuf {
        let path = mount.join("documents").join(name);
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn pull_imports_device_only_book_and_records_sync() {
        let (_dir, cat, mut conn, mount) = fresh();
        write_doc(mount.path(), "Dune.txt", b"the spice must flow");

        let outcome = pull(
            &mut conn,
            &cat,
            "AAA",
            mount.path(),
            "documents/Dune.txt",
            false,
        )
        .unwrap();

        assert!(outcome.imported);
        assert_eq!(outcome.device_path, PathBuf::from("documents/Dune.txt"));
        assert_eq!(outcome.bytes, b"the spice must flow".len() as u64);

        // The file landed in the catalog under books/<id>/.
        let book_dir = cat.join("books").join(outcome.book_id.to_string());
        assert!(book_dir.is_dir());

        // Exact sync state recorded against the mount-relative path.
        let synced = devices::synced_paths(&conn, "AAA").unwrap();
        assert_eq!(
            synced.get(&PathBuf::from("documents/Dune.txt")),
            Some(&outcome.book_id)
        );
    }

    #[test]
    fn pull_duplicate_records_sync_without_reimport() {
        let (dir, cat, mut conn, mount) = fresh();
        // Same bytes already in the catalog → dedup by full hash.
        let existing = dir.path().join("Dune.txt");
        fs::write(&existing, b"identical bytes").unwrap();
        let added = catalog_books::handle_add(&mut conn, &cat, &[existing], false);
        let existing_id = added.rows[0].book_id.unwrap();

        write_doc(mount.path(), "Dune.txt", b"identical bytes");
        let outcome = pull(
            &mut conn,
            &cat,
            "AAA",
            mount.path(),
            "documents/Dune.txt",
            false,
        )
        .unwrap();

        assert!(!outcome.imported);
        assert_eq!(outcome.book_id, existing_id);
        let synced = devices::synced_paths(&conn, "AAA").unwrap();
        assert_eq!(
            synced.get(&PathBuf::from("documents/Dune.txt")),
            Some(&existing_id)
        );
    }

    #[test]
    fn pull_force_reimports_even_if_duplicate() {
        let (dir, cat, mut conn, mount) = fresh();
        let existing = dir.path().join("Dune.txt");
        fs::write(&existing, b"identical bytes").unwrap();
        let added = catalog_books::handle_add(&mut conn, &cat, &[existing], false);
        let existing_id = added.rows[0].book_id.unwrap();

        write_doc(mount.path(), "Dune.txt", b"identical bytes");
        let outcome = pull(
            &mut conn,
            &cat,
            "AAA",
            mount.path(),
            "documents/Dune.txt",
            true,
        )
        .unwrap();

        assert!(outcome.imported);
        assert_ne!(outcome.book_id, existing_id);
    }

    #[test]
    fn pull_rejects_path_outside_mount() {
        let (dir, cat, mut conn, mount) = fresh();
        // A real file that lives outside the mount, referenced absolutely.
        let outside = dir.path().join("escape.txt");
        fs::write(&outside, b"x").unwrap();

        let err = pull(
            &mut conn,
            &cat,
            "AAA",
            mount.path(),
            outside.to_str().unwrap(),
            false,
        )
        .unwrap_err();
        assert!(matches!(err, Error::OutsideMount { .. }));
    }

    #[test]
    fn pull_nonexistent_path_errors() {
        let (_dir, cat, mut conn, mount) = fresh();
        let err = pull(
            &mut conn,
            &cat,
            "AAA",
            mount.path(),
            "documents/ghost.txt",
            false,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Io { .. }));
    }

    #[test]
    fn pull_unsupported_format_surfaces_catalog_error() {
        let (_dir, cat, mut conn, mount) = fresh();
        write_doc(mount.path(), "weird.xyz", b"x");
        let err = pull(
            &mut conn,
            &cat,
            "AAA",
            mount.path(),
            "documents/weird.xyz",
            false,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Catalog(_)));
    }

    #[test]
    fn pull_directory_is_not_a_file() {
        let (_dir, cat, mut conn, mount) = fresh();
        fs::create_dir_all(mount.path().join("documents/sub")).unwrap();
        let err = pull(&mut conn, &cat, "AAA", mount.path(), "documents/sub", false).unwrap_err();
        assert!(matches!(err, Error::NotAFile { .. }));
    }
}
