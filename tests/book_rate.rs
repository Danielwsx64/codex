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

fn rating_for(f: &Fixture, id: &str) -> Option<i64> {
    let out = f
        .cdx_json()
        .args(["ls", "--columns", "id,rating"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let want: i64 = id.parse().expect("test passes numeric id strings");
    for line in text.lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        if v["id"].as_i64() == Some(want) {
            return v["rating"].as_i64();
        }
    }
    panic!("book id {id} not found in ls output:\n{text}");
}

fn mark_synced(f: &Fixture, id: &str) {
    f.cdx().args(["embed", "sync"]).assert().success();
    assert_eq!(embed_status_for(f, id), "synced");
}

#[test]
fn rate_sets_value_and_marks_pending() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
    mark_synced(&f, "1");

    f.cdx()
        .args(["rate", "1", "4"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Rated book 1"))
        .stdout(predicate::str::contains("★★★★☆"));

    assert_eq!(rating_for(&f, "1"), Some(4));
    assert_eq!(embed_status_for(&f, "1"), "pending");
}

#[test]
fn rate_zero_clears_existing_rating() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
    f.cdx().args(["rate", "1", "3"]).assert().success();
    mark_synced(&f, "1");

    f.cdx()
        .args(["rate", "1", "0"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Cleared rating for book 1"));

    assert_eq!(rating_for(&f, "1"), None);
    assert_eq!(embed_status_for(&f, "1"), "pending");
}

#[test]
fn rate_noop_preserves_embed_status() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
    f.cdx().args(["rate", "1", "4"]).assert().success();
    mark_synced(&f, "1");

    f.cdx()
        .args(["rate", "1", "4"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rating unchanged"));
    assert_eq!(embed_status_for(&f, "1"), "synced");
}

#[test]
fn rate_rejects_out_of_range_value() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    f.cdx().args(["rate", "1", "6"]).assert().failure();
    f.cdx().args(["rate", "1", "abc"]).assert().failure();
}

#[test]
fn rate_jsonl_includes_previous_and_embed_status() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
    f.cdx().args(["rate", "1", "2"]).assert().success();

    let out = f.cdx_json().args(["rate", "1", "5"]).assert().success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let line = text.lines().next().expect("one line");
    let v: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(v["action"], "rate");
    assert_eq!(v["id"], 1);
    assert_eq!(v["rating"], 5);
    assert_eq!(v["previous_rating"], 2);
    assert_eq!(v["changed"], true);
    assert_eq!(v["embed_status"], "pending");
}

#[test]
fn rate_resolves_target_by_title() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    f.cdx()
        .args(["rate", "sample book", "3"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Rated book 1"));
}
