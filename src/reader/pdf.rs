use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use lopdf::Document;

use super::style::{StyledLine, StyledSpan, TextStyle};
use super::{Chapter, Error, Result};

// Multi-column detection works on extracted text only (pdf-extract exposes no
// glyph coordinates): a column gutter survives extraction as a run of spaces
// in the middle of many consecutive lines.
const MIN_LINES_FOR_COLUMN_CHECK: usize = 6;
const COLUMN_GAP_MIN_SPACES: usize = 3;
const COLUMN_SUSPECT_RATIO_PCT: usize = 40;

const MULTICOLUMN_WARNING: &str = "[cdx] best-effort: multi-column layout detected, \
     text order may be scrambled — convert to EPUB for accurate reflow";

pub(crate) fn read_pdf(path: &Path) -> Result<Vec<Chapter>> {
    gate_encryption(path)?;
    let pages = extract_pages(path)?;
    let chapters = pages_to_chapters(&pages);
    ensure_readable(chapters, path)
}

// This loads the document a second time (pdf-extract parses it again for the
// text), but the structural parse is cheap next to glyph decoding and a book
// is opened once per session. Gating up front keeps encrypted and garbage
// files away from the panic-prone extractor with a precise message.
fn gate_encryption(path: &Path) -> Result<()> {
    let doc = Document::load(path).map_err(|source| Error::PdfStructure {
        path: path.to_path_buf(),
        source,
    })?;
    if doc.trailer.get(b"Encrypt").is_ok() {
        return Err(Error::PdfEncrypted {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

// pdf-extract unwraps liberally on malformed fonts and content streams; one
// bad book must not abort the whole TUI session — catch the unwind and
// surface it as a regular reader error.
fn extract_pages(path: &Path) -> Result<Vec<String>> {
    match catch_unwind(AssertUnwindSafe(|| {
        pdf_extract::extract_text_by_pages(path)
    })) {
        Ok(Ok(pages)) => Ok(pages),
        Ok(Err(source)) => Err(Error::Pdf {
            path: path.to_path_buf(),
            source,
        }),
        Err(_) => Err(Error::PdfPanic {
            path: path.to_path_buf(),
        }),
    }
}

// One chapter per PDF page so `:cN` jumps to the page people actually cite.
// Empty pages stay as empty chapters to keep that numbering aligned.
fn pages_to_chapters(pages: &[String]) -> Vec<Chapter> {
    pages
        .iter()
        .enumerate()
        .map(|(i, page)| {
            let mut lines = page_lines(page);
            if looks_multicolumn(page) {
                inject_warning(&mut lines);
            }
            Chapter::from_lines(format!("Page {}", i + 1), lines)
        })
        .collect()
}

fn page_lines(page: &str) -> Vec<StyledLine> {
    // pdf-extract prefixes every page with blank lines; strip them so each
    // chapter opens on content.
    page.trim_start_matches('\n')
        .split('\n')
        .map(StyledLine::from_plain)
        .collect()
}

fn looks_multicolumn(page: &str) -> bool {
    let lines: Vec<&str> = page.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() < MIN_LINES_FOR_COLUMN_CHECK {
        return false;
    }
    let gutter_lines = lines.iter().filter(|l| has_internal_gutter(l)).count();
    gutter_lines * 100 >= lines.len() * COLUMN_SUSPECT_RATIO_PCT
}

// A gutter is a run of spaces with text on both sides; leading indentation
// and trailing padding are trimmed away first so they never count.
fn has_internal_gutter(line: &str) -> bool {
    let mut run = 0usize;
    let mut saw_text = false;
    for ch in line.trim().chars() {
        if ch == ' ' {
            run += 1;
        } else {
            if saw_text && run >= COLUMN_GAP_MIN_SPACES {
                return true;
            }
            saw_text = true;
            run = 0;
        }
    }
    false
}

// The warning goes on every affected page (not just the first): chapters are
// laid out independently and `:cN` can land anywhere in the book.
fn inject_warning(lines: &mut Vec<StyledLine>) {
    let style = TextStyle {
        italic: true,
        ..TextStyle::PLAIN
    };
    lines.insert(
        0,
        StyledLine {
            spans: vec![StyledSpan::styled(MULTICOLUMN_WARNING, style)],
        },
    );
}

fn ensure_readable(chapters: Vec<Chapter>, path: &Path) -> Result<Vec<Chapter>> {
    if chapters.iter().all(|c| c.text.trim().is_empty()) {
        return Err(Error::EmptyContent {
            path: path.to_path_buf(),
        });
    }
    Ok(chapters)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_column_page() -> String {
        std::iter::repeat("left column text      right column text")
            .take(8)
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn detects_two_column_page() {
        assert!(looks_multicolumn(&two_column_page()));
    }

    #[test]
    fn plain_prose_is_not_multicolumn() {
        let page = std::iter::repeat("a normal sentence with single spaces only")
            .take(8)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!looks_multicolumn(&page));
    }

    #[test]
    fn too_few_lines_never_flag_multicolumn() {
        let page = "left text      right text\nleft text      right text";
        assert!(!looks_multicolumn(page));
    }

    #[test]
    fn indentation_is_not_a_gutter() {
        let page = std::iter::repeat("        an indented paragraph line")
            .take(8)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!looks_multicolumn(page.as_str()));
    }

    #[test]
    fn low_gutter_ratio_is_not_multicolumn() {
        let mut lines = vec!["prose line without any gutter at all"; 7];
        lines.push("aligned      pair");
        assert!(!looks_multicolumn(&lines.join("\n")));
    }

    #[test]
    fn one_chapter_per_page_with_one_based_titles() {
        let pages = vec!["\n\nalpha".to_string(), "\n\nbeta".to_string()];
        let chapters = pages_to_chapters(&pages);
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].title, "Page 1");
        assert_eq!(chapters[0].text, "alpha");
        assert_eq!(chapters[1].title, "Page 2");
        assert_eq!(chapters[1].text, "beta");
    }

    #[test]
    fn empty_page_stays_as_chapter_to_keep_numbering() {
        let pages = vec!["alpha".to_string(), String::new(), "gamma".to_string()];
        let chapters = pages_to_chapters(&pages);
        assert_eq!(chapters.len(), 3);
        assert_eq!(chapters[1].title, "Page 2");
        assert!(chapters[1].text.is_empty());
        assert_eq!(chapters[2].title, "Page 3");
    }

    #[test]
    fn multicolumn_page_gets_italic_warning_line() {
        let pages = vec![two_column_page()];
        let chapters = pages_to_chapters(&pages);
        let first_line = &chapters[0].lines[0];
        assert!(first_line.spans[0].style.italic);
        assert!(chapters[0].text.starts_with("[cdx] best-effort"));
    }

    #[test]
    fn single_column_page_gets_no_warning() {
        let pages = vec!["plain text".to_string()];
        let chapters = pages_to_chapters(&pages);
        assert!(!chapters[0].text.contains("[cdx]"));
    }

    #[test]
    fn ensure_readable_rejects_all_blank_pages() {
        let chapters = vec![
            Chapter::from_text("Page 1".into(), "   ".into()),
            Chapter::from_text("Page 2".into(), String::new()),
        ];
        let err = ensure_readable(chapters, Path::new("b.pdf")).unwrap_err();
        assert!(matches!(err, Error::EmptyContent { .. }));
    }

    #[test]
    fn ensure_readable_passes_with_any_content() {
        let chapters = vec![
            Chapter::from_text("Page 1".into(), String::new()),
            Chapter::from_text("Page 2".into(), "words".into()),
        ];
        assert_eq!(
            ensure_readable(chapters, Path::new("b.pdf")).unwrap().len(),
            2
        );
    }

    #[test]
    fn read_pdf_rejects_garbage_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("junk.pdf");
        std::fs::write(&path, b"this is not a pdf").unwrap();
        let err = read_pdf(&path).unwrap_err();
        assert!(matches!(err, Error::PdfStructure { .. }));
    }
}
