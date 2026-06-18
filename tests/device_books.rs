mod common;

use codex::device::DISABLE_SCAN_ENV;
use common::Fixture;
use predicates::prelude::*;

// The host USB scan is disabled (`DISABLE_SCAN_ENV`) so the suite stays hermetic
// without a real Kindle. With no device detected, `device books` must fail with
// a clear selection error rather than guessing — the presence/matching logic
// itself is covered by unit tests in `src/device/books.rs`.

#[test]
fn books_without_a_connected_device_errors() {
    let f = Fixture::new();
    f.init_lib();

    f.cdx()
        .env(DISABLE_SCAN_ENV, "1")
        .args(["device", "books"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no device connected"));
}

#[test]
fn books_for_known_but_disconnected_device_reports_not_connected() {
    let f = Fixture::new();
    let lib = f.init_lib();
    let conn = codex::catalog::open_existing(&lib).unwrap();
    codex::catalog::devices::record_seen(&conn, "SERIAL_A").unwrap();
    codex::catalog::devices::set_alias(&conn, "SERIAL_A", "paperwhite").unwrap();

    f.cdx()
        .env(DISABLE_SCAN_ENV, "1")
        .args(["device", "books", "--device", "paperwhite"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not currently connected"));
}

#[test]
fn books_for_unknown_device_reports_no_match() {
    let f = Fixture::new();
    f.init_lib();

    f.cdx()
        .env(DISABLE_SCAN_ENV, "1")
        .args(["device", "books", "--device", "ghost"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no device matches"));
}
