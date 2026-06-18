mod common;

use common::Fixture;
use predicates::prelude::*;

// Insert a book row directly into the catalog DB (no file on disk, no hash).
// Used to craft metadata-only duplicates the `cdx add` pipeline can't produce.
fn insert_book(lib: &std::path::Path, title: &str, author: &str, format: &str, added_at: &str) {
    let conn = codex::catalog::open_existing(lib).expect("open catalog");
    conn.execute(
        "INSERT INTO books (title, author, format, file_path, added_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        [
            title,
            author,
            format,
            &format!("books/x/{title}.{format}"),
            added_at,
        ],
    )
    .expect("insert book");
}

#[test]
fn dedup_lists_hash_identical_group() {
    let f = Fixture::new();
    let lib = f.init_lib();

    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
    f.cdx()
        .args(["add", "--force"])
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
    assert!(lib.join("books/2").is_dir());

    f.cdx()
        .arg("dedup")
        .assert()
        .success()
        .stdout(predicate::str::contains("Group 1"))
        .stdout(predicate::str::contains("identical hash"))
        .stdout(predicate::str::contains("suggest removing"));
}

#[test]
fn dedup_json_emits_one_object_per_group() {
    let f = Fixture::new();
    f.init_lib();

    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
    f.cdx()
        .args(["add", "--force"])
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    let out = f.cdx_json().arg("dedup").assert().success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1, "one JSON object per group");
    let obj: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(obj["reason"], "identical_hash");
    assert!(obj["linked_by"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("hash")));
    let members = obj["members"].as_array().unwrap();
    assert_eq!(members.len(), 2);
    let suggested = members.iter().filter(|m| m["suggested"] == true).count();
    assert_eq!(suggested, 1);
}

#[test]
fn dedup_yes_removes_suggested_copy() {
    let f = Fixture::new();
    let lib = f.init_lib();

    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
    f.cdx()
        .args(["add", "--force"])
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    f.cdx()
        .args(["dedup", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed book"));

    // One copy is gone; the other survives.
    let out = f.cdx_json().arg("ls").assert().success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert_eq!(stdout.lines().count(), 1, "exactly one book should remain");
    let remaining = lib.join("books/1").is_dir() ^ lib.join("books/2").is_dir();
    assert!(remaining, "exactly one of the two book dirs should remain");
}

#[test]
fn dedup_keep_moves_file_to_cwd() {
    let f = Fixture::new();
    f.init_lib();
    let cwd = f.work_dir.path().join("out");
    std::fs::create_dir_all(&cwd).unwrap();

    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
    f.cdx()
        .args(["add", "--force"])
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    f.cdx()
        .current_dir(&cwd)
        .args(["dedup", "--yes", "--keep"])
        .assert()
        .success();

    let kept = std::fs::read_dir(&cwd)
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| e.path().extension().is_some_and(|ext| ext == "epub"));
    assert!(kept, "--keep should leave an .epub in the cwd");
}

#[test]
fn dedup_meta_groups_only_under_meta_or_all() {
    let f = Fixture::new();
    let lib = f.init_lib();
    // Two metadata-identical books, different formats, no shared hash.
    insert_book(
        &lib,
        "Dune",
        "Frank Herbert",
        "epub",
        "2024-01-01T00:00:00Z",
    );
    insert_book(&lib, "Dune", "Frank Herbert", "pdf", "2024-01-02T00:00:00Z");

    // Hash signal alone finds nothing.
    f.cdx_json()
        .args(["dedup", "--by", "hash"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    // Meta (and the default union) group them.
    for args in [
        vec!["dedup", "--by", "meta"],
        vec!["dedup", "--by", "all"],
        vec!["dedup"],
    ] {
        let out = f.cdx_json().args(&args).assert().success();
        let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
        let lines: Vec<&str> = stdout.lines().collect();
        assert_eq!(lines.len(), 1, "{args:?} should report one group");
        let obj: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(obj["reason"], "older");
        assert_eq!(obj["linked_by"], serde_json::json!(["meta"]));
    }
}

#[test]
fn dedup_empty_catalog_json_is_silent() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    // No duplicates: JSON prints nothing, human is friendly.
    f.cdx_json()
        .arg("dedup")
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
    f.cdx()
        .arg("dedup")
        .assert()
        .success()
        .stdout(predicate::str::contains("No duplicate books found."));
}

#[test]
fn dedup_rm_without_tty_or_yes_fails() {
    let f = Fixture::new();
    let lib = f.init_lib();
    insert_book(
        &lib,
        "Dune",
        "Frank Herbert",
        "epub",
        "2024-01-01T00:00:00Z",
    );
    insert_book(&lib, "Dune", "Frank Herbert", "pdf", "2024-01-02T00:00:00Z");

    // `--rm` needs an interactive terminal; piped stdout must bail, not delete.
    f.cdx()
        .args(["dedup", "--rm"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("terminal"));
}
