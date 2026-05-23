mod common;

use common::Fixture;
use predicates::prelude::*;

fn setup_with_two_books(f: &Fixture) {
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .arg(Fixture::fixture("sample.pdf"))
        .assert()
        .success();
}

#[test]
fn inspect_by_id() {
    let f = Fixture::new();
    setup_with_two_books(&f);

    f.cdx()
        .arg("inspect")
        .arg("1")
        .assert()
        .success()
        .stdout(predicate::str::contains("Sample Book"))
        .stdout(predicate::str::contains("Jane Doe"));
}

#[test]
fn inspect_by_exact_title_case_insensitive() {
    let f = Fixture::new();
    setup_with_two_books(&f);

    f.cdx()
        .arg("inspect")
        .arg("sample book")
        .assert()
        .success()
        .stdout(predicate::str::contains("Sample Book"));
}

#[test]
fn inspect_unknown_id_errors() {
    let f = Fixture::new();
    setup_with_two_books(&f);

    f.cdx()
        .arg("inspect")
        .arg("9999")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no book matches"));
}

#[test]
fn inspect_unknown_title_errors() {
    let f = Fixture::new();
    setup_with_two_books(&f);

    f.cdx()
        .arg("inspect")
        .arg("Ghost Book")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no book matches"));
}

#[test]
fn inspect_ambiguous_title_lists_ids() {
    let f = Fixture::new();
    f.init_lib();
    // Import the same epub twice to get two books with identical title.
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    f.cdx()
        .arg("inspect")
        .arg("Sample Book")
        .assert()
        .failure()
        .stderr(predicate::str::contains("multiple books"))
        .stderr(predicate::str::contains("1"))
        .stderr(predicate::str::contains("2"));
}

#[test]
fn inspect_jsonl_includes_absolute_path() {
    let f = Fixture::new();
    setup_with_two_books(&f);

    let out = f.cdx_json().arg("inspect").arg("1").assert().success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let line = text.lines().next().expect("one line");
    let v: serde_json::Value = serde_json::from_str(line).unwrap();
    let path = v["file_path"].as_str().unwrap();
    assert!(
        std::path::Path::new(path).is_absolute(),
        "file_path should be absolute: {path}"
    );
    assert!(path.ends_with("Jane_Doe_-_Sample_Book.epub"));
}
