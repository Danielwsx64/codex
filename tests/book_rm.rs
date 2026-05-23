mod common;

use std::fs;

use common::Fixture;
use predicates::prelude::*;

#[test]
fn rm_default_deletes_record_and_file() {
    let f = Fixture::new();
    let lib = f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
    let book_dir = lib.join("books/1");
    assert!(book_dir.is_dir());

    f.cdx()
        .arg("rm")
        .arg("1")
        .assert()
        .success()
        .stdout(predicate::str::contains("deleted its file"));

    assert!(!book_dir.exists(), "books/1 should be gone");

    f.cdx()
        .arg("ls")
        .assert()
        .success()
        .stdout(predicate::str::contains("No books"));
}

#[test]
fn rm_keep_moves_file_to_cwd() {
    let f = Fixture::new();
    let lib = f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    // Run with a controlled cwd so we can find the kept file deterministically.
    let cwd = f.work_dir.path().join("keep_dest");
    fs::create_dir_all(&cwd).unwrap();
    f.cdx()
        .current_dir(&cwd)
        .arg("rm")
        .arg("1")
        .arg("--keep")
        .assert()
        .success()
        .stdout(predicate::str::contains("file kept at"));

    let kept = cwd.join("Jane_Doe_-_Sample_Book.epub");
    assert!(kept.is_file(), "kept file missing at {}", kept.display());
    assert!(
        !lib.join("books/1").exists(),
        "books/1 dir should be removed"
    );
}

#[test]
fn rm_keep_collision_uses_suffix() {
    let f = Fixture::new();
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    let cwd = f.work_dir.path().join("keep_dest");
    fs::create_dir_all(&cwd).unwrap();
    // Pre-create a file with the same name to force a collision.
    fs::write(cwd.join("Jane_Doe_-_Sample_Book.epub"), b"existing").unwrap();

    f.cdx()
        .current_dir(&cwd)
        .arg("rm")
        .arg("1")
        .arg("--keep")
        .assert()
        .success();

    // Original untouched.
    let original = fs::read(cwd.join("Jane_Doe_-_Sample_Book.epub")).unwrap();
    assert_eq!(original, b"existing");
    // Suffix variant is the actual kept book.
    let suffix = cwd.join("Jane_Doe_-_Sample_Book.1.epub");
    assert!(
        suffix.is_file(),
        "expected suffix file at {}",
        suffix.display()
    );
}

#[test]
fn rm_unknown_target_errors_without_touching_catalog() {
    let f = Fixture::new();
    let lib = f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    f.cdx()
        .arg("rm")
        .arg("ghost")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no book matches"));

    // Existing book still present.
    assert!(lib.join("books/1").is_dir());
}

#[test]
fn rm_ambiguous_target_errors_without_deleting() {
    let f = Fixture::new();
    let lib = f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    f.cdx()
        .arg("rm")
        .arg("Sample Book")
        .assert()
        .failure()
        .stderr(predicate::str::contains("multiple books"));

    // Both still on disk.
    assert!(lib.join("books/1").is_dir());
    assert!(lib.join("books/2").is_dir());
}
