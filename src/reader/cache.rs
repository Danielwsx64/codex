use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::import::Format;
use crate::reader::Chapter;

// Bump whenever the serialized shape of `Chapter` / `StyledLine` changes;
// a mismatch silently invalidates every existing cache file.
const CACHE_SCHEMA_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct CacheEnvelope {
    schema_version: u32,
    render_width: usize,
    source_mtime_ns: u64,
    source_size: u64,
    chapters: Vec<Chapter>,
}

pub(crate) fn is_cacheable(format: Format) -> bool {
    // TXT/MD parse about as fast as the cached JSON would deserialize, so
    // caching them is pure disk churn.
    matches!(
        format,
        Format::Pdf | Format::Epub | Format::Mobi | Format::Azw3
    )
}

pub(crate) fn load(
    catalog_dir: &Path,
    book_id: i64,
    source_path: &Path,
    render_width: usize,
) -> Option<Vec<Chapter>> {
    let root = cache_root()?;
    load_in(&root, catalog_dir, book_id, source_path, render_width)
}

pub(crate) fn store(
    catalog_dir: &Path,
    book_id: i64,
    source_path: &Path,
    render_width: usize,
    chapters: &[Chapter],
) {
    let Some(root) = cache_root() else {
        return;
    };
    store_in(
        &root,
        catalog_dir,
        book_id,
        source_path,
        render_width,
        chapters,
    );
}

fn cache_root() -> Option<PathBuf> {
    // `CDX_CACHE_DIR` is the cache-side sibling of `--data-dir`: integration
    // tests set it so an in-process `reader::open` never touches the user's
    // real XDG cache.
    if let Some(dir) = std::env::var_os("CDX_CACHE_DIR") {
        return Some(PathBuf::from(dir));
    }
    ProjectDirs::from("", "", "cdx").map(|p| p.cache_dir().to_path_buf())
}

fn load_in(
    cache_root: &Path,
    catalog_dir: &Path,
    book_id: i64,
    source_path: &Path,
    render_width: usize,
) -> Option<Vec<Chapter>> {
    let (source_mtime_ns, source_size) = source_stamp(source_path)?;
    let path = entry_path(cache_root, catalog_dir, book_id);
    let bytes = fs::read(&path).ok()?;
    let envelope: CacheEnvelope = match serde_json::from_slice(&bytes) {
        Ok(envelope) => envelope,
        Err(err) => {
            tracing::debug!(path = %path.display(), error = %err, "ignoring unreadable reader cache");
            return None;
        }
    };
    let hit = envelope.schema_version == CACHE_SCHEMA_VERSION
        && envelope.render_width == render_width
        && envelope.source_mtime_ns == source_mtime_ns
        && envelope.source_size == source_size;
    hit.then_some(envelope.chapters)
}

