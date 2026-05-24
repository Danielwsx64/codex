use std::path::{Path, PathBuf};

use thiserror::Error;

pub mod epub;
pub mod mobi;
pub mod pdf;

const MAX_STEM_LEN: usize = 150;
const SUPPORTED_LIST: &str = "epub, pdf, mobi, azw3";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Epub,
    Pdf,
    Mobi,
    Azw3,
}

impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Format::Epub => "epub",
            Format::Pdf => "pdf",
            Format::Mobi => "mobi",
            Format::Azw3 => "azw3",
        }
    }

    pub fn extension(self) -> &'static str {
        self.label()
    }

    pub fn parse_label(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "epub" => Some(Format::Epub),
            "pdf" => Some(Format::Pdf),
            "mobi" => Some(Format::Mobi),
            "azw3" => Some(Format::Azw3),
            _ => None,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Metadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub series_name: Option<String>,
    pub series_index: Option<f64>,
    pub isbn: Option<String>,
    pub publisher: Option<String>,
    pub language: Option<String>,
    pub published_date: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("file `{}` does not exist", .path.display())]
    NotFound { path: PathBuf },
    #[error("file `{}` has no extension; cannot determine format (supported: {SUPPORTED_LIST})", .path.display())]
    NoExtension { path: PathBuf },
    #[error("format `.{ext}` is not supported for `{}` (supported: {SUPPORTED_LIST})", .path.display())]
    Unsupported { path: PathBuf, ext: String },
    #[error("io error on `{}`: {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read epub `{}`: {source}", .path.display())]
    EpubZip {
        path: PathBuf,
        #[source]
        source: zip::result::ZipError,
    },
    #[error("failed to parse epub xml in `{}`: {source}", .path.display())]
    EpubXml {
        path: PathBuf,
        #[source]
        source: quick_xml::Error,
    },
    #[error("epub `{}` has no rootfile in META-INF/container.xml", .path.display())]
    EpubMissingOpf { path: PathBuf },
    #[error("failed to read pdf `{}`: {source}", .path.display())]
    Pdf {
        path: PathBuf,
        #[source]
        source: lopdf::Error,
    },
    #[error("failed to read mobi/azw3 `{}`: {source}", .path.display())]
    Mobi {
        path: PathBuf,
        #[source]
        source: ::mobi::MobiError,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn detect(path: &Path) -> Result<Format> {
    if !path.exists() {
        return Err(Error::NotFound {
            path: path.to_path_buf(),
        });
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .ok_or_else(|| Error::NoExtension {
            path: path.to_path_buf(),
        })?
        .to_ascii_lowercase();
    Format::parse_label(&ext).ok_or_else(|| Error::Unsupported {
        path: path.to_path_buf(),
        ext,
    })
}

pub fn extract(path: &Path, format: Format) -> Result<Metadata> {
    match format {
        Format::Epub => epub::extract(path),
        Format::Pdf => pdf::extract(path),
        Format::Mobi | Format::Azw3 => mobi::extract(path),
    }
}

pub fn build_filename(meta: &Metadata, format: Format, fallback_stem: &str) -> String {
    let title = meta
        .title
        .as_deref()
        .map(sanitize)
        .filter(|s| !s.is_empty());
    let author = meta
        .author
        .as_deref()
        .map(sanitize)
        .filter(|s| !s.is_empty());

    let stem = match (author, title) {
        (Some(a), Some(t)) => truncate_stem(&format!("{a}_-_{t}")),
        (Some(a), None) => truncate_stem(&a),
        (None, Some(t)) => truncate_stem(&t),
        (None, None) => {
            let fallback = sanitize(fallback_stem);
            if fallback.is_empty() {
                "book".to_string()
            } else {
                truncate_stem(&fallback)
            }
        }
    };
    format!("{stem}.{ext}", ext = format.extension())
}

pub fn sanitize(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_underscore = false;
    for ch in input.chars() {
        let mapped = if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' {
            Some(ch)
        } else if ch == ' ' || ch == '_' || ch == '\t' {
            Some('_')
        } else {
            None
        };
        match mapped {
            Some('_') => {
                if !prev_underscore {
                    out.push('_');
                    prev_underscore = true;
                }
            }
            Some(c) => {
                out.push(c);
                prev_underscore = false;
            }
            None => {}
        }
    }
    while out.starts_with('_') || out.starts_with('.') {
        out.remove(0);
    }
    while out.ends_with('_') || out.ends_with('.') {
        out.pop();
    }
    out
}

fn truncate_stem(s: &str) -> String {
    if s.len() <= MAX_STEM_LEN {
        return s.to_string();
    }
    let mut end = MAX_STEM_LEN;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    let mut truncated = s[..end].to_string();
    while truncated.ends_with('_') || truncated.ends_with('.') {
        truncated.pop();
    }
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn detect_lowercases_extension() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("Book.EPUB");
        fs::write(&p, b"dummy").unwrap();
        assert_eq!(detect(&p).unwrap(), Format::Epub);
    }

    #[test]
    fn detect_rejects_unknown_extension() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("note.txt");
        fs::write(&p, b"hello").unwrap();
        let err = detect(&p).unwrap_err();
        assert!(matches!(err, Error::Unsupported { ref ext, .. } if ext == "txt"));
        assert!(format!("{err}").contains("supported: epub, pdf, mobi, azw3"));
    }

    #[test]
    fn detect_rejects_missing_file() {
        let dir = tempdir().unwrap();
        let err = detect(&dir.path().join("ghost.epub")).unwrap_err();
        assert!(matches!(err, Error::NotFound { .. }));
    }

    #[test]
    fn detect_rejects_no_extension() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("noext");
        fs::write(&p, b"hi").unwrap();
        let err = detect(&p).unwrap_err();
        assert!(matches!(err, Error::NoExtension { .. }));
    }

    #[test]
    fn sanitize_replaces_spaces_with_underscore() {
        assert_eq!(sanitize("The Way of Kings"), "The_Way_of_Kings");
    }

    #[test]
    fn sanitize_strips_special_chars() {
        assert_eq!(sanitize("Foo: Bar / Baz!"), "Foo_Bar_Baz");
    }

    #[test]
    fn sanitize_collapses_underscores() {
        assert_eq!(sanitize("a   b___c"), "a_b_c");
    }

    #[test]
    fn sanitize_strips_leading_trailing_punct() {
        assert_eq!(sanitize(".__hello world__."), "hello_world");
    }

    #[test]
    fn sanitize_removes_accents_as_unknown_chars() {
        assert_eq!(sanitize("Brontë"), "Bront");
    }

    #[test]
    fn sanitize_empty_when_only_special() {
        assert_eq!(sanitize("!!!???"), "");
    }

    #[test]
    fn build_filename_author_and_title() {
        let meta = Metadata {
            title: Some("The Way of Kings".to_string()),
            author: Some("Brandon Sanderson".to_string()),
            ..Metadata::default()
        };
        assert_eq!(
            build_filename(&meta, Format::Epub, "fallback"),
            "Brandon_Sanderson_-_The_Way_of_Kings.epub"
        );
    }

    #[test]
    fn build_filename_only_title() {
        let meta = Metadata {
            title: Some("Lone Title".to_string()),
            ..Metadata::default()
        };
        assert_eq!(build_filename(&meta, Format::Pdf, "f"), "Lone_Title.pdf");
    }

    #[test]
    fn build_filename_only_author() {
        let meta = Metadata {
            author: Some("Solo Author".to_string()),
            ..Metadata::default()
        };
        assert_eq!(build_filename(&meta, Format::Mobi, "f"), "Solo_Author.mobi");
    }

    #[test]
    fn build_filename_empty_uses_fallback() {
        let meta = Metadata::default();
        assert_eq!(
            build_filename(&meta, Format::Azw3, "my-book v1"),
            "my-book_v1.azw3"
        );
    }

    #[test]
    fn build_filename_truncates_long_stem() {
        let meta = Metadata {
            title: Some("x".repeat(500)),
            ..Metadata::default()
        };
        let name = build_filename(&meta, Format::Epub, "f");
        assert!(name.len() <= MAX_STEM_LEN + ".epub".len());
        assert!(name.ends_with(".epub"));
    }

    #[test]
    fn build_filename_blank_metadata_blank_fallback() {
        let meta = Metadata::default();
        assert_eq!(build_filename(&meta, Format::Epub, "???"), "book.epub");
    }
}
