mod common;

use common::Fixture;
use predicates::prelude::*;

#[test]
fn ls_human_empty_prints_hint() {
    let f = Fixture::new();
    f.init_lib();

    f.cdx()
        .arg("ls")
        .assert()
        .success()
        .stdout(predicate::str::contains("No books"));
}

#[test]
fn ls_jsonl_empty_emits_nothing() {
    let f = Fixture::new();
    f.init_lib();

    let out = f.cdx_json().arg("ls").assert().success();
    assert!(
        out.get_output().stdout.is_empty(),
        "empty JSONL must emit zero bytes"
    );
}

#[test]
fn ls_after_add_lists_books_sorted_by_title() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.pdf"))
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    let out = f.cdx().arg("ls").assert().success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    // header + 2 rows.
    assert_eq!(lines.len(), 3);
    // First row title is "Sample Book" (epub) which sorts before "Sample PDF Title".
    assert!(lines[1].contains("Sample Book"));
    assert!(lines[2].contains("Sample PDF Title"));
}

#[test]
fn ls_jsonl_one_object_per_line() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    let out = f.cdx_json().arg("ls").assert().success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 1);
    let v: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(v["title"], "Sample Book");
    assert_eq!(v["author"], "Jane Doe");
    assert_eq!(v["format"], "epub");
}