fn store_in(
    cache_root: &Path,
    catalog_dir: &Path,
    book_id: i64,
    source_path: &Path,
    render_width: usize,
    chapters: &[Chapter],
) {
    // Cache writes are best-effort: any failure is logged and swallowed so a
    // cache problem can never break opening a book.
    let Some((source_mtime_ns, source_size)) = source_stamp(source_path) else {
        return;
    };
    let envelope = CacheEnvelope {
        schema_version: CACHE_SCHEMA_VERSION,
        render_width,
        source_mtime_ns,
        source_size,
        chapters: chapters.to_vec(),
    };
    let path = entry_path(cache_root, catalog_dir, book_id);
    let bytes = match serde_json::to_vec(&envelope) {
        Ok(bytes) => bytes,
        Err(err) => {
            tracing::warn!(path = %path.display(), error = %err, "failed to serialize reader cache");
            return;
        }
    };
    if let Err(err) = write_atomic(&path, &bytes) {
        tracing::warn!(path = %path.display(), error = %err, "failed to write reader cache");
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

fn entry_path(cache_root: &Path, catalog_dir: &Path, book_id: i64) -> PathBuf {
    cache_root
        .join(catalog_key(catalog_dir))
        .join(format!("{book_id}.json"))
}

fn catalog_key(catalog_dir: &Path) -> String {
    // Canonicalize so relative and absolute spellings of the same catalog
    // share one cache bucket; fall back to the raw path if the dir vanished.
    let canonical = catalog_dir
        .canonicalize()
        .unwrap_or_else(|_| catalog_dir.to_path_buf());
    let digest = Sha256::digest(canonical.to_string_lossy().as_bytes());
    let mut key = hex(digest);
    key.truncate(16);
    key
}

fn source_stamp(path: &Path) -> Option<(u64, u64)> {
    let meta = fs::metadata(path).ok()?;
    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    Some((mtime_ns, meta.len()))
}

fn hex(digest: impl AsRef<[u8]>) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(64);
    for b in digest.as_ref() {
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    const WIDTH: usize = 10_000;

    fn sample_chapters() -> Vec<Chapter> {
        vec![Chapter::from_text(
            "Chapter 1".into(),
            "hello\nworld".into(),
        )]
    }

    fn setup() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
        let cache_root = tempdir().expect("tempdir is available in tests");
        let catalog = tempdir().expect("tempdir is available in tests");
        let source = catalog.path().join("book.pdf");
        fs::write(&source, b"%PDF-1.4 fake").expect("test source file is writable");
        (cache_root, catalog, source)
    }

    #[test]
    fn round_trip_hits_on_identical_inputs() {
        let (root, catalog, source) = setup();
        let chapters = sample_chapters();
        store_in(root.path(), catalog.path(), 7, &source, WIDTH, &chapters);
        let loaded = load_in(root.path(), catalog.path(), 7, &source, WIDTH)
            .expect("store followed by load with identical inputs is a hit");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].title, "Chapter 1");
        assert_eq!(loaded[0].text, "hello\nworld");
        assert_eq!(loaded[0].lines, chapters[0].lines);
    }

    #[test]
    fn miss_on_render_width_change() {
        let (root, catalog, source) = setup();
        store_in(
            root.path(),
            catalog.path(),
            7,
            &source,
            WIDTH,
            &sample_chapters(),
        );
        assert!(load_in(root.path(), catalog.path(), 7, &source, 80).is_none());
    }

    #[test]
    fn miss_on_schema_version_mismatch() {
        let (root, catalog, source) = setup();
        store_in(
            root.path(),
            catalog.path(),
            7,
            &source,
            WIDTH,
            &sample_chapters(),
        );
        let path = entry_path(root.path(), catalog.path(), 7);
        let bytes = fs::read(&path).expect("store_in just wrote this entry");
        let mut value: serde_json::Value =
            serde_json::from_slice(&bytes).expect("store_in writes valid JSON");
        value["schema_version"] = serde_json::json!(CACHE_SCHEMA_VERSION + 1);
        fs::write(&path, value.to_string()).expect("cache entry is writable in tests");
        assert!(load_in(root.path(), catalog.path(), 7, &source, WIDTH).is_none());
    }

    #[test]
    fn miss_on_source_mtime_change() {
        let (root, catalog, source) = setup();
        store_in(
            root.path(),
            catalog.path(),
            7,
            &source,
            WIDTH,
            &sample_chapters(),
        );
        let file = File::options()
            .write(true)
            .open(&source)
            .expect("test source file is writable");
        // Move mtime far into the past so the change is visible regardless of
        // filesystem timestamp granularity.
        let past = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        file.set_times(fs::FileTimes::new().set_modified(past))
            .expect("set_times is supported on test filesystems");
        assert!(load_in(root.path(), catalog.path(), 7, &source, WIDTH).is_none());
    }

    #[test]
    fn miss_on_source_size_change() {
        let (root, catalog, source) = setup();
        store_in(
            root.path(),
            catalog.path(),
            7,
            &source,
            WIDTH,
            &sample_chapters(),
        );
        let mut bytes = fs::read(&source).expect("test source file is readable");
        bytes.push(b'!');
        fs::write(&source, &bytes).expect("test source file is writable");
        // Re-stamp mtime to the stored value so only the size differs.
        let stored: CacheEnvelope = serde_json::from_slice(
            &fs::read(entry_path(root.path(), catalog.path(), 7))
                .expect("store_in just wrote this entry"),
        )
        .expect("store_in writes valid JSON");
        let mtime = std::time::SystemTime::UNIX_EPOCH
            + std::time::Duration::from_nanos(stored.source_mtime_ns);
        File::options()
            .write(true)
            .open(&source)
            .expect("test source file is writable")
            .set_times(fs::FileTimes::new().set_modified(mtime))
            .expect("set_times is supported on test filesystems");
        assert!(load_in(root.path(), catalog.path(), 7, &source, WIDTH).is_none());
    }

    #[test]
    fn corrupt_cache_file_is_ignored() {
        let (root, catalog, source) = setup();
        let path = entry_path(root.path(), catalog.path(), 7);
        fs::create_dir_all(path.parent().expect("entry path always has a parent"))
            .expect("cache dir is creatable in tests");
        fs::write(&path, b"not json at all").expect("cache entry is writable in tests");
        assert!(load_in(root.path(), catalog.path(), 7, &source, WIDTH).is_none());
    }

    #[test]
    fn missing_source_is_a_miss() {
        let (root, catalog, source) = setup();
        store_in(
            root.path(),
            catalog.path(),
            7,
            &source,
            WIDTH,
            &sample_chapters(),
        );
        fs::remove_file(&source).expect("test source file is removable");
        assert!(load_in(root.path(), catalog.path(), 7, &source, WIDTH).is_none());
    }

    #[test]
    fn is_cacheable_excludes_plain_text_formats() {
        assert!(is_cacheable(Format::Pdf));
        assert!(is_cacheable(Format::Epub));
        assert!(is_cacheable(Format::Mobi));
        assert!(is_cacheable(Format::Azw3));
        assert!(!is_cacheable(Format::Txt));
        assert!(!is_cacheable(Format::Md));
    }

    #[test]
    fn catalog_key_is_stable_and_short() {
        let (_, catalog, _) = setup();
        let a = catalog_key(catalog.path());
        let b = catalog_key(catalog.path());
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
    }
}
