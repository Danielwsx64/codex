mod common;

use common::Fixture;
use predicates::prelude::*;

fn setup_with_books(f: &Fixture) {
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .arg(Fixture::fixture("sample.pdf"))
        .assert()
        .success();
    // Tag the epub to exercise the tag branch of the search.
    f.cdx()
        .args(["tag", "1", "fiction", "favorite"])
        .assert()
        .success();
}

#[test]
fn search_by_title_substring_matches_books() {
    let f = Fixture::new();
    setup_with_books(&f);

    let out = f.cdx().args(["search", "sample"]).assert().success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    // header + 2 rows.
    assert_eq!(text.lines().count(), 3);
    assert!(text.contains("Sample Book"));
    assert!(text.contains("Sample PDF Title"));
}

#[test]
fn search_by_author_substring_matches() {
    let f = Fixture::new();
    setup_with_books(&f);

    let out = f.cdx().args(["search", "jane"]).assert().success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(text.contains("Sample Book"));
    assert!(!text.contains("Sample PDF Title"));
}

#[test]
fn search_by_tag_matches() {
    let f = Fixture::new();
    setup_with_books(&f);

    let out = f.cdx().args(["search", "fiction"]).assert().success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(text.contains("Sample Book"));
    assert!(!text.contains("Sample PDF Title"));
}

#[test]
fn search_multi_token_is_and_across_fields() {
    let f = Fixture::new();
    setup_with_books(&f);

    // "sample" matches both books' titles, but only the epub also has the
    // "favorite" tag — multi-token AND filters out the PDF.
    let out = f
        .cdx()
        .args(["search", "sample favorite"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(text.contains("Sample Book"));
    assert!(!text.contains("Sample PDF Title"));
}

#[test]
fn search_no_results_human_prints_hint() {
    let f = Fixture::new();
    setup_with_books(&f);

    f.cdx()
        .args(["search", "nonexistent-token"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No books"));
}

#[test]
fn search_jsonl_emits_one_object_per_match() {
    let f = Fixture::new();
    setup_with_books(&f);

    let out = f.cdx_json().args(["search", "jane"]).assert().success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 1);
    let v: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(v["title"], "Sample Book");
    assert_eq!(v["author"], "Jane Doe");
}

#[test]
fn search_jsonl_no_results_emits_nothing() {
    let f = Fixture::new();
    setup_with_books(&f);

    let out = f
        .cdx_json()
        .args(["search", "nonexistent-token"])
        .assert()
        .success();
    assert!(
        out.get_output().stdout.is_empty(),
        "empty JSONL must emit zero bytes"
    );
}

#[test]
fn search_empty_query_errors() {
    let f = Fixture::new();
    setup_with_books(&f);

    f.cdx()
        .args(["search", "   "])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must not be empty"));
}
