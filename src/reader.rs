use std::path::{Path, PathBuf};

use html2text::render::{RichAnnotation, TaggedLine, TaggedLineElement};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod layout;
pub mod style;

mod cache;
mod mobi;
mod pdf;

use crate::catalog::books::Book as CatalogBook;
use crate::epub;
use crate::import::Format;
use crate::reader::style::{StyledLine, StyledSpan, TextStyle};

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
    #[error("book file `{}` is DRM-protected; cdx does not remove DRM", .path.display())]
    DrmProtected { path: PathBuf },
    #[error("book file `{}` uses HUFF/CDIC compression, which the reader cannot decode", .path.display())]
    UnsupportedCompression { path: PathBuf },
    #[error("book file `{}` is not a readable MOBI container ({kind})", .path.display())]
    UnsupportedKindleContainer { path: PathBuf, kind: String },
    #[error("AZW3 file `{}` carries only a KF8 stream, which the reader cannot decode; convert it to EPUB first", .path.display())]
    Azw3NoLegacyStream { path: PathBuf },
    #[error("failed to parse mobi file `{}`: {source}", .path.display())]
    Mobi {
        path: PathBuf,
        #[source]
        source: ::mobi::MobiError,
    },
    #[error("mobi parser crashed reading `{}`; the file is likely malformed or truncated", .path.display())]
    MobiPanic { path: PathBuf },
    #[error("book file `{}` produced no readable text", .path.display())]
    EmptyContent { path: PathBuf },
    #[error("book file `{}` is encrypted; cdx does not handle PDF passwords or DRM", .path.display())]
    PdfEncrypted { path: PathBuf },
    #[error("failed to read pdf structure of `{}`: {source}", .path.display())]
    PdfStructure {
        path: PathBuf,
        #[source]
        source: lopdf::Error,
    },
    #[error("failed to extract text from pdf `{}`: {source}", .path.display())]
    Pdf {
        path: PathBuf,
        #[source]
        source: pdf_extract::OutputError,
    },
    #[error("pdf text extractor crashed reading `{}`; the file is likely malformed", .path.display())]
    PdfPanic { path: PathBuf },
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub struct Book {
    pub id: i64,
    pub title: String,
    pub author: Option<String>,
    pub chapters: Vec<Chapter>,
}

// serde derives exist because chapters cross the reader-cache file boundary
// (`reader::cache`); `Book` itself stays out of the cache so catalog metadata
// edits (title/author) never go stale on a cache hit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub title: String,
    /// Plain-text view of the chapter (no styles), used as the cursor's
    /// canonical coordinate space. Must equal the concatenation of every
    /// span across `lines`, joined by '\n' between logical lines.
    pub text: String,
    /// Pre-wrapped, paragraph-sized lines carrying styled spans. The reader
    /// reflows these to the active viewport width at render time.
    pub lines: Vec<StyledLine>,
}

impl Chapter {
    /// Build a chapter from a list of styled lines, keeping `text` in sync.
    pub fn from_lines(title: String, lines: Vec<StyledLine>) -> Self {
        let text = lines
            .iter()
            .map(StyledLine::plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        Self { title, text, lines }
    }

    /// Convenience for callers that only have a flat text body. Treats each
    /// '\n'-separated source line as its own unstyled `StyledLine`.
    pub fn from_text(title: String, text: String) -> Self {
        let lines: Vec<StyledLine> = text.split('\n').map(StyledLine::from_plain).collect();
        Self { title, text, lines }
    }
}

pub fn open(catalog_dir: &Path, book: &CatalogBook, render_width: usize) -> Result<Book> {
    let format = Format::parse_label(&book.format).ok_or_else(|| Error::UnsupportedFormat {
        format: book.format.clone(),
    })?;
    let abs_path = catalog_dir.join(&book.file_path);
    let chapters = match cache::is_cacheable(format)
        .then(|| cache::load(catalog_dir, book.id, &abs_path, render_width))
        .flatten()
    {
        Some(chapters) => chapters,
        None => {
            let chapters = read_chapters(&abs_path, format, render_width)?;
            if cache::is_cacheable(format) {
                cache::store(catalog_dir, book.id, &abs_path, render_width, &chapters);
            }
            chapters
        }
    };
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
        Format::Mobi | Format::Azw3 => mobi::read_mobi(path, render_width),
        // render_width only tunes html2text output; pdf text arrives already
        // line-broken and the TUI lay_out reflows it to the viewport.
        Format::Pdf => pdf::read_pdf(path),
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
        let tagged_lines =
            html2text::from_read_rich(&bytes[..], width).map_err(|source| Error::HtmlRender {
                title: title.clone(),
                source,
            })?;
        let styled_lines = tagged_lines_to_styled(&tagged_lines);
        chapters.push(Chapter::from_lines(title, styled_lines));
    }
    Ok(chapters)
}

