mod common;

use common::Fixture;
use predicates::prelude::*;

fn embed_status_for(f: &Fixture, id: &str) -> String {
    let out = f
        .cdx_json()
        .args(["ls", "--columns", "id,embed"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let want: i64 = id.parse().expect("test passes numeric id strings");
    for line in text.lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        if v["id"].as_i64() == Some(want) {
            return v["embed"].as_str().unwrap().to_string();
        }
    }
    panic!("book id {id} not found in ls output:\n{text}");
}

fn series_for(f: &Fixture, id: &str) -> (Option<String>, Option<f64>) {
    let out = f.cdx_json().args(["inspect", id]).assert().success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let line = text.lines().next().expect("inspect emits one jsonl line");
    let v: serde_json::Value = serde_json::from_str(line).unwrap();
    let name = v["series_name"].as_str().map(str::to_string);
    let idx = v["series_index"].as_f64();
    (name, idx)
}

fn mark_synced(f: &Fixture, id: &str) {
    f.cdx().args(["embed", "sync"]).assert().success();
    assert_eq!(embed_status_for(f, id), "synced");
}

#[test]
fn series_sets_name_and_index_and_marks_pending() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
    mark_synced(&f, "1");

    f.cdx()
        .args(["series", "1", "Foundation", "--index", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Set series for book 1"))
        .stdout(predicate::str::contains("Foundation #2"));

    let (name, idx) = series_for(&f, "1");
    assert_eq!(name.as_deref(), Some("Foundation"));
    assert_eq!(idx, Some(2.0));
    assert_eq!(embed_status_for(&f, "1"), "pending");
}

#[test]
fn series_preserves_index_when_only_name_changes() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
    f.cdx()
        .args(["series", "1", "Foundation", "--index", "2"])
        .assert()
        .success();
    mark_synced(&f, "1");

    f.cdx()
        .args(["series", "1", "Second Foundation"])
        .assert()
        .success();
    let (name, idx) = series_for(&f, "1");
    assert_eq!(name.as_deref(), Some("Second Foundation"));
    assert_eq!(idx, Some(2.0));
    assert_eq!(embed_status_for(&f, "1"), "pending");
}

#[test]
fn series_clear_removes_both_columns() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
    f.cdx()
        .args(["series", "1", "Foundation", "--index", "2"])
        .assert()
        .success();
    mark_synced(&f, "1");

    f.cdx()
        .args(["series", "1", "--clear"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Cleared series for book 1"));

    let (name, idx) = series_for(&f, "1");
    assert!(name.is_none());
    assert!(idx.is_none());
    assert_eq!(embed_status_for(&f, "1"), "pending");
}

#[test]
fn series_noop_preserves_embed_status() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
    f.cdx()
        .args(["series", "1", "Foundation", "--index", "2"])
        .assert()
        .success();
    mark_synced(&f, "1");

    f.cdx()
        .args(["series", "1", "Foundation", "--index", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("series unchanged"));
    assert_eq!(embed_status_for(&f, "1"), "synced");
}

#[test]
fn series_clear_conflicts_with_name_and_index() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    f.cdx()
        .args(["series", "1", "Foundation", "--clear"])
        .assert()
        .failure();
    f.cdx()
        .args(["series", "1", "--clear", "--index", "2"])
        .assert()
        .failure();
}

#[test]
fn series_without_name_or_clear_errors() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    f.cdx().args(["series", "1"]).assert().failure();
}

#[test]
fn series_jsonl_includes_previous_and_embed_status() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
    f.cdx()
        .args(["series", "1", "Foundation", "--index", "1"])
        .assert()
        .success();

    let out = f
        .cdx_json()
        .args(["series", "1", "Foundation", "--index", "2"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let line = text.lines().next().expect("one line");
    let v: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(v["action"], "series");
    assert_eq!(v["id"], 1);
    assert_eq!(v["series_name"], "Foundation");
    assert_eq!(v["series_index"], 2.0);
    assert_eq!(v["previous_series_name"], "Foundation");
    assert_eq!(v["previous_series_index"], 1.0);
    assert_eq!(v["changed"], true);
    assert_eq!(v["embed_status"], "pending");
}
