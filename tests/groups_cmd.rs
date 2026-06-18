mod common;

use common::Fixture;
use predicates::prelude::*;

// Insert a book row directly so tests control author/rating precisely, the way
// `dedup_cmd` does — the `cdx add` pipeline derives metadata from the file.
fn insert_book(
    lib: &std::path::Path,
    title: &str,
    author: Option<&str>,
    rating: Option<i64>,
) -> i64 {
    let conn = codex::catalog::open_existing(lib).expect("open catalog");
    conn.execute(
        "INSERT INTO books (title, author, format, file_path, rating)
         VALUES (?1, ?2, 'epub', ?3, ?4)",
        rusqlite::params![title, author, format!("books/x/{title}.epub"), rating],
    )
    .expect("insert book");
    conn.last_insert_rowid()
}

fn add_tag(lib: &std::path::Path, book_id: i64, name: &str) {
    let conn = codex::catalog::open_existing(lib).expect("open catalog");
    conn.execute(
        "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
        rusqlite::params![name],
    )
    .unwrap();
    let tag_id: i64 = conn
        .query_row(
            "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
            rusqlite::params![name],
            |r| r.get(0),
        )
        .unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO book_tags (book_id, tag_id) VALUES (?1, ?2)",
        rusqlite::params![book_id, tag_id],
    )
    .unwrap();
}

#[test]
fn groups_by_author_human_lists_value_and_count() {
    let f = Fixture::new();
    let lib = f.init_lib();
    insert_book(&lib, "Emma", Some("Jane Austen"), None);
    insert_book(&lib, "Persuasion", Some("Jane Austen"), None);
    insert_book(&lib, "Frankenstein", Some("Mary Shelley"), None);
    insert_book(&lib, "Anon", None, None);

    f.cdx()
        .args(["groups", "--by", "author"])
        .assert()
        .success()
        .stdout(predicate::str::contains("GROUP"))
        .stdout(predicate::str::contains("COUNT"))
        .stdout(predicate::str::contains("Jane Austen"))
        .stdout(predicate::str::contains("Mary Shelley"))
        // The catch-all group for the author-less book.
        .stdout(predicate::str::contains("(no author)"));
}

#[test]
fn groups_by_author_json_emits_one_object_per_group() {
    let f = Fixture::new();
    let lib = f.init_lib();
    insert_book(&lib, "Emma", Some("Jane Austen"), None);
    insert_book(&lib, "Persuasion", Some("Jane Austen"), None);
    insert_book(&lib, "Anon", None, None);

    let out = f
        .cdx_json()
        .args(["groups", "--by", "author"])
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "one object per group (named + catch-all)");

    let austen: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(austen["value"], "Jane Austen");
    assert_eq!(austen["count"], 2);

    // The author-less group serializes value as JSON null.
    let none: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert!(none["value"].is_null());
    assert_eq!(none["count"], 1);
}

#[test]
fn groups_by_tag_counts_a_book_in_each_of_its_tags() {
    let f = Fixture::new();
    let lib = f.init_lib();
    let a = insert_book(&lib, "A", Some("X"), None);
    let b = insert_book(&lib, "B", Some("Y"), None);
    insert_book(&lib, "C", Some("Z"), None); // untagged
    add_tag(&lib, a, "fiction");
    add_tag(&lib, a, "sci-fi");
    add_tag(&lib, b, "fiction");

    let out = f
        .cdx_json()
        .args(["groups", "--by", "tag"])
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3, "fiction, sci-fi, untagged");

    let fiction: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(fiction["value"], "fiction");
    assert_eq!(fiction["count"], 2, "book A and B both tagged fiction");

    let untagged: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
    assert!(untagged["value"].is_null());
    assert_eq!(untagged["count"], 1);
}

#[test]
fn groups_by_rating_groups_scores_with_unrated_catch_all() {
    let f = Fixture::new();
    let lib = f.init_lib();
    insert_book(&lib, "A", Some("X"), Some(5));
    insert_book(&lib, "B", Some("Y"), Some(5));
    insert_book(&lib, "C", Some("Z"), Some(3));
    insert_book(&lib, "D", Some("W"), None);

    let out = f
        .cdx_json()
        .args(["groups", "--by", "rating"])
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 3);

    let five: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(five["value"], "5");
    assert_eq!(five["count"], 2);

    let unrated: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
    assert!(unrated["value"].is_null());
    assert_eq!(unrated["count"], 1);

    f.cdx()
        .args(["groups", "--by", "rating"])
        .assert()
        .success()
        .stdout(predicate::str::contains("(unrated)"));
}

#[test]
fn groups_empty_catalog_json_is_silent() {
    let f = Fixture::new();
    f.init_lib();

    f.cdx_json()
        .args(["groups", "--by", "author"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    f.cdx()
        .args(["groups", "--by", "author"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No groups found."));
}

#[test]
fn groups_requires_by_flag() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx().arg("groups").assert().failure();
}
