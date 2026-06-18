mod common;

use codex::device::DISABLE_SCAN_ENV;
use common::Fixture;
use predicates::prelude::*;

// The host USB scan is disabled (`DISABLE_SCAN_ENV`) so the suite stays hermetic
// without a real Kindle. With no device detected, `cdx device clean` must fail
// with the shared selection error rather than guessing. The deletion itself
// (file + sync-state row, catalog left untouched) is covered by unit tests in
// `src/device/clean.rs`, its rendering in `src/catalog/render.rs`, and the
// multi-select picker in `src/tui/pick.rs` — an integration test can't fabricate
// a detected device to exercise the apply path.

#[test]
fn clean_without_a_connected_device_errors() {
    let f = Fixture::new();
    f.init_lib();

    f.cdx()
        .env(DISABLE_SCAN_ENV, "1")
        .args(["device", "clean", "--all", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no device connected"));
}

#[test]
fn clean_for_unknown_device_reports_no_match() {
    let f = Fixture::new();
    f.init_lib();

    f.cdx()
        .env(DISABLE_SCAN_ENV, "1")
        .args(["device", "clean", "--device", "ghost", "--all", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no device matches"));
}

#[test]
fn clean_for_known_but_disconnected_device_reports_not_connected() {
    let f = Fixture::new();
    let lib = f.init_lib();
    let conn = codex::catalog::open_existing(&lib).unwrap();
    codex::catalog::devices::record_seen(&conn, "SERIAL_A").unwrap();
    codex::catalog::devices::set_alias(&conn, "SERIAL_A", "paperwhite").unwrap();

    f.cdx()
        .env(DISABLE_SCAN_ENV, "1")
        .args(["device", "clean", "--device", "paperwhite", "--all"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not currently connected"));
}
