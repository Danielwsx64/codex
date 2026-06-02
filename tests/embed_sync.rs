mod common;

use common::Fixture;
use predicates::prelude::*;
use rusqlite::params;

fn setup_with_books(f: &Fixture) {
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .arg(Fixture::fixture("sample.pdf"))
        .assert()
        .success();
}

#[test]
fn sync_with_nothing_pending_reports_and_exits_zero() {
    let f = Fixture::new();
    setup_with_books(&f);
    // Mark every book synced so the queue is empty.
    let conn = rusqlite::Connection::open(f.lib_path("lib").join("catalog.db")).unwrap();
    conn.execute(
        "UPDATE books SET embed_status='synced', embed_synced_at=datetime('now')",
        [],
    )
    .unwrap();
    drop(conn);

    f.cdx()
        .args(["embed", "sync"])
        .assert()
        .success()
        .stdout(predicate::str::contains("nothing to sync"));
}

#[test]
fn sync_embeds_pending_and_flips_status_to_synced() {
    let f = Fixture::new();
    setup_with_books(&f);

    f.cdx()
        .args(["embed", "sync"])
        .assert()
        .success()
        .stdout(predicate::str::contains("synced"))
        .stdout(predicate::str::contains("Done:"));

    let conn = rusqlite::Connection::open(f.lib_path("lib").join("catalog.db")).unwrap();
    let statuses: Vec<String> = conn
        .prepare("SELECT embed_status FROM books ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(statuses, vec!["synced", "synced"]);

    // Second run: no work left.
    f.cdx()
        .args(["embed", "sync"])
        .assert()
        .success()
        .stdout(predicate::str::contains("nothing to sync"));
}

#[test]
fn sync_marks_mobi_as_unsupported_and_skips_next_time() {
    let f = Fixture::new();
    f.init_lib();
    // Insert a MOBI row directly — we don't ship a real mobi fixture.
    let conn = rusqlite::Connection::open(f.lib_path("lib").join("catalog.db")).unwrap();
    conn.execute(
        "INSERT INTO books (title, author, format, file_path) VALUES (?1, ?2, 'mobi', ?3)",
        params!["Old", "A", "books/1/A_-_Old.mobi"],
    )
    .unwrap();
    drop(conn);

    f.cdx()
        .args(["embed", "sync"])
        .assert()
        .success()
        .stdout(predicate::str::contains("unsupported"));

    let conn = rusqlite::Connection::open(f.lib_path("lib").join("catalog.db")).unwrap();
    let status: String = conn
        .query_row("SELECT embed_status FROM books WHERE id=1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(status, "unsupported");

    // Second run should report nothing pending — unsupported is terminal.
    f.cdx()
        .args(["embed", "sync"])
        .assert()
        .success()
        .stdout(predicate::str::contains("nothing to sync"));
}
