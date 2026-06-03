mod common;

use std::fs;

use common::Fixture;
use predicates::prelude::*;

#[test]
fn add_imports_epub_and_renames_with_metadata() {
    let f = Fixture::new();
    let lib = f.init_lib();

    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success()
        .stdout(predicate::str::contains("Imported"))
        .stdout(predicate::str::contains("Jane_Doe_-_Sample_Book.epub"));

    let stored = lib.join("books/1/Jane_Doe_-_Sample_Book.epub");
    assert!(
        stored.is_file(),
        "expected stored file at {}",
        stored.display()
    );
}

#[test]
fn add_imports_pdf_metadata() {
    let f = Fixture::new();
    let lib = f.init_lib();

    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.pdf"))
        .assert()
        .success();

    let stored = lib.join("books/1/PDF_Author_-_Sample_PDF_Title.pdf");
    assert!(
        stored.is_file(),
        "expected stored pdf at {}",
        stored.display()
    );
}

#[test]
fn add_rejects_unsupported_format_with_clear_message() {
    let f = Fixture::new();
    f.init_lib();
    let bad = f.work_dir.path().join("note.doc");
    fs::write(&bad, b"not a book").unwrap();

    f.cdx()
        .arg("add")
        .arg(&bad)
        .assert()
        .failure()
        .stdout(predicate::str::contains("not supported"))
        .stdout(predicate::str::contains("epub, pdf, mobi, azw3, txt, md"));
}

#[test]
fn add_supports_multiple_files_in_one_call() {
    let f = Fixture::new();
    let lib = f.init_lib();

    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .arg(Fixture::fixture("sample.pdf"))
        .assert()
        .success();

    assert!(lib.join("books/1").is_dir());
    assert!(lib.join("books/2").is_dir());
}

#[test]
fn add_partial_failure_keeps_successful_imports() {
    let f = Fixture::new();
    let lib = f.init_lib();
    let bad = f.work_dir.path().join("oops.doc");
    fs::write(&bad, b"x").unwrap();

    let assert = f
        .cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .arg(&bad)
        .assert();
    // At least one import succeeded → command exits 0 overall, but the
    // failure line is still on stdout.
    assert
        .success()
        .stdout(predicate::str::contains("Imported"))
        .stdout(predicate::str::contains("not supported"));
    assert!(lib.join("books/1").is_dir());
}

#[test]
fn add_jsonl_emits_one_object_per_file() {
    let f = Fixture::new();
    f.init_lib();

    let out = f
        .cdx_json()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .arg(Fixture::fixture("sample.pdf"))
        .assert()
        .success();

    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(lines.len(), 2);
    for line in &lines {
        let v: serde_json::Value = serde_json::from_str(line).expect("valid JSON line");
        assert_eq!(v["status"], "imported");
        assert!(v["id"].as_i64().is_some());
        assert!(v["stored_path"].as_str().is_some());
    }
}
