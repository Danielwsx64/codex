mod common;

use common::Fixture;
use predicates::str::contains;

#[test]
fn use_switches_current() {
    let f = Fixture::new();
    let a = f.lib_path("a");
    let b = f.lib_path("b");
    f.cdx()
        .args(["catalog", "init", "a"])
        .arg(&a)
        .assert()
        .success();
    f.cdx()
        .args(["catalog", "init", "b", "--no-switch"])
        .arg(&b)
        .assert()
        .success();

    f.cdx()
        .args(["catalog", "use", "b"])
        .assert()
        .success()
        .stdout(contains("Switched to catalog `b`"));

    let out = f.cdx().args(["catalog", "ls"]).assert().success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let b_row = stdout
        .lines()
        .find(|l| l.contains("b "))
        .or_else(|| {
            stdout
                .lines()
                .find(|l| l.contains(" b\t") || l.ends_with(" b"))
        })
        .unwrap_or_else(|| panic!("b row not found in: {stdout}"));
    assert!(
        b_row.trim_start().starts_with('*'),
        "b should be current: {b_row}"
    );
}

#[test]
fn use_unknown_name_errors() {
    let f = Fixture::new();
    f.cdx()
        .args(["catalog", "use", "ghost"])
        .assert()
        .failure()
        .stderr(contains("no catalog named `ghost`"));
}
