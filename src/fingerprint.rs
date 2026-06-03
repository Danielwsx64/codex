use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::reader::Reader;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::import::Format;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    // Hash of the whole file as it sits on disk. Changes when `embed sync`
    // rewrites the file, so it is accumulated as a list per book.
    Full,
    // Hash of the format's immutable payload, stable across `embed sync`.
    // Only emitted for formats where we can isolate that payload (EPUB today).
    Content,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Full => "full",
            Kind::Content => "content",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "full" => Some(Kind::Full),
            "content" => Some(Kind::Content),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint {
    pub kind: Kind,
    pub hash: String,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("io error on `{}`: {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("zip error on `{}`: {source}", .path.display())]
    Zip {
        path: PathBuf,
        #[source]
        source: zip::result::ZipError,
    },
    #[error("epub `{}` has no rootfile in META-INF/container.xml", .path.display())]
    MissingOpf { path: PathBuf },
    #[error("failed to parse epub xml in `{}`: {source}", .path.display())]
    Xml {
        path: PathBuf,
        #[source]
        source: quick_xml::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn compute(path: &Path, format: Format) -> Result<Vec<Fingerprint>> {
    match format {
        Format::Epub => epub_fingerprints(path),
        Format::Pdf | Format::Mobi | Format::Azw3 | Format::Txt | Format::Md => {
            Ok(vec![Fingerprint {
                kind: Kind::Full,
                hash: hash_full(path)?,
            }])
        }
    }
}

pub fn hash_full(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(hasher.finalize()))
}

fn epub_fingerprints(path: &Path) -> Result<Vec<Fingerprint>> {
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut fingerprints = vec![Fingerprint {
        kind: Kind::Full,
        hash: hex(Sha256::digest(&bytes)),
    }];
    // The content hash is best-effort: a malformed EPUB still imports with just
    // the whole-file fingerprint, mirroring how metadata extraction tolerates
    // junk rather than rejecting the file.
    match epub_content_hash(&bytes, path) {
        Ok(content) => fingerprints.push(Fingerprint {
            kind: Kind::Content,
            hash: content,
        }),
        Err(err) => {
            tracing::warn!(path = %path.display(), error = %err, "epub content hash unavailable; using full hash only")
        }
    }
    Ok(fingerprints)
}

// `embed sync` only rewrites the OPF entry and re-zips the rest, so the OPF is
// excluded and we hash the *decompressed* content of every other entry (the
// re-deflate can change compressed bytes while leaving the payload identical).
fn epub_content_hash(bytes: &[u8], path: &Path) -> Result<String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|source| Error::Zip {
        path: path.to_path_buf(),
        source,
    })?;
    let opf_path = find_opf_path(&mut archive, path)?;
    let mut names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    names.sort();

    let mut hasher = Sha256::new();
    for name in &names {
        if *name == opf_path || name.ends_with('/') {
            continue;
        }
        let data = read_entry(&mut archive, name, path)?;
        // Length-framed so an entry's name can't bleed into another's bytes.
        hasher.update(name.as_bytes());
        hasher.update([0u8]);
        hasher.update((data.len() as u64).to_le_bytes());
        hasher.update(&data);
    }
    Ok(hex(hasher.finalize()))
}

fn read_entry<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
    path: &Path,
) -> Result<Vec<u8>> {
    let mut entry = archive.by_name(name).map_err(|source| Error::Zip {
        path: path.to_path_buf(),
        source,
    })?;
    let mut buf = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut buf).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(buf)
}

fn find_opf_path<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    path: &Path,
) -> Result<String> {
    let container = read_entry(archive, "META-INF/container.xml", path)?;
    let mut reader = Reader::from_reader(container.as_slice());
    let mut buf = Vec::new();
    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|source| Error::Xml {
                path: path.to_path_buf(),
                source,
            })? {
            Event::Empty(e) | Event::Start(e) if local_name(e.name().as_ref()) == "rootfile" => {
                for attr in e.attributes().filter_map(|a| a.ok()) {
                    if local_name(attr.key.as_ref()) == "full-path" {
                        return Ok(String::from_utf8_lossy(attr.value.as_ref()).into_owned());
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Err(Error::MissingOpf {
        path: path.to_path_buf(),
    })
}

fn local_name(qname: &[u8]) -> &str {
    let s = std::str::from_utf8(qname).unwrap_or("");
    match s.rsplit_once(':') {
        Some((_, local)) => local,
        None => s,
    }
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
    use tempfile::tempdir;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    fn pick(fps: &[Fingerprint], kind: Kind) -> &str {
        fps.iter()
            .find(|f| f.kind == kind)
            .map(|f| f.hash.as_str())
            .expect("fingerprint of requested kind is present")
    }

    #[test]
    fn epub_emits_full_and_content() {
        let fps = compute(&fixture("sample.epub"), Format::Epub).unwrap();
        assert_eq!(fps.len(), 2);
        assert!(fps.iter().any(|f| f.kind == Kind::Full));
        assert!(fps.iter().any(|f| f.kind == Kind::Content));
    }

    #[test]
    fn pdf_emits_only_full() {
        let fps = compute(&fixture("sample.pdf"), Format::Pdf).unwrap();
        assert_eq!(fps.len(), 1);
        assert_eq!(fps[0].kind, Kind::Full);
    }

    #[test]
    fn epub_content_hash_survives_embed_metadata_rewrite() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("book.epub");
        std::fs::copy(fixture("sample.epub"), &target).unwrap();

        let before = compute(&target, Format::Epub).unwrap();

        // Rewrite the OPF metadata in place via the real embed path.
        let book = crate::catalog::books::Book {
            id: 1,
            title: "Totally Different Title".into(),
            author: Some("Someone Else".into()),
            format: "epub".into(),
            file_path: "books/1/x.epub".into(),
            added_at: "now".into(),
            description: Some("changed".into()),
            series_name: None,
            series_index: None,
            rating: None,
            isbn: None,
            publisher: None,
            language: None,
            published_date: None,
            tags: vec!["changed".into()],
            embed_status: crate::catalog::books::EmbedStatus::Pending,
            embed_synced_at: None,
        };
        crate::embed::epub::write(&target, &book).unwrap();

        let after = compute(&target, Format::Epub).unwrap();

        // Whole-file hash changes (OPF + re-zip); content hash is stable.
        assert_ne!(pick(&before, Kind::Full), pick(&after, Kind::Full));
        assert_eq!(pick(&before, Kind::Content), pick(&after, Kind::Content));
    }

    #[test]
    fn full_hash_changes_with_content() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a.pdf");
        let b = dir.path().join("b.pdf");
        std::fs::write(&a, b"hello").unwrap();
        std::fs::write(&b, b"world").unwrap();
        assert_ne!(hash_full(&a).unwrap(), hash_full(&b).unwrap());
    }
}
