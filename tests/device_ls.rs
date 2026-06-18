mod common;

use codex::device::DISABLE_SCAN_ENV;
use common::Fixture;
use predicates::prelude::*;

// These drive the *known* (DB) path: seed the devices table directly through
// the library, then assert the CLI lists the rows as disconnected. The host
// USB scan is disabled via `DISABLE_SCAN_ENV` so the suite stays hermetic even
// when a real Kindle is plugged into the machine running the tests.

#[test]
fn ls_empty_prints_hint() {
    let f = Fixture::new();
    f.init_lib();

    f.cdx()
        .env(DISABLE_SCAN_ENV, "1")
        .args(["device", "ls"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No devices"));
}

#[test]
fn ls_empty_jsonl_emits_nothing() {
    let f = Fixture::new();
    f.init_lib();

    let out = f
        .cdx_json()
        .env(DISABLE_SCAN_ENV, "1")
        .args(["device", "ls"])
        .assert()
        .success();
    assert!(
        out.get_output().stdout.is_empty(),
        "empty JSONL must emit zero bytes"
    );
}

#[test]
fn ls_known_device_human_shows_disconnected() {
    let f = Fixture::new();
    let lib = f.init_lib();
    let conn = codex::catalog::open_existing(&lib).unwrap();
    codex::catalog::devices::record_seen(&conn, "DEVICE_123").unwrap();

    f.cdx()
        .env(DISABLE_SCAN_ENV, "1")
        .args(["device", "ls"])
        .assert()
        .success()
        .stdout(predicate::str::contains("DEVICE_123"))
        .stdout(predicate::str::contains("no"));
}

#[test]
fn ls_known_device_jsonl_one_object_per_line() {
    let f = Fixture::new();
    let lib = f.init_lib();
    let conn = codex::catalog::open_existing(&lib).unwrap();
    codex::catalog::devices::record_seen(&conn, "SERIAL_A").unwrap();

    let out = f
        .cdx_json()
        .env(DISABLE_SCAN_ENV, "1")
        .args(["device", "ls"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 1);
    let v: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(v["serial"], "SERIAL_A");
    assert_eq!(v["connected"], false);
    assert_eq!(v["mount_path"], serde_json::Value::Null);
    assert_eq!(v["book_count"], serde_json::Value::Null);
}

#[test]
fn ls_alias_fallback_then_alias() {
    let f = Fixture::new();
    let lib = f.init_lib();
    let conn = codex::catalog::open_existing(&lib).unwrap();
    codex::catalog::devices::record_seen(&conn, "SERIAL_B").unwrap();

    // Without an alias, the serial fills the ALIAS column.
    let out = f
        .cdx_json()
        .env(DISABLE_SCAN_ENV, "1")
        .args(["device", "ls"])
        .assert()
        .success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    assert_eq!(v["alias"], serde_json::Value::Null);

    // After setting an alias, it shows up in the human listing.
    codex::catalog::devices::set_alias(&conn, "SERIAL_B", "basement").unwrap();
    f.cdx()
        .env(DISABLE_SCAN_ENV, "1")
        .args(["device", "ls"])
        .assert()
        .success()
        .stdout(predicate::str::contains("basement"));
}
