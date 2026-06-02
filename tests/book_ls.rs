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

#[test]
fn ls_columns_selects_and_orders_human_output() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    let out = f
        .cdx()
        .args(["ls", "--columns", "title,id,embed"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2);
    let header = lines[0];
    let title_pos = header.find("TITLE").expect("TITLE column");
    let id_pos = header.find("ID").expect("ID column");
    let embed_pos = header.find("EMBED").expect("EMBED column");
    assert!(
        title_pos < id_pos && id_pos < embed_pos,
        "columns must appear in requested order: {header}"
    );
    // Embed column shows the persisted status word.
    assert!(lines[1].contains("pending"));
}

#[test]
fn ls_columns_filters_json_to_chosen_keys() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    let out = f
        .cdx_json()
        .args(["ls", "--columns", "id,title,embed"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let line = text.lines().next().expect("one line");
    let v: serde_json::Value = serde_json::from_str(line).unwrap();
    let obj = v.as_object().expect("object");
    let keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["id", "title", "embed"]);
    assert_eq!(obj["embed"], "pending");
    assert!(obj.get("author").is_none(), "author must be filtered out");
}

#[test]
fn ls_all_columns_emits_every_slug() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    let out = f
        .cdx_json()
        .args(["ls", "--all-columns"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let line = text.lines().next().expect("one line");
    let v: serde_json::Value = serde_json::from_str(line).unwrap();
    let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    assert!(keys.contains(&"embed"));
    assert!(keys.contains(&"series"));
    assert!(keys.contains(&"rating"));
    assert!(keys.contains(&"format"));
}

#[test]
fn ls_columns_unknown_slug_errors() {
    let f = Fixture::new();
    f.init_lib();

    f.cdx()
        .args(["ls", "--columns", "title,bogus,id"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown column `bogus`"))
        .stderr(predicate::str::contains("title"));
}

#[test]
fn ls_columns_and_all_columns_are_mutually_exclusive() {
    let f = Fixture::new();
    f.init_lib();

    // clap should reject the combination at parse time.
    f.cdx()
        .args(["ls", "--columns", "id", "--all-columns"])
        .assert()
        .failure();
}
