mod common;

use common::Fixture;
use predicates::prelude::*;

#[test]
fn add_same_epub_twice_is_duplicate() {
    let f = Fixture::new();
    let lib = f.init_lib();

    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success()
        .stdout(predicate::str::contains("Imported"));

    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success()
        .stdout(predicate::str::contains("duplicate of book #1"));

    assert!(lib.join("books/1").is_dir());
    assert!(
        !lib.join("books/2").exists(),
        "duplicate must not create a second book dir"
    );
}

#[test]
fn add_force_reimports_duplicate() {
    let f = Fixture::new();
    let lib = f.init_lib();

    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    f.cdx()
        .args(["add", "--force"])
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success()
        .stdout(predicate::str::contains("Imported"));

    assert!(lib.join("books/2").is_dir(), "--force should import a copy");
}

#[test]
fn add_duplicate_jsonl_reports_existing_id() {
    let f = Fixture::new();
    f.init_lib();

    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    let out = f
        .cdx_json()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let line = stdout.lines().next().expect("one json line");
    let v: serde_json::Value = serde_json::from_str(line).expect("valid JSON line");
    assert_eq!(v["status"], "duplicate");
    assert_eq!(v["existing_id"], 1);
    assert!(v["id"].is_null(), "no new id for a skipped duplicate");
}

#[test]
fn reimport_original_epub_after_embed_is_duplicate_via_content_hash() {
    let f = Fixture::new();
    let lib = f.init_lib();

    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    // Embed rewrites the stored copy's OPF (and thus its whole-file hash).
    f.cdx().args(["embed", "sync"]).assert().success();

    // The original file is byte-for-byte unchanged; its content hash still
    // matches the stored book, so this is detected as a duplicate.
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success()
        .stdout(predicate::str::contains("duplicate of book #1"));

    assert!(!lib.join("books/2").exists());
}

#[test]
fn reimport_embedded_pdf_copy_is_duplicate_via_full_hash() {
    let f = Fixture::new();
    let lib = f.init_lib();

    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.pdf"))
        .assert()
        .success();

    f.cdx().args(["embed", "sync"]).assert().success();

    // PDF has no stable content hash, but the post-embed whole-file hash was
    // added to the book's fingerprint list, so re-adding the embedded copy is
    // recognized as a duplicate.
    let stored = lib.join("books/1/PDF_Author_-_Sample_PDF_Title.pdf");
    let copy = f.work_dir.path().join("again.pdf");
    std::fs::copy(&stored, &copy).unwrap();

    f.cdx()
        .arg("add")
        .arg(&copy)
        .assert()
        .success()
        .stdout(predicate::str::contains("duplicate of book #1"));

    assert!(!lib.join("books/2").exists());
}
