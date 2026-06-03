use std::io::Read;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use mobi::headers::{Compression, Encryption};
use mobi::{Mobi, MobiResult};

use super::{tagged_lines_to_styled, Chapter, Error, Result};

// PDB containers carry a type/creator signature at bytes 60..68; MOBI/AZW3
// files read `BOOKMOBI`, bare PalmDOC reads `TEXtREAd`.
const PDB_SIGNATURE_START: usize = 60;
const PDB_SIGNATURE_END: usize = 68;

pub(crate) fn read_mobi(path: &Path, render_width: usize) -> Result<Vec<Chapter>> {
    sniff_container(path)?;
    let book = catch_mobi(path, || Mobi::from_path(path))?;
    gate_drm(&book, path)?;
    gate_compression(&book, path)?;
    let html = extract_html(&book, path)?;
    let fragments = split_on_pagebreak(&html);
    let chapters = render_fragments(&fragments, render_width)?;
    ensure_readable(chapters, path)
}

// Topaz and KFX are not PDB/MOBI containers at all; the mobi crate would
// surface them as an opaque parse error (or worse, a panic). Sniffing the
// magic bytes up front buys a precise message for those formats.
fn sniff_container(path: &Path) -> Result<()> {
    let file = std::fs::File::open(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut head = Vec::with_capacity(PDB_SIGNATURE_END);
    file.take(PDB_SIGNATURE_END as u64)
        .read_to_end(&mut head)
        .map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
    container_kind(&head).map_err(|kind| Error::UnsupportedKindleContainer {
        path: path.to_path_buf(),
        kind: kind.to_string(),
    })
}

// `Err` carries the human-readable container kind for the error message.
fn container_kind(head: &[u8]) -> std::result::Result<(), &'static str> {
    if head.starts_with(b"TPZ") {
        return Err("Amazon Topaz");
    }
    if head.starts_with(b"CONT") || head.starts_with(b"\xeaDRMION\xee") {
        return Err("Amazon KFX");
    }
    let Some(signature) = head.get(PDB_SIGNATURE_START..PDB_SIGNATURE_END) else {
        return Err("truncated file");
    };
    if signature == b"BOOKMOBI" || signature == b"TEXtREAd" {
        return Ok(());
    }
    Err("unknown signature")
}

// The mobi crate slices record content without bounds checks (record.rs in
// mobi 0.8), so a truncated or malformed file can panic instead of erroring.
// One bad book must not abort the whole TUI session — catch the unwind and
// surface it as a regular reader error.
fn catch_mobi<T>(path: &Path, op: impl FnOnce() -> MobiResult<T>) -> Result<T> {
    match catch_unwind(AssertUnwindSafe(op)) {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(source)) => Err(Error::Mobi {
            path: path.to_path_buf(),
            source,
        }),
        Err(_) => Err(Error::MobiPanic {
            path: path.to_path_buf(),
        }),
    }
}

