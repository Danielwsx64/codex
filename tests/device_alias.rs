mod common;

use codex::device::DISABLE_SCAN_ENV;
use common::Fixture;
use predicates::prelude::*;

// Aliasing operates purely on the DB, so the USB scan is disabled via
// `DISABLE_SCAN_ENV` and devices are seeded in-process with `record_seen`.

#[test]
fn alias_by_serial_then_ls_shows_alias() {
    let f = Fixture::new();
    let lib = f.init_lib();
    let conn = codex::catalog::open_existing(&lib).unwrap();
    codex::catalog::devices::record_seen(&conn, "SERIAL_A").unwrap();

    f.cdx()
        .env(DISABLE_SCAN_ENV, "1")
        .args(["device", "alias", "SERIAL_A", "paperwhite"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Aliased device SERIAL_A"));

    f.cdx()
        .env(DISABLE_SCAN_ENV, "1")
        .args(["device", "ls"])
        .assert()
        .success()
        .stdout(predicate::str::contains("paperwhite"));
}

#[test]
fn alias_by_existing_alias_renames() {
    let f = Fixture::new();
    let lib = f.init_lib();
    let conn = codex::catalog::open_existing(&lib).unwrap();
    codex::catalog::devices::record_seen(&conn, "SERIAL_A").unwrap();
    codex::catalog::devices::set_alias(&conn, "SERIAL_A", "paperwhite").unwrap();

    f.cdx()
        .env(DISABLE_SCAN_ENV, "1")
        .args(["device", "alias", "paperwhite", "study"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Renamed device SERIAL_A"));
}

#[test]
fn alias_unknown_target_fails() {
    let f = Fixture::new();
    f.init_lib();

    f.cdx()
        .env(DISABLE_SCAN_ENV, "1")
        .args(["device", "alias", "nope", "x"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no device matches"));
}

#[test]
fn alias_duplicate_on_other_device_fails() {
    let f = Fixture::new();
    let lib = f.init_lib();
    let conn = codex::catalog::open_existing(&lib).unwrap();
    codex::catalog::devices::record_seen(&conn, "SERIAL_A").unwrap();
    codex::catalog::devices::record_seen(&conn, "SERIAL_B").unwrap();
    codex::catalog::devices::set_alias(&conn, "SERIAL_A", "paperwhite").unwrap();

    f.cdx()
        .env(DISABLE_SCAN_ENV, "1")
        .args(["device", "alias", "SERIAL_B", "paperwhite"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already used"));
}

#[test]
fn alias_json_emits_action_object() {
    let f = Fixture::new();
    let lib = f.init_lib();
    let conn = codex::catalog::open_existing(&lib).unwrap();
    codex::catalog::devices::record_seen(&conn, "SERIAL_A").unwrap();

    let out = f
        .cdx_json()
        .env(DISABLE_SCAN_ENV, "1")
        .args(["device", "alias", "SERIAL_A", "paperwhite"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 1);
    let v: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(v["action"], "alias");
    assert_eq!(v["serial"], "SERIAL_A");
    assert_eq!(v["alias"], "paperwhite");
}