/// Map html2text rich-rendered output to our `StyledLine` shape. Headings
/// arrive as plain text prefixed by `#` (per `RichDecorator::header_prefix`);
/// we strip that and tag the whole line with a heading level so the renderer
/// can apply a level-dependent style.
pub(crate) fn tagged_lines_to_styled(
    tagged: &[TaggedLine<Vec<RichAnnotation>>],
) -> Vec<StyledLine> {
    tagged.iter().map(tagged_line_to_styled).collect()
}

fn tagged_line_to_styled(line: &TaggedLine<Vec<RichAnnotation>>) -> StyledLine {
    let mut spans: Vec<StyledSpan> = Vec::new();
    for el in line.iter() {
        let TaggedLineElement::Str(ts) = el else {
            continue;
        };
        if ts.s.is_empty() {
            continue;
        }
        let style = annotations_to_style(&ts.tag);
        let span = StyledSpan {
            text: ts.s.clone(),
            style,
        };
        // Merge consecutive spans that share a style — keeps `Vec<Span>` short
        // and stops the renderer from emitting one `ratatui::Span` per char.
        if let Some(last) = spans.last_mut() {
            if last.style == span.style {
                last.text.push_str(&span.text);
                continue;
            }
        }
        spans.push(span);
    }

    let mut styled = StyledLine { spans };
    apply_heading_prefix(&mut styled);
    styled
}

fn annotations_to_style(tags: &[RichAnnotation]) -> TextStyle {
    let mut style = TextStyle::PLAIN;
    for tag in tags {
        match tag {
            RichAnnotation::Strong => style.bold = true,
            RichAnnotation::Emphasis => style.italic = true,
            RichAnnotation::Strikeout => style.strikethrough = true,
            RichAnnotation::Code => style.code = true,
            RichAnnotation::Preformat(_) => style.code = true,
            RichAnnotation::Link(_) => {
                style.link = true;
                style.underline = true;
            }
            // Default, Image (title text), Colour, BgColour — left as plain
            // for now. Colour/BgColour would override the terminal theme,
            // which the user controls; bringing book colours in feels wrong.
            _ => {}
        }
    }
    style
}

/// Strip the `# `-style heading prefix injected by `RichDecorator` and tag
/// every span on the line with the corresponding heading level.
fn apply_heading_prefix(line: &mut StyledLine) {
    let Some(first) = line.spans.first() else {
        return;
    };
    if !first.style.is_plain() {
        return;
    }
    let head = first.text.as_str();
    let hashes = head.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes > 6 {
        return;
    }
    let after_hashes: String = head.chars().skip(hashes).collect();
    if !after_hashes.starts_with(' ') {
        return;
    }
    let remaining: String = after_hashes.chars().skip(1).collect();
    line.spans[0].text = remaining;
    if line.spans[0].text.is_empty() {
        line.spans.remove(0);
    }
    let level = hashes as u8;
    for span in line.spans.iter_mut() {
        span.style.heading_level = Some(level);
    }
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
    Ok(vec![Chapter::from_text(title, text)])
}

fn read_md(path: &Path) -> Result<Vec<Chapter>> {
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let source = String::from_utf8(bytes).map_err(|_| Error::NotUtf8 {
        path: path.to_path_buf(),
    })?;
    let lines = markdown_to_lines(&source);
    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Document")
        .to_string();
    Ok(vec![Chapter::from_lines(title, lines)])
}

