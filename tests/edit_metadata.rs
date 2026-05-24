mod common;

use common::Fixture;
use predicates::prelude::*;

#[test]
fn ls_human_has_tags_column() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    f.cdx()
        .arg("ls")
        .assert()
        .success()
        .stdout(predicate::str::contains("TAGS"))
        .stdout(predicate::str::contains("Sample Book"));
}

#[test]
fn ls_jsonl_includes_tags_array_field() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    let out = f.cdx_json().arg("ls").assert().success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let line = text.lines().next().expect("one line");
    let v: serde_json::Value = serde_json::from_str(line).unwrap();
    assert!(v["tags"].is_array(), "tags must be an array, got {v}");
    assert_eq!(v["title"], "Sample Book");
}

#[test]
fn inspect_jsonl_includes_tags_array_field() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    let out = f.cdx_json().arg("inspect").arg("1").assert().success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let line = text.lines().next().expect("one line");
    let v: serde_json::Value = serde_json::from_str(line).unwrap();
    assert!(v["tags"].is_array());
    assert_eq!(v["title"], "Sample Book");
}
