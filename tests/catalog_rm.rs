mod common;

use common::Fixture;
use predicates::str::contains;

#[test]
fn rm_removes_entry_keeps_files() {
    let f = Fixture::new();
    let lib = f.lib_path("main");
    f.cdx()
        .args(["catalog", "init", "main"])
        .arg(&lib)
        .assert()
        .success();

    f.cdx()
        .args(["catalog", "rm", "main"])
        .assert()
        .success()
        .stdout(contains("Removed catalog `main`"));

    assert!(
        lib.join("catalog.db").is_file(),
        "files should remain without --purge"
    );
}

#[test]
fn rm_purge_deletes_files() {
    let f = Fixture::new();
    let lib = f.lib_path("main");
    f.cdx()
        .args(["catalog", "init", "main"])
        .arg(&lib)
        .assert()
        .success();

    f.cdx()
        .args(["catalog", "rm", "main", "--purge"])
        .assert()
        .success()
        .stdout(contains("purged from disk"));

    assert!(
        !lib.exists(),
        "catalog directory should be gone after --purge"
    );
}

#[test]
fn rm_unknown_name_errors() {
    let f = Fixture::new();
    f.cdx()
        .args(["catalog", "rm", "ghost"])
        .assert()
        .failure()
        .stderr(contains("no catalog named `ghost`"));
}