fn markdown_to_lines(source: &str) -> Vec<StyledLine> {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};

    let parser = Parser::new(source);
    let mut lines: Vec<StyledLine> = Vec::new();
    let mut current: Vec<StyledSpan> = Vec::new();
    let mut style_stack: Vec<TextStyle> = Vec::new();
    let mut heading_level: Option<u8> = None;
    let mut list_depth: usize = 0;
    let mut in_code_block = false;
    let mut blank_before_next_block = false;

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                flush_line(&mut lines, &mut current);
                emit_blank_if_pending(&mut lines, &mut blank_before_next_block);
                heading_level = Some(level as u8);
            }
            Event::End(TagEnd::Heading(_)) => {
                flush_line(&mut lines, &mut current);
                heading_level = None;
                blank_before_next_block = true;
            }
            Event::Start(Tag::Paragraph) => {
                flush_line(&mut lines, &mut current);
                emit_blank_if_pending(&mut lines, &mut blank_before_next_block);
            }
            Event::End(TagEnd::Paragraph) => {
                flush_line(&mut lines, &mut current);
                blank_before_next_block = true;
            }
            Event::Start(Tag::BlockQuote(_)) => {
                // BlockQuotes wrap their content in their own Paragraph(s);
                // pushing italic onto the style stack visually cues the quote
                // without us having to prefix every line.
                style_stack.push(TextStyle {
                    italic: true,
                    ..TextStyle::PLAIN
                });
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                style_stack.pop();
            }
            Event::Start(Tag::CodeBlock(_)) => {
                flush_line(&mut lines, &mut current);
                emit_blank_if_pending(&mut lines, &mut blank_before_next_block);
                in_code_block = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                flush_line(&mut lines, &mut current);
                in_code_block = false;
                blank_before_next_block = true;
            }
            Event::Start(Tag::List(_)) => {
                flush_line(&mut lines, &mut current);
                emit_blank_if_pending(&mut lines, &mut blank_before_next_block);
                list_depth += 1;
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                blank_before_next_block = true;
            }
            Event::Start(Tag::Item) => {
                flush_line(&mut lines, &mut current);
                let indent = "  ".repeat(list_depth.saturating_sub(1));
                let bullet = format!("{indent}- ");
                push_run(
                    &mut current,
                    &bullet,
                    resolve_style(&style_stack, None, false),
                );
            }
            Event::End(TagEnd::Item) => {
                flush_line(&mut lines, &mut current);
            }
            Event::Start(Tag::Strong) => style_stack.push(TextStyle {
                bold: true,
                ..TextStyle::PLAIN
            }),
            Event::End(TagEnd::Strong) => {
                style_stack.pop();
            }
            Event::Start(Tag::Emphasis) => style_stack.push(TextStyle {
                italic: true,
                ..TextStyle::PLAIN
            }),
            Event::End(TagEnd::Emphasis) => {
                style_stack.pop();
            }
            Event::Start(Tag::Strikethrough) => style_stack.push(TextStyle {
                strikethrough: true,
                ..TextStyle::PLAIN
            }),
            Event::End(TagEnd::Strikethrough) => {
                style_stack.pop();
            }
            Event::Start(Tag::Link { .. }) => style_stack.push(TextStyle {
                link: true,
                underline: true,
                ..TextStyle::PLAIN
            }),
            Event::End(TagEnd::Link) => {
                style_stack.pop();
            }
            Event::Text(t) => {
                let style = resolve_style(&style_stack, heading_level, in_code_block);
                if in_code_block && t.contains('\n') {
                    let chunks: Vec<&str> = t.split('\n').collect();
                    for (i, chunk) in chunks.iter().enumerate() {
                        push_run(&mut current, chunk, style);
                        if i + 1 < chunks.len() {
                            flush_line(&mut lines, &mut current);
                        }
                    }
                } else {
                    push_run(&mut current, &t, style);
                }
            }
            Event::Code(t) => {
                let style = resolve_style(&style_stack, heading_level, true);
                push_run(&mut current, &t, style);
            }
            Event::Html(t) | Event::InlineHtml(t) => {
                let style = resolve_style(&style_stack, heading_level, false);
                push_run(&mut current, &t, style);
            }
            Event::SoftBreak => {
                let style = resolve_style(&style_stack, heading_level, false);
                push_run(&mut current, " ", style);
            }
            Event::HardBreak => {
                flush_line(&mut lines, &mut current);
            }
            Event::Rule => {
                flush_line(&mut lines, &mut current);
                emit_blank_if_pending(&mut lines, &mut blank_before_next_block);
                push_run(&mut current, "───", TextStyle::PLAIN);
                flush_line(&mut lines, &mut current);
                blank_before_next_block = true;
            }
            _ => {}
        }
    }

    flush_line(&mut lines, &mut current);
    lines
}

fn resolve_style(stack: &[TextStyle], heading_level: Option<u8>, in_code_block: bool) -> TextStyle {
    let mut acc = TextStyle::PLAIN;
    for s in stack {
        if s.bold {
            acc.bold = true;
        }
        if s.italic {
            acc.italic = true;
        }
        if s.underline {
            acc.underline = true;
        }
        if s.strikethrough {
            acc.strikethrough = true;
        }
        if s.code {
            acc.code = true;
        }
        if s.link {
            acc.link = true;
        }
    }
    if in_code_block {
        acc.code = true;
    }
    if let Some(level) = heading_level {
        acc.heading_level = Some(level);
    }
    acc
}

fn push_run(current: &mut Vec<StyledSpan>, text: &str, style: TextStyle) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = current.last_mut() {
        if last.style == style {
            last.text.push_str(text);
            return;
        }
    }
    current.push(StyledSpan {
        text: text.to_string(),
        style,
    });
}

