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

fn mark_synced(f: &Fixture, id: &str) {
    // Drive embed sync through the public CLI so the test exercises the same
    // code path used in production. The sample.epub fixture is supported.
    f.cdx().args(["embed", "sync"]).assert().success();
    assert_eq!(embed_status_for(f, id), "synced");
}

#[test]
fn tag_adds_new_and_marks_pending() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
    mark_synced(&f, "1");

    f.cdx()
        .args(["tag", "1", "sci-fi", "classic"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Tagged book 1"))
        .stdout(predicate::str::contains("+sci-fi"))
        .stdout(predicate::str::contains("+classic"));

    assert_eq!(embed_status_for(&f, "1"), "pending");
}

#[test]
fn tag_jsonl_reports_added_and_already_present() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    f.cdx().args(["tag", "1", "sci-fi"]).assert().success();

    let out = f
        .cdx_json()
        .args(["tag", "1", "sci-fi", "epic"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let line = text.lines().next().expect("one line");
    let v: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(v["action"], "tag");
    assert_eq!(v["id"], 1);
    assert_eq!(v["added"], serde_json::json!(["epic"]));
    assert_eq!(v["already_present"], serde_json::json!(["sci-fi"]));
}

#[test]
fn tag_pure_noop_preserves_embed_status() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
    f.cdx().args(["tag", "1", "sci-fi"]).assert().success();
    mark_synced(&f, "1");

    f.cdx().args(["tag", "1", "sci-fi"]).assert().success();
    assert_eq!(embed_status_for(&f, "1"), "synced");
}

#[test]
fn tag_rejects_only_whitespace_args() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    f.cdx()
        .args(["tag", "1", "   ", ""])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no valid tag names"));
}

#[test]
fn tag_resolves_target_by_title() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    f.cdx()
        .args(["tag", "sample book", "history"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Tagged book 1"))
        .stdout(predicate::str::contains("+history"));
}

#[test]
fn untag_removes_present_and_reports_absent() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
    f.cdx()
        .args(["tag", "1", "sci-fi", "classic"])
        .assert()
        .success();

    let out = f
        .cdx_json()
        .args(["untag", "1", "sci-fi", "ghost"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let line = text.lines().next().expect("one line");
    let v: serde_json::Value = serde_json::from_str(line).unwrap();
    assert_eq!(v["action"], "untag");
    assert_eq!(v["removed"], serde_json::json!(["sci-fi"]));
    assert_eq!(v["not_present"], serde_json::json!(["ghost"]));
}

#[test]
fn untag_pure_noop_preserves_embed_status() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
    mark_synced(&f, "1");

    f.cdx().args(["untag", "1", "ghost"]).assert().success();
    assert_eq!(embed_status_for(&f, "1"), "synced");
}

#[test]
fn untag_all_clears_every_tag_and_marks_pending() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
    f.cdx()
        .args(["tag", "1", "sci-fi", "classic", "epic"])
        .assert()
        .success();
    mark_synced(&f, "1");

    let out = f
        .cdx_json()
        .args(["untag", "1", "--all"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let line = text.lines().next().expect("one line");
    let v: serde_json::Value = serde_json::from_str(line).unwrap();
    let removed = v["removed"].as_array().unwrap();
    assert_eq!(removed.len(), 3);
    assert_eq!(v["not_present"], serde_json::json!([]));
    assert_eq!(embed_status_for(&f, "1"), "pending");
}

#[test]
fn untag_all_on_empty_set_is_noop() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
    mark_synced(&f, "1");

    f.cdx()
        .args(["untag", "1", "--all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no tags to remove"));
    assert_eq!(embed_status_for(&f, "1"), "synced");
}

#[test]
fn untag_without_tags_or_all_errors() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    f.cdx()
        .args(["untag", "1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no valid tag names"));
}
