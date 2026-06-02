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
        .stderr(predicate::str::contains("no search criteria"));
}

#[test]
fn search_no_args_errors() {
    let f = Fixture::new();
    setup_with_books(&f);

    f.cdx()
        .arg("search")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn search_by_author_flag_filters() {
    let f = Fixture::new();
    setup_with_books(&f);

    let out = f
        .cdx()
        .args(["search", "--author", "jane"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(text.contains("Sample Book"));
    assert!(!text.contains("Sample PDF Title"));
}

#[test]
fn search_by_tag_flag_filters() {
    let f = Fixture::new();
    setup_with_books(&f);

    let out = f
        .cdx()
        .args(["search", "--tag", "fiction"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(text.contains("Sample Book"));
    assert!(!text.contains("Sample PDF Title"));
}

#[test]
fn search_multi_tag_is_and() {
    let f = Fixture::new();
    setup_with_books(&f);

    // The epub has both `fiction` and `favorite` — both filters keep it.
    let out = f
        .cdx()
        .args(["search", "--tag", "fiction", "--tag", "favorite"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(text.contains("Sample Book"));

    // Adding a tag the epub doesn't have drops it out.
    f.cdx()
        .args(["search", "--tag", "fiction", "--tag", "ghost"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No books"));
}

#[test]
fn search_by_series_flag_filters() {
    let f = Fixture::new();
    setup_with_books(&f);
    f.cdx()
        .args(["series", "1", "Foundation Saga"])
        .assert()
        .success();

    let out = f
        .cdx()
        .args(["search", "--series", "saga"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(text.contains("Sample Book"));
    assert!(!text.contains("Sample PDF Title"));
}

#[test]
fn search_by_rating_exact_and_range() {
    let f = Fixture::new();
    setup_with_books(&f);
    f.cdx().args(["rate", "1", "5"]).assert().success();
    f.cdx().args(["rate", "2", "3"]).assert().success();

    // Exact match.
    let out = f.cdx().args(["search", "--rating", "5"]).assert().success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(text.contains("Sample Book"));
    assert!(!text.contains("Sample PDF Title"));

    // Range includes both.
    let out = f
        .cdx()
        .args(["search", "--rating", "3..5"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(text.contains("Sample Book"));
    assert!(text.contains("Sample PDF Title"));
}

#[test]
fn search_combines_query_and_filter() {
    let f = Fixture::new();
    setup_with_books(&f);

    // Positional matches both books, --author narrows to the epub.
    let out = f
        .cdx()
        .args(["search", "sample", "--author", "jane"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(text.contains("Sample Book"));
    assert!(!text.contains("Sample PDF Title"));
}

#[test]
fn search_jsonl_with_filter() {
    let f = Fixture::new();
    setup_with_books(&f);

    let out = f
        .cdx_json()
        .args(["search", "--tag", "fiction"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 1);
    let v: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(v["title"], "Sample Book");
}

#[test]
fn search_respects_columns_flag() {
    let f = Fixture::new();
    setup_with_books(&f);

    // Custom columns only — header reflects the selection, other slugs absent.
    let out = f
        .cdx()
        .args(["search", "sample", "--columns", "id,title"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(text.contains("ID"));
    assert!(text.contains("TITLE"));
    assert!(!text.contains("AUTHOR"));
    assert!(!text.contains("FORMAT"));
}

#[test]
fn search_respects_all_columns_flag() {
    let f = Fixture::new();
    setup_with_books(&f);

    let out = f
        .cdx()
        .args(["search", "sample", "--all-columns"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    // --all-columns surfaces fields outside the default set (publisher, isbn, ...).
    assert!(text.contains("PUBLISHER"));
    assert!(text.contains("LANGUAGE"));
    assert!(text.contains("EMBED"));
}

#[test]
fn search_invalid_rating_value_errors() {
    let f = Fixture::new();
    setup_with_books(&f);

    f.cdx()
        .args(["search", "--rating", "6"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("rating must be"));

    f.cdx()
        .args(["search", "--rating", "5..3"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("min must be <= max"));

    f.cdx()
        .args(["search", "--rating", "abc"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("rating must be"));
}
