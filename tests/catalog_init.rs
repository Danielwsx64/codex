mod common;

use common::Fixture;
use predicates::str::contains;

#[test]
fn init_creates_catalog_files_and_registers() {
    let f = Fixture::new();
    let lib = f.lib_path("main");

    f.cdx()
        .arg("catalog")
        .arg("init")
        .arg("main")
        .arg(&lib)
        .assert()
        .success()
        .stdout(contains("Initialized catalog `main`"))
        .stdout(contains("(now current)"));

    assert!(lib.join("catalog.db").is_file(), "catalog.db should exist");
    assert!(lib.join("books").is_dir(), "books/ should exist");
    assert!(
        f.cfg_path().join("config.toml").is_file(),
        "config.toml should be written under --data-dir"
    );
}

#[test]
fn init_refuses_existing_catalog() {
    let f = Fixture::new();
    let lib = f.lib_path("main");
    f.cdx()
        .args(["catalog", "init", "main"])
        .arg(&lib)
        .assert()
        .success();

    f.cdx()
        .args(["catalog", "init", "other"])
        .arg(&lib)
        .assert()
        .failure()
        .stderr(contains("already initialized"));
}

#[test]
fn init_rejects_invalid_name() {
    let f = Fixture::new();
    let lib = f.lib_path("main");

    f.cdx()
        .args(["catalog", "init", "bad name"])
        .arg(&lib)
        .assert()
        .failure()
        .stderr(contains("invalid"));
}

#[test]
fn init_with_no_switch_keeps_first_as_current() {
    let f = Fixture::new();
    let first = f.lib_path("first");
    let second = f.lib_path("second");
    f.cdx()
        .args(["catalog", "init", "first"])
        .arg(&first)
        .assert()
        .success();
    f.cdx()
        .args(["catalog", "init", "second", "--no-switch"])
        .arg(&second)
        .assert()
        .success();

    let out = f.cdx().args(["catalog", "ls"]).assert().success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let first_row = stdout
        .lines()
        .find(|l| l.contains("first"))
        .expect("first row");
    let second_row = stdout
        .lines()
        .find(|l| l.contains("second"))
        .expect("second row");
    assert!(
        first_row.trim_start().starts_with('*'),
        "first should be current: {first_row}"
    );
    assert!(
        !second_row.trim_start().starts_with('*'),
        "second should not be current: {second_row}"
    );
}
