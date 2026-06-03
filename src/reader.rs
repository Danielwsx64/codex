use std::path::{Path, PathBuf};

use thiserror::Error;

pub mod layout;

use crate::catalog::books::Book as CatalogBook;
use crate::epub;
use crate::import::Format;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io error on `{}`: {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Epub(#[from] epub::Error),
    #[error("format `{format}` is not supported by the reader yet")]
    UnsupportedFormat { format: String },
    #[error("book file `{}` is not valid UTF-8", .path.display())]
    NotUtf8 { path: PathBuf },
    #[error("failed to render chapter `{title}` as text: {source}")]
    HtmlRender {
        title: String,
        #[source]
        source: html2text::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub struct Book {
    pub id: i64,
    pub title: String,
    pub author: Option<String>,
    pub chapters: Vec<Chapter>,
}

#[derive(Debug, Clone)]
pub struct Chapter {
    pub title: String,
    pub text: String,
}

pub fn open(catalog_dir: &Path, book: &CatalogBook, render_width: usize) -> Result<Book> {
    let format = Format::parse_label(&book.format).ok_or_else(|| Error::UnsupportedFormat {
        format: book.format.clone(),
    })?;
    let abs_path = catalog_dir.join(&book.file_path);
    let chapters = read_chapters(&abs_path, format, render_width)?;
    Ok(Book {
        id: book.id,
        title: book.title.clone(),
        author: book.author.clone(),
        chapters,
    })
}

fn read_chapters(path: &Path, format: Format, render_width: usize) -> Result<Vec<Chapter>> {
    match format {
        Format::Epub => read_epub(path, render_width),
        Format::Txt => read_txt(path),
        Format::Md => read_md(path),
        Format::Pdf | Format::Mobi | Format::Azw3 => Err(Error::UnsupportedFormat {
            format: format.label().to_string(),
        }),
    }
}

fn read_epub(path: &Path, render_width: usize) -> Result<Vec<Chapter>> {
    let mut book = epub::Epub::open(path)?;
    let toc: Vec<(String, String)> = book
        .toc()
        .iter()
        .map(|e| (e.title.clone(), e.href.clone()))
        .collect();
    let spine_len = book.spine().len();
    let mut chapters = Vec::with_capacity(spine_len);
    let width = render_width.max(20);
    for idx in 0..spine_len {
        let title = chapter_title_for(&book, &toc, idx);
        let bytes = book.read_spine_item(idx)?;
        let text = html2text::from_read(&bytes[..], width).map_err(|source| Error::HtmlRender {
            title: title.clone(),
            source,
        })?;
        chapters.push(Chapter { title, text });
    }
    Ok(chapters)
}

fn chapter_title_for(book: &epub::Epub, toc: &[(String, String)], idx: usize) -> String {
    let default = format!("Chapter {}", idx + 1);
    let Some(item) = book.spine().get(idx) else {
        return default;
    };
    for (title, href) in toc {
        if hrefs_match(&item.href, href) {
            let trimmed = title.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    default
}

fn hrefs_match(spine_href: &str, toc_href: &str) -> bool {
    let a = strip_href(spine_href);
    let b = strip_href(toc_href);
    a == b || a.ends_with(b) || b.ends_with(a)
}

fn strip_href(s: &str) -> &str {
    s.split('#').next().unwrap_or(s).trim_start_matches("./")
}

fn read_txt(path: &Path) -> Result<Vec<Chapter>> {
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let text = String::from_utf8(bytes).map_err(|_| Error::NotUtf8 {
        path: path.to_path_buf(),
    })?;
    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Document")
        .to_string();
    Ok(vec![Chapter { title, text }])
}

fn read_md(path: &Path) -> Result<Vec<Chapter>> {
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let source = String::from_utf8(bytes).map_err(|_| Error::NotUtf8 {
        path: path.to_path_buf(),
    })?;
    let rendered = markdown_to_text(&source);
    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Document")
        .to_string();
    Ok(vec![Chapter {
        title,
        text: rendered,
    }])
}

fn markdown_to_text(source: &str) -> String {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};
    let parser = Parser::new(source);
    let mut out = String::with_capacity(source.len());
    let mut list_depth: usize = 0;
    let mut at_line_start = true;

    let push_newline = |out: &mut String, at_line_start: &mut bool| {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        *at_line_start = true;
    };

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                push_newline(&mut out, &mut at_line_start);
                if !out.is_empty() && !out.ends_with("\n\n") {
                    out.push('\n');
                }
                for _ in 0..(level as usize) {
                    out.push('#');
                }
                out.push(' ');
                at_line_start = false;
            }
            Event::End(TagEnd::Heading(_)) => {
                push_newline(&mut out, &mut at_line_start);
                out.push('\n');
            }
            Event::Start(Tag::Paragraph) => {
                if !out.is_empty() && !out.ends_with("\n\n") {
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push('\n');
                }
                at_line_start = true;
            }
            Event::End(TagEnd::Paragraph) => {
                push_newline(&mut out, &mut at_line_start);
            }
            Event::Start(Tag::BlockQuote(_)) => {
                push_newline(&mut out, &mut at_line_start);
                out.push_str("> ");
                at_line_start = false;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                push_newline(&mut out, &mut at_line_start);
            }
            Event::Start(Tag::CodeBlock(_)) => {
                push_newline(&mut out, &mut at_line_start);
                out.push_str("```\n");
                at_line_start = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                push_newline(&mut out, &mut at_line_start);
                out.push_str("```\n");
            }
            Event::Start(Tag::List(_)) => {
                push_newline(&mut out, &mut at_line_start);
                list_depth += 1;
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                push_newline(&mut out, &mut at_line_start);
            }
            Event::Start(Tag::Item) => {
                push_newline(&mut out, &mut at_line_start);
                for _ in 1..list_depth {
                    out.push_str("  ");
                }
                out.push_str("- ");
                at_line_start = false;
            }
            Event::End(TagEnd::Item) => {
                push_newline(&mut out, &mut at_line_start);
            }
            Event::Text(t) | Event::Code(t) | Event::Html(t) | Event::InlineHtml(t) => {
                out.push_str(&t);
                at_line_start = false;
            }
            Event::SoftBreak => {
                out.push(' ');
                at_line_start = false;
            }
            Event::HardBreak => {
                push_newline(&mut out, &mut at_line_start);
            }
            Event::Rule => {
                push_newline(&mut out, &mut at_line_start);
                out.push_str("---\n");
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_txt_returns_one_chapter() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, b"hello world\nsecond line").unwrap();
        let chapters = read_txt(&path).unwrap();
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].title, "notes");
        assert!(chapters[0].text.contains("hello world"));
    }

    #[test]
    fn markdown_renders_headings_and_paragraphs() {
        let out = markdown_to_text("# Title\n\nFirst para.\n\nSecond *italic* para.\n");
        assert!(out.contains("# Title"));
        assert!(out.contains("First para."));
        assert!(out.contains("Second italic para."));
    }

    #[test]
    fn markdown_renders_lists() {
        let out = markdown_to_text("- a\n- b\n- c\n");
        assert!(out.contains("- a"));
        assert!(out.contains("- b"));
        assert!(out.contains("- c"));
    }

    #[test]
    fn read_epub_loads_sample_fixture_chapters() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("sample.epub");
        let chapters = read_epub(&fixture, 80).unwrap();
        assert!(
            !chapters.is_empty(),
            "fixture must produce at least one chapter"
        );
        assert!(
            chapters.iter().any(|c| !c.text.trim().is_empty()),
            "fixture must produce non-empty text"
        );
    }

    #[test]
    fn unsupported_format_errors_out() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.pdf");
        std::fs::write(&path, b"%PDF-").unwrap();
        let err = read_chapters(&path, Format::Pdf, 80).unwrap_err();
        match err {
            Error::UnsupportedFormat { format } => assert_eq!(format, "pdf"),
            other => panic!("expected UnsupportedFormat, got {other:?}"),
        }
    }

    #[test]
    fn hrefs_match_strips_fragment_and_dot_prefix() {
        assert!(hrefs_match("ch1.xhtml", "ch1.xhtml#intro"));
        assert!(hrefs_match("./ch1.xhtml", "ch1.xhtml"));
        assert!(hrefs_match("OEBPS/ch1.xhtml", "ch1.xhtml"));
    }
}
