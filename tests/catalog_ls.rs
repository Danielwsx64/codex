mod common;

use std::fs;

use common::Fixture;
use predicates::str::contains;

#[test]
fn ls_empty_registry_prints_hint() {
    let f = Fixture::new();
    f.cdx()
        .args(["catalog", "ls"])
        .assert()
        .success()
        .stdout(contains("No catalogs registered"));
}

#[test]
fn ls_empty_registry_jsonl_emits_nothing() {
    let f = Fixture::new();
    let out = f.cdx_json().args(["catalog", "ls"]).assert().success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(stdout.is_empty(), "expected empty stdout, got: {stdout:?}");
}

#[test]
fn ls_jsonl_emits_one_object_per_line() {
    let f = Fixture::new();
    let lib1 = f.lib_path("a");
    let lib2 = f.lib_path("b");
    f.cdx()
        .args(["catalog", "init", "a"])
        .arg(&lib1)
        .assert()
        .success();
    f.cdx()
        .args(["catalog", "init", "b", "--no-switch"])
        .arg(&lib2)
        .assert()
        .success();

    let out = f.cdx_json().args(["catalog", "ls"]).assert().success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(lines.len(), 2);
    for line in &lines {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(v.get("name").is_some());
        assert!(v.get("path").is_some());
    }
}

#[test]
fn ls_marks_missing_when_dir_was_deleted() {
    let f = Fixture::new();
    let lib = f.lib_path("gone");
    f.cdx()
        .args(["catalog", "init", "gone"])
        .arg(&lib)
        .assert()
        .success();
    fs::remove_dir_all(&lib).unwrap();

    f.cdx()
        .args(["catalog", "ls"])
        .assert()
        .success()
        .stdout(contains("(missing)"));
}
