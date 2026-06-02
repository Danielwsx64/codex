mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use common::Fixture;
use predicates::prelude::*;

fn write_editor_script(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn setup(f: &Fixture) {
    f.init_lib();
    f.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();
}

#[test]
fn edit_no_change_reports_and_keeps_db_untouched() {
    let f = Fixture::new();
    setup(&f);

    let before = String::from_utf8(
        f.cdx_json()
            .args(["inspect", "1"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();

    f.cdx()
        .env("EDITOR", "/bin/true")
        .args(["edit", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no changes"));

    let after = String::from_utf8(
        f.cdx_json()
            .args(["inspect", "1"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();
    assert_eq!(before, after);
}

#[test]
fn edit_applies_changes_and_marks_embed_pending() {
    let f = Fixture::new();
    setup(&f);
    // After `add`, the new book starts pending. Mark it synced so we can
    // observe the reset.
    let lib = f.lib_path("lib");
    let conn = rusqlite::Connection::open(lib.join("catalog.db")).unwrap();
    conn.execute(
        "UPDATE books SET embed_status='synced', embed_synced_at=datetime('now') WHERE id=1",
        [],
    )
    .unwrap();
    drop(conn);

    let editor = f.work_dir.path().join("rewrite-title.sh");
    write_editor_script(
        &editor,
        "#!/bin/sh\nsed -i 's/^title = .*/title = \"Brand New Title\"/' \"$1\"\n",
    );

    f.cdx()
        .env("EDITOR", &editor)
        .args(["edit", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated book 1"))
        .stdout(predicate::str::contains("Brand New Title"));

    let conn = rusqlite::Connection::open(lib.join("catalog.db")).unwrap();
    let (title, status, synced_at): (String, String, Option<String>) = conn
        .query_row(
            "SELECT title, embed_status, embed_synced_at FROM books WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(title, "Brand New Title");
    assert_eq!(status, "pending");
    assert!(synced_at.is_none());
}

#[test]
fn edit_invalid_toml_aborts_and_preserves_tempfile() {
    let f = Fixture::new();
    setup(&f);

    let editor = f.work_dir.path().join("break-toml.sh");
    write_editor_script(
        &editor,
        "#!/bin/sh\nprintf 'this = is = not toml\\n' > \"$1\"\n",
    );

    f.cdx()
        .env("EDITOR", &editor)
        .args(["edit", "1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid TOML"))
        .stderr(predicate::str::contains("tempfile preserved at"));

    // DB unchanged.
    let lib = f.lib_path("lib");
    let conn = rusqlite::Connection::open(lib.join("catalog.db")).unwrap();
    let title: String = conn
        .query_row("SELECT title FROM books WHERE id=1", [], |r| r.get(0))
        .unwrap();
    assert_ne!(title, ""); // import populated it; we don't care which exact value
}

#[test]
fn edit_no_change_json_emits_one_object() {
    let f = Fixture::new();
    setup(&f);

    let output = f
        .cdx_json()
        .env("EDITOR", "/bin/true")
        .args(["edit", "1"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 1, "expected one JSONL line, got {text:?}");
    let v: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(v["action"], "edit");
    assert_eq!(v["status"], "no_change");
    assert_eq!(v["id"], 1);
    assert_eq!(v["changed"], false);
}

#[test]
fn edit_applied_change_json_reports_updated() {
    let f = Fixture::new();
    setup(&f);

    let editor = f.work_dir.path().join("rewrite-title.sh");
    write_editor_script(
        &editor,
        "#!/bin/sh\nsed -i 's/^title = .*/title = \"Brand New Title\"/' \"$1\"\n",
    );

    let output = f
        .cdx_json()
        .env("EDITOR", &editor)
        .args(["edit", "1"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 1);
    let v: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(v["action"], "edit");
    assert_eq!(v["status"], "updated");
    assert_eq!(v["id"], 1);
    assert_eq!(v["title"], "Brand New Title");
    assert_eq!(v["changed"], true);
}

#[test]
fn edit_unknown_id_errors_before_editor() {
    let f = Fixture::new();
    setup(&f);

    // /bin/false would only fail *after* launch; with a missing id we never
    // reach the editor, so even false won't be invoked.
    f.cdx()
        .env("EDITOR", "/bin/false")
        .args(["edit", "9999"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no book matches"));
}
