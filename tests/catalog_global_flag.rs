mod common;

use common::Fixture;

#[test]
fn catalog_flag_does_not_change_current() {
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

    // `--catalog b catalog ls` should succeed without flipping current.
    f.cdx()
        .args(["--catalog", "b", "catalog", "ls"])
        .assert()
        .success();

    let out = f.cdx().args(["catalog", "ls"]).assert().success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let a_row = stdout
        .lines()
        .find(|l| {
            let t = l.trim_start();
            t.starts_with('*') && t.contains(" a ")
        })
        .or_else(|| {
            stdout
                .lines()
                .find(|l| l.contains(" a ") || l.contains("\ta\t"))
        })
        .unwrap_or_else(|| panic!("a row not found in: {stdout}"));
    assert!(
        a_row.trim_start().starts_with('*'),
        "a should still be current: {a_row}"
    );
}
