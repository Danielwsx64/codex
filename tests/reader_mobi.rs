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
fn open_sample_mobi_via_reader_domain_returns_chapters() {
    let (_tmp, dir) = make_catalog();
    let sample = common::Fixture::fixture("sample.mobi");
    let book = seed_book_at(&dir, &sample, "mobi");

    let loaded = reader::open(&dir, &book, tui_reader::HTML_RENDER_WIDTH).expect("open book");
    assert_eq!(loaded.id, book.id);
    // The fixture carries <mbp:pagebreak/> markers, so the split must
    // produce more than one chapter, each titled "Chapter N".
    assert!(
        loaded.chapters.len() > 1,
        "sample mobi splits into multiple chapters, got {}",
        loaded.chapters.len()
    );
    assert!(loaded.chapters.iter().any(|c| !c.text.trim().is_empty()));
    assert_eq!(loaded.chapters[0].title, "Chapter 1");
}

#[test]
fn open_kf8_only_azw3_fails_with_clear_message() {
    let (_tmp, dir) = make_catalog();
    // Calibre emits KF8-only AZW3 (no legacy KF7 stream), which mobi 0.8
    // cannot decode — the reader must say so instead of showing a blank book.
    let sample = common::Fixture::fixture("sample.azw3");
    let book = seed_book_at(&dir, &sample, "azw3");

    let err = reader::open(&dir, &book, tui_reader::HTML_RENDER_WIDTH).unwrap_err();
    match err {
        reader::Error::Azw3NoLegacyStream { .. } => {}
        other => panic!("expected Azw3NoLegacyStream, got {other:?}"),
    }
}
