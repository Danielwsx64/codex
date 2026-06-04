mod common;

use std::path::PathBuf;

use codex::catalog::books::{Book, EmbedStatus};
use codex::reader;
use codex::tui::reader as tui_reader;

fn seed_book_at(catalog_dir: &std::path::Path, src: &std::path::Path, format: &str) -> Book {
    let conn = codex::catalog::open(catalog_dir).expect("open catalog");
    let stored_rel = format!("books/1/sample.{format}");
    let abs = catalog_dir.join(&stored_rel);
    std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
    std::fs::copy(src, &abs).unwrap();
    conn.execute(
        "INSERT INTO books (title, author, format, file_path) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params!["Sample Book", "Jane Doe", format, stored_rel],
    )
    .unwrap();
    let id = conn.last_insert_rowid();
    Book {
        id,
        title: "Sample Book".to_string(),
        author: Some("Jane Doe".to_string()),
        format: format.to_string(),
        file_path: stored_rel,
        added_at: String::new(),
        description: None,
        series_name: None,
        series_index: None,
        rating: None,
        isbn: None,
        publisher: None,
        language: None,
        published_date: None,
        tags: Vec::new(),
        embed_status: EmbedStatus::Synced,
        embed_synced_at: None,
    }
}

fn make_catalog() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("lib");
    let _ = codex::catalog::init(&dir).expect("init catalog");
    (tmp, dir)
}

#[test]
fn open_sample_pdf_via_reader_domain_returns_page_chapters() {
    let (_tmp, dir) = make_catalog();
    let sample = common::Fixture::fixture("sample_text.pdf");
    let book = seed_book_at(&dir, &sample, "pdf");

    let loaded = reader::open(&dir, &book, tui_reader::HTML_RENDER_WIDTH).expect("open book");
    assert_eq!(loaded.id, book.id);
    assert_eq!(loaded.chapters.len(), 2);
    assert_eq!(loaded.chapters[0].title, "Page 1");
    assert!(loaded.chapters[0]
        .text
        .contains("First page of the sample book."));
    assert_eq!(loaded.chapters[1].title, "Page 2");
    assert!(loaded.chapters[1]
        .text
        .contains("Second page with more text."));
}

#[test]
fn pdf_without_extractable_text_reports_empty_content() {
    let (_tmp, dir) = make_catalog();
    // sample.pdf declares no /Font resource, so pdf-extract yields no text;
    // the reader must refuse instead of showing a blank book.
    let sample = common::Fixture::fixture("sample.pdf");
    let book = seed_book_at(&dir, &sample, "pdf");

    let err = reader::open(&dir, &book, tui_reader::HTML_RENDER_WIDTH).unwrap_err();
    match err {
        reader::Error::EmptyContent { .. } => {}
        other => panic!("expected EmptyContent, got {other:?}"),
    }
}

#[test]
fn open_garbage_pdf_fails_cleanly() {
    let (_tmp, dir) = make_catalog();
    let tmp = tempfile::tempdir().unwrap();
    let junk = tmp.path().join("junk.pdf");
    std::fs::write(&junk, b"this is not a pdf").unwrap();
    let book = seed_book_at(&dir, &junk, "pdf");

    let err = reader::open(&dir, &book, tui_reader::HTML_RENDER_WIDTH).unwrap_err();
    match err {
        reader::Error::PdfStructure { .. } => {}
        other => panic!("expected PdfStructure, got {other:?}"),
    }
}