fn flush_line(lines: &mut Vec<StyledLine>, current: &mut Vec<StyledSpan>) {
    if current.is_empty() {
        return;
    }
    let spans = std::mem::take(current);
    lines.push(StyledLine { spans });
}

fn emit_blank_if_pending(lines: &mut Vec<StyledLine>, flag: &mut bool) {
    if *flag && !lines.is_empty() {
        lines.push(StyledLine::empty());
    }
    *flag = false;
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
    fn markdown_tags_headings_and_inline_emphasis() {
        let lines = markdown_to_lines("# Title\n\nFirst para.\n\nSecond *italic* para.\n");
        // The heading line has level=1 on every span and no `#` prefix.
        let heading = lines
            .iter()
            .find(|l| l.heading_level() == Some(1))
            .expect("heading present");
        assert_eq!(heading.plain_text(), "Title");
        // The "Second *italic*" paragraph has an italic span.
        let para = lines
            .iter()
            .find(|l| l.plain_text().contains("Second") && l.plain_text().contains("italic"))
            .expect("italic paragraph present");
        assert!(
            para.spans.iter().any(|s| s.style.italic),
            "italic span present in paragraph"
        );
    }

    #[test]
    fn markdown_renders_lists_with_bullets() {
        let lines = markdown_to_lines("- a\n- b\n- c\n");
        let plain: Vec<String> = lines.iter().map(StyledLine::plain_text).collect();
        assert!(plain.iter().any(|p| p == "- a"));
        assert!(plain.iter().any(|p| p == "- b"));
        assert!(plain.iter().any(|p| p == "- c"));
    }

    #[test]
    fn markdown_marks_strong_and_inline_code_with_their_styles() {
        let lines = markdown_to_lines("Plain **bold** and `code` here.\n");
        let line = &lines[0];
        assert!(
            line.spans.iter().any(|s| s.style.bold && s.text == "bold"),
            "bold span carries the bold flag"
        );
        assert!(
            line.spans.iter().any(|s| s.style.code && s.text == "code"),
            "code span carries the code flag"
        );
    }

    #[test]
    fn markdown_propagates_heading_level_through_inline_styles() {
        let lines = markdown_to_lines("## With **bold** word\n");
        let heading = lines
            .iter()
            .find(|l| l.heading_level() == Some(2))
            .expect("level-2 heading present");
        assert_eq!(heading.plain_text(), "With bold word");
        let bold = heading
            .spans
            .iter()
            .find(|s| s.style.bold)
            .expect("bold span inside heading");
        assert_eq!(bold.style.heading_level, Some(2));
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
    fn truncated_pdf_errors_out() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.pdf");
        std::fs::write(&path, b"%PDF-").unwrap();
        let err = read_chapters(&path, Format::Pdf, 80).unwrap_err();
        assert!(matches!(
            err,
            Error::PdfStructure { .. } | Error::Pdf { .. }
        ));
    }

    #[test]
    fn hrefs_match_strips_fragment_and_dot_prefix() {
        assert!(hrefs_match("ch1.xhtml", "ch1.xhtml#intro"));
        assert!(hrefs_match("./ch1.xhtml", "ch1.xhtml"));
        assert!(hrefs_match("OEBPS/ch1.xhtml", "ch1.xhtml"));
    }

    #[test]
    fn rich_pipeline_preserves_bold_italic_and_promotes_headings() {
        // html2text 0.15 maps the *semantic* tags (<strong>, <em>) to
        // Strong/Emphasis annotations. Presentational <b>/<i> currently
        // come through as plain text — most modern EPUBs use the semantic
        // form, so we test against that.
        let html = b"<h1>Big Title</h1>\
                     <p>plain <strong>bold</strong> and <em>italic</em> \
                     and <code>code</code> here</p>";
        let tagged = html2text::from_read_rich(&html[..], 200).unwrap();
        let styled = tagged_lines_to_styled(&tagged);

        // The heading line should have its prefix stripped and every span
        // tagged with heading_level=1.
        let heading = styled
            .iter()
            .find(|l| l.heading_level() == Some(1))
            .expect("heading line tagged as level 1");
        assert!(heading.plain_text().contains("Big Title"));
        assert!(!heading.plain_text().starts_with('#'));

        // Paragraph line carries bold, italic and code spans.
        let para = styled
            .iter()
            .find(|l| l.plain_text().contains("bold") && l.plain_text().contains("italic"))
            .expect("paragraph line present");
        assert!(
            para.spans.iter().any(|s| s.style.bold),
            "at least one span is bold"
        );
        assert!(
            para.spans.iter().any(|s| s.style.italic),
            "at least one span is italic"
        );
        assert!(
            para.spans.iter().any(|s| s.style.code),
            "at least one span is code"
        );
    }
}