// Encryption is what actually makes the text records unreadable; a DRM
// offset with no encryption still decodes fine, so it is not gated on.
fn gate_drm(book: &Mobi, path: &Path) -> Result<()> {
    if book.encryption() != Encryption::No {
        return Err(Error::DrmProtected {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

// The crate's HUFF/CDIC decoder is unproven and its lossy path degrades to
// an empty string; better to refuse loudly than render a blank book.
fn gate_compression(book: &Mobi, path: &Path) -> Result<()> {
    if book.compression() == Compression::Huff {
        return Err(Error::UnsupportedCompression {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

fn extract_html(book: &Mobi, path: &Path) -> Result<String> {
    catch_mobi(path, || {
        Ok(book.content_as_string().unwrap_or_else(|error| {
            tracing::warn!(
                path = %path.display(),
                %error,
                "strict text decode failed; falling back to lossy"
            );
            book.content_as_string_lossy()
        }))
    })
}

// MOBI6 marks chapter boundaries with <mbp:pagebreak/> tags inside the
// single HTML blob. The split is intentionally literal (this exact tag, any
// case, any attributes): saved reading progress stores (chapter, offset),
// so a fuzzier heuristic changing later would shift stored positions.
fn split_on_pagebreak(html: &str) -> Vec<&str> {
    const MARKER: &str = "<mbp:pagebreak";
    // ASCII lowercasing keeps byte offsets aligned with the original.
    let lower = html.to_ascii_lowercase();
    let mut fragments = Vec::new();
    let mut start = 0;
    let mut cursor = 0;
    while let Some(found) = lower[cursor..].find(MARKER) {
        let at = cursor + found;
        fragments.push(&html[start..at]);
        let after = lower[at..]
            .find('>')
            .map(|gt| at + gt + 1)
            .unwrap_or(html.len());
        start = after;
        cursor = after;
    }
    fragments.push(&html[start..]);
    fragments
}

fn render_fragments(fragments: &[&str], render_width: usize) -> Result<Vec<Chapter>> {
    let width = render_width.max(20);
    let mut chapters: Vec<Chapter> = Vec::new();
    for fragment in fragments {
        let title = format!("Chapter {}", chapters.len() + 1);
        let tagged = html2text::from_read_rich(fragment.as_bytes(), width).map_err(|source| {
            Error::HtmlRender {
                title: title.clone(),
                source,
            }
        })?;
        let chapter = Chapter::from_lines(title, tagged_lines_to_styled(&tagged));
        // A pagebreak at the very start (or back-to-back breaks) yields an
        // empty fragment — drop it instead of rendering a blank chapter.
        if chapter.text.trim().is_empty() {
            continue;
        }
        chapters.push(chapter);
    }
    Ok(chapters)
}

fn ensure_readable(chapters: Vec<Chapter>, path: &Path) -> Result<Vec<Chapter>> {
    if !chapters.is_empty() {
        return Ok(chapters);
    }
    let is_azw3 = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("azw3"));
    if is_azw3 {
        // mobi 0.8 has no KF8 support: a KF8-only AZW3 parses as a container
        // but yields no readable legacy (KF7) text records.
        Err(Error::Azw3NoLegacyStream {
            path: path.to_path_buf(),
        })
    } else {
        Err(Error::EmptyContent {
            path: path.to_path_buf(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapters_from(html: &str) -> Vec<Chapter> {
        render_fragments(&split_on_pagebreak(html), 200).unwrap()
    }

    #[test]
    fn splits_on_pagebreak() {
        let chapters = chapters_from("<p>first part</p><mbp:pagebreak/><p>second part</p>");
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].title, "Chapter 1");
        assert!(chapters[0].text.contains("first part"));
        assert_eq!(chapters[1].title, "Chapter 2");
        assert!(chapters[1].text.contains("second part"));
    }

    #[test]
    fn single_chapter_when_no_pagebreak() {
        let chapters = chapters_from("<p>just one body of text</p>");
        assert_eq!(chapters.len(), 1);
        assert!(chapters[0].text.contains("just one body of text"));
    }

    #[test]
    fn pagebreak_split_tolerant_of_attrs_and_case() {
        let chapters =
            chapters_from("<p>a</p><MBP:PAGEBREAK><p>b</p><mbp:pagebreak align=\"x\" /><p>c</p>");
        assert_eq!(chapters.len(), 3);
        assert!(chapters[0].text.contains('a'));
        assert!(chapters[1].text.contains('b'));
        assert!(chapters[2].text.contains('c'));
    }

    #[test]
    fn leading_pagebreak_does_not_create_blank_chapter() {
        let chapters = chapters_from("<mbp:pagebreak/><p>body</p>");
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].title, "Chapter 1");
        assert!(chapters[0].text.contains("body"));
    }

    #[test]
    fn sniff_accepts_bookmobi_and_textread() {
        let mut head = vec![0u8; PDB_SIGNATURE_END];
        head[PDB_SIGNATURE_START..].copy_from_slice(b"BOOKMOBI");
        assert!(container_kind(&head).is_ok());
        head[PDB_SIGNATURE_START..].copy_from_slice(b"TEXtREAd");
        assert!(container_kind(&head).is_ok());
    }

    #[test]
    fn sniff_rejects_topaz_kfx_truncated_and_unknown() {
        assert_eq!(container_kind(b"TPZ0 rest"), Err("Amazon Topaz"));
        assert_eq!(container_kind(b"CONT rest"), Err("Amazon KFX"));
        assert_eq!(container_kind(b"\xeaDRMION\xee rest"), Err("Amazon KFX"));
        assert_eq!(container_kind(b"too short"), Err("truncated file"));
        let garbage = vec![b'x'; PDB_SIGNATURE_END];
        assert_eq!(container_kind(&garbage), Err("unknown signature"));
    }

    #[test]
    fn gate_drm_rejects_encrypted() {
        let mut book = Mobi::default();
        book.metadata.palmdoc.encryption = Encryption::MobiPocket;
        let err = gate_drm(&book, Path::new("b.mobi")).unwrap_err();
        assert!(matches!(err, Error::DrmProtected { .. }));
    }

    #[test]
    fn gate_compression_rejects_huff() {
        let mut book = Mobi::default();
        book.metadata.palmdoc.compression = Compression::Huff;
        let err = gate_compression(&book, Path::new("b.mobi")).unwrap_err();
        assert!(matches!(err, Error::UnsupportedCompression { .. }));
    }

    #[test]
    fn gates_pass_for_plain_unencrypted_book() {
        let book = Mobi::default();
        gate_drm(&book, Path::new("b.mobi")).unwrap();
        gate_compression(&book, Path::new("b.mobi")).unwrap();
    }

    #[test]
    fn empty_azw3_maps_to_no_legacy_stream() {
        let err = ensure_readable(Vec::new(), Path::new("book.azw3")).unwrap_err();
        assert!(matches!(err, Error::Azw3NoLegacyStream { .. }));
    }

    #[test]
    fn empty_mobi_maps_to_empty_content() {
        let err = ensure_readable(Vec::new(), Path::new("book.mobi")).unwrap_err();
        assert!(matches!(err, Error::EmptyContent { .. }));
    }

    #[test]
    fn catch_mobi_converts_panics_into_errors() {
        let err = catch_mobi(Path::new("b.mobi"), || -> MobiResult<()> {
            panic!("simulated parser panic")
        })
        .unwrap_err();
        assert!(matches!(err, Error::MobiPanic { .. }));
    }

    #[test]
    fn read_mobi_rejects_garbage_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("junk.mobi");
        std::fs::write(&path, b"this is not a mobi").unwrap();
        let err = read_mobi(&path, 80).unwrap_err();
        assert!(matches!(err, Error::UnsupportedKindleContainer { .. }));
    }
}
