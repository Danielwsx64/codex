mod common;

use std::fs;

use common::Fixture;
use predicates::str::contains;

#[test]
fn add_registers_existing_catalog() {
    let f = Fixture::new();
    let lib = f.lib_path("main");
    // First init under a different config dir to get a real catalog on disk.
    let other_cfg = tempfile::tempdir().unwrap();
    assert_cmd::Command::cargo_bin("cdx")
        .unwrap()
        .args(["--data-dir"])
        .arg(other_cfg.path())
        .args(["catalog", "init", "main"])
        .arg(&lib)
        .assert()
        .success();

    f.cdx()
        .args(["catalog", "add", "shared"])
        .arg(&lib)
        .assert()
        .success()
        .stdout(contains("Registered catalog `shared`"));
}

#[test]
fn add_rejects_dir_without_catalog_db() {
    let f = Fixture::new();
    let empty = f.lib_path("empty");
    fs::create_dir_all(&empty).unwrap();

    f.cdx()
        .args(["catalog", "add", "name"])
        .arg(&empty)
        .assert()
        .failure()
        .stderr(contains("missing its database"));
}

#[test]
fn add_no_switch_preserves_current() {
    let f = Fixture::new();
    let first = f.lib_path("first");
    let second_lib = f.lib_path("second");

    f.cdx()
        .args(["catalog", "init", "first"])
        .arg(&first)
        .assert()
        .success();

    // Init second via a separate cfg, then add into this cfg with --no-switch
    let other_cfg = tempfile::tempdir().unwrap();
    assert_cmd::Command::cargo_bin("cdx")
        .unwrap()
        .args(["--data-dir"])
        .arg(other_cfg.path())
        .args(["catalog", "init", "second"])
        .arg(&second_lib)
        .assert()
        .success();

    f.cdx()
        .args(["catalog", "add", "second", "--no-switch"])
        .arg(&second_lib)
        .assert()
        .success();

    let out = f.cdx().args(["catalog", "ls"]).assert().success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let first_row = stdout
        .lines()
        .find(|l| l.contains("first"))
        .expect("first row");
    assert!(
        first_row.trim_start().starts_with('*'),
        "first should still be current: {first_row}"
    );
}
