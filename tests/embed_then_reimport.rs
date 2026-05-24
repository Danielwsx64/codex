mod common;

use common::Fixture;

#[test]
fn embed_then_reimport_preserves_metadata() {
    let f1 = Fixture::new();
    let lib1 = f1.init_lib();

    // 1. Import the fixture EPUB into catalog 1.
    f1.cdx()
        .arg("add")
        .arg(Fixture::fixture("sample.epub"))
        .assert()
        .success();

    // 2. Edit metadata via handle_update (mirrors what the TUI `e` modal does).
    let mut conn = codex::catalog::open_existing(&lib1).unwrap();
    let update = codex::catalog::books::BookUpdate {
        title: "Renamed Title".into(),
        author: Some("Renamed Author".into()),
        description: Some("Edited description".into()),
        series_name: Some("My Series".into()),
        series_index: Some(3.0),
        rating: Some(4),
        isbn: Some("9781234567897".into()),
        publisher: Some("My Publisher".into()),
        language: Some("pt-BR".into()),
        published_date: Some("2025-06-01".into()),
        tags: vec!["legal".into(), "contrato".into()],
    };
    let updated = codex::catalog::books::handle_update(&mut conn, &lib1, 1, update).unwrap();
    drop(conn);

    // 3. Embed to file (mirrors TUI `w` action).
    let abs = lib1.join(&updated.file_path);
    let format = codex::import::Format::parse_label(&updated.format).unwrap();
    let outcome = codex::embed::embed_into_file(&abs, format, &updated).unwrap();
    assert!(matches!(outcome, codex::embed::EmbedOutcome::Written));

    // 4. Copy the now-embedded file out and add to a *different* catalog.
    let f2 = Fixture::new();
    let lib2 = f2.init_lib();
    let copied = f2.work_dir.path().join("copied.epub");
    std::fs::copy(&abs, &copied).unwrap();

    f2.cdx().arg("add").arg(&copied).assert().success();

    // 5. Read back the metadata from catalog 2.
    let conn2 = codex::catalog::open_existing(&lib2).unwrap();
    let book2 = codex::catalog::books::handle_inspect(&conn2, "1").unwrap();

    assert_eq!(book2.title, "Renamed Title");
    assert_eq!(book2.author.as_deref(), Some("Renamed Author"));
    assert_eq!(book2.description.as_deref(), Some("Edited description"));
    assert_eq!(book2.publisher.as_deref(), Some("My Publisher"));
    assert_eq!(book2.language.as_deref(), Some("pt-BR"));
    assert_eq!(book2.published_date.as_deref(), Some("2025-06-01"));
    assert_eq!(book2.isbn.as_deref(), Some("9781234567897"));
    assert_eq!(book2.series_name.as_deref(), Some("My Series"));
    assert_eq!(book2.series_index, Some(3.0));
    let mut tags = book2.tags.clone();
    tags.sort();
    assert_eq!(tags, vec!["contrato", "legal"]);
}
