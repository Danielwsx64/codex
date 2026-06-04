mod common;

use std::path::PathBuf;

use codex::catalog::books::{self, Book, EmbedStatus};
use codex::config::ReaderSettings;
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
    common::isolate_reader_cache();
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("lib");
    let _ = codex::catalog::init(&dir).expect("init catalog");
    (tmp, dir)
}

#[test]
fn open_sample_epub_via_reader_domain_returns_chapters() {
    let (_tmp, dir) = make_catalog();
    let sample = common::Fixture::fixture("sample.epub");
    let book = seed_book_at(&dir, &sample, "epub");

    let loaded = reader::open(&dir, &book, tui_reader::HTML_RENDER_WIDTH).expect("open book");
    assert_eq!(loaded.id, book.id);
    assert!(
        !loaded.chapters.is_empty(),
        "sample epub has at least one chapter"
    );
    assert!(loaded.chapters.iter().any(|c| !c.text.trim().is_empty()));
}

#[test]
fn tui_reader_state_persists_and_restores_progress() {
    let (_tmp, dir) = make_catalog();
    let sample = common::Fixture::fixture("sample.epub");
    let book = seed_book_at(&dir, &sample, "epub");

    // First session: open the reader, advance the cursor manually, save progress.
    let mut s = tui_reader::open_book(dir.clone(), &book, ReaderSettings::default())
        .expect("open book in tui reader");
    // Compose a layout at a known width so we can move the cursor predictably.
    s.layout_width = 40;
    s.page_height = 5;
    s.cursor_offset = 25;
    {
        let conn = codex::catalog::open_existing(&dir).unwrap();
        books::update_reading_progress(
            &conn,
            book.id,
            books::ReadingProgress {
                chapter: 0,
                offset: 25,
            },
        )
        .unwrap();
    }
    drop(s);

    // Second session: open the reader fresh; the constructor should restore
    // the chapter+offset from the catalog.
    let restored =
        tui_reader::open_book(dir.clone(), &book, ReaderSettings::default()).expect("reopen book");
    assert_eq!(restored.current_chapter, 0);
    assert_eq!(restored.cursor_offset, 25);
}

#[test]
fn reader_open_rejects_unknown_format() {
    let (_tmp, dir) = make_catalog();
    // Every catalog format now opens in the reader; an unknown format label
    // (e.g. a row written by a future cdx) is what UnsupportedFormat guards.
    let sample = common::Fixture::fixture("sample.pdf");
    let book = seed_book_at(&dir, &sample, "djvu");
    let err = reader::open(&dir, &book, 80).unwrap_err();
    match err {
        reader::Error::UnsupportedFormat { format } => assert_eq!(format, "djvu"),
        other => panic!("expected UnsupportedFormat, got {other:?}"),
    }
}

#[test]
fn reader_open_txt_returns_single_chapter() {
    let (_tmp, dir) = make_catalog();
    // Create a tiny TXT file inside the catalog's books dir.
    let rel = "books/1/notes.txt";
    let abs = dir.join(rel);
    std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
    std::fs::write(&abs, b"line one\nline two\nline three").unwrap();
    let conn = codex::catalog::open(&dir).unwrap();
    conn.execute(
        "INSERT INTO books (title, author, format, file_path) VALUES ('Notes', 'me', 'txt', ?1)",
        rusqlite::params![rel],
    )
    .unwrap();
    let id = conn.last_insert_rowid();
    let book = Book {
        id,
        title: "Notes".into(),
        author: Some("me".into()),
        format: "txt".into(),
        file_path: rel.to_string(),
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
        embed_status: EmbedStatus::Unsupported,
        embed_synced_at: None,
    };
    let loaded = reader::open(&dir, &book, 80).unwrap();
    assert_eq!(loaded.chapters.len(), 1);
    assert!(loaded.chapters[0].text.contains("line one"));
}
