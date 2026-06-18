mod common;

use codex::device::DISABLE_SCAN_ENV;
use common::Fixture;
use predicates::prelude::*;

// The host USB scan is disabled (`DISABLE_SCAN_ENV`) so the suite stays hermetic
// without a real Kindle. With no device detected, `cdx sync` must fail with the
// shared selection error rather than guessing. The plan computation is covered by
// unit tests in `src/device/sync.rs`, its rendering in `src/catalog/render.rs`, and
// the `y/n/a/q` parser in `src/cli/device.rs` — an integration test can't fabricate
// a detected device to exercise the apply path.

#[test]
fn sync_without_a_connected_device_errors() {
    let f = Fixture::new();
    f.init_lib();

    f.cdx()
        .env(DISABLE_SCAN_ENV, "1")
        .arg("sync")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no device connected"));
}

#[test]
fn sync_dry_run_without_a_connected_device_errors() {
    let f = Fixture::new();
    f.init_lib();

    // --dry-run still needs a device to diff against.
    f.cdx()
        .env(DISABLE_SCAN_ENV, "1")
        .args(["sync", "--dry-run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no device connected"));
}

#[test]
fn sync_for_unknown_device_reports_no_match() {
    let f = Fixture::new();
    f.init_lib();

    f.cdx()
        .env(DISABLE_SCAN_ENV, "1")
        .args(["sync", "--device", "ghost"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no device matches"));
}

#[test]
fn sync_for_known_but_disconnected_device_reports_not_connected() {
    let f = Fixture::new();
    let lib = f.init_lib();
    let conn = codex::catalog::open_existing(&lib).unwrap();
    codex::catalog::devices::record_seen(&conn, "SERIAL_A").unwrap();
    codex::catalog::devices::set_alias(&conn, "SERIAL_A", "paperwhite").unwrap();

    f.cdx()
        .env(DISABLE_SCAN_ENV, "1")
        .args(["sync", "--device", "paperwhite"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not currently connected"));
}
