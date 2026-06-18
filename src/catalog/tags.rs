use rusqlite::{params, Transaction};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TagDelta {
    pub changed: Vec<String>,
    pub unchanged: Vec<String>,
}

// Tag-list separators. Different sources delimit differently — Calibre and many
// EPUB/MOBI exports use `;`, others use `,` — so both are accepted everywhere a
// tag list is parsed (file metadata, the `cdx tag` CLI, the TUI edit field).
const SEPARATORS: &[char] = &[',', ';'];

pub fn normalize(input: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    push_split(input, &mut out);
    out
}

pub fn normalize_many<I, S>(values: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out: Vec<String> = Vec::new();
    for v in values {
        // Each incoming value may itself be a delimited list (e.g. a single
        // `<dc:subject>` carrying `a; b; c`), so split it too.
        push_split(v.as_ref(), &mut out);
    }
    out
}

// Split on any separator, trim, drop empties, and dedup case-insensitively
// against what is already collected (preserving first-seen casing/order).
fn push_split(input: &str, out: &mut Vec<String>) {
    for piece in input.split(SEPARATORS) {
        let trimmed = piece.trim();
        if trimmed.is_empty() {
            continue;
        }
        if out
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(trimmed))
        {
            continue;
        }
        out.push(trimmed.to_string());
    }
}

pub fn sync(tx: &Transaction<'_>, book_id: i64, names: &[String]) -> rusqlite::Result<()> {
    tx.execute("DELETE FROM book_tags WHERE book_id = ?1", params![book_id])?;
    if names.is_empty() {
        return Ok(());
    }
    {
        let mut upsert = tx.prepare_cached("INSERT OR IGNORE INTO tags (name) VALUES (?1)")?;
        let mut select = tx.prepare_cached("SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE")?;
        let mut link =
            tx.prepare_cached("INSERT OR IGNORE INTO book_tags (book_id, tag_id) VALUES (?1, ?2)")?;
        for name in names {
            upsert.execute(params![name])?;
            let tag_id: i64 = select.query_row(params![name], |r| r.get(0))?;
            link.execute(params![book_id, tag_id])?;
        }
    }
    Ok(())
}

pub fn add_for_book(
    tx: &Transaction<'_>,
    book_id: i64,
    names: &[String],
) -> rusqlite::Result<TagDelta> {
    let mut delta = TagDelta::default();
    if names.is_empty() {
        return Ok(delta);
    }
    let mut upsert = tx.prepare_cached("INSERT OR IGNORE INTO tags (name) VALUES (?1)")?;
    let mut select_id = tx.prepare_cached("SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE")?;
    let mut link =
        tx.prepare_cached("INSERT OR IGNORE INTO book_tags (book_id, tag_id) VALUES (?1, ?2)")?;
    for name in names {
        upsert.execute(params![name])?;
        let tag_id: i64 = select_id.query_row(params![name], |r| r.get(0))?;
        let inserted = link.execute(params![book_id, tag_id])?;
        if inserted > 0 {
            delta.changed.push(name.clone());
        } else {
            delta.unchanged.push(name.clone());
        }
    }
    Ok(delta)
}

pub fn remove_for_book(
    tx: &Transaction<'_>,
    book_id: i64,
    names: &[String],
) -> rusqlite::Result<TagDelta> {
    let mut delta = TagDelta::default();
    if names.is_empty() {
        return Ok(delta);
    }
    let mut delete = tx.prepare_cached(
        "DELETE FROM book_tags
         WHERE book_id = ?1
           AND tag_id IN (SELECT id FROM tags WHERE name = ?2 COLLATE NOCASE)",
    )?;
    for name in names {
        let removed = delete.execute(params![book_id, name])?;
        if removed > 0 {
            delta.changed.push(name.clone());
        } else {
            delta.unchanged.push(name.clone());
        }
    }
    Ok(delta)
}

pub fn clear_for_book(tx: &Transaction<'_>, book_id: i64) -> rusqlite::Result<Vec<String>> {
    let removed: Vec<String> = {
        let mut stmt = tx.prepare_cached(
            "SELECT t.name
             FROM book_tags bt
             JOIN tags t ON t.id = bt.tag_id
             WHERE bt.book_id = ?1
             ORDER BY LOWER(t.name)",
        )?;
        let rows = stmt.query_map(params![book_id], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    tx.execute("DELETE FROM book_tags WHERE book_id = ?1", params![book_id])?;
    Ok(removed)
}

pub fn fetch_for_book(conn: &rusqlite::Connection, book_id: i64) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare_cached(
        "SELECT t.name
         FROM book_tags bt
         JOIN tags t ON t.id = bt.tag_id
         WHERE bt.book_id = ?1
         ORDER BY LOWER(t.name)",
    )?;
    let names: rusqlite::Result<Vec<String>> = stmt
        .query_map(params![book_id], |r| r.get::<_, String>(0))?
        .collect();
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;
    use rusqlite::params;
    use tempfile::tempdir;

    #[test]
    fn normalize_splits_and_trims() {
        assert_eq!(
            normalize("fiction, sci-fi ,  space opera"),
            vec!["fiction", "sci-fi", "space opera"]
        );
    }

    #[test]
    fn normalize_drops_empty_pieces() {
        assert_eq!(normalize(", ,fiction,,"), vec!["fiction"]);
    }

    #[test]
    fn normalize_dedups_case_insensitive_preserving_first() {
        assert_eq!(
            normalize("Sci-Fi, sci-fi, SCI-FI, fantasy"),
            vec!["Sci-Fi", "fantasy"]
        );
    }

    #[test]
    fn normalize_many_handles_pre_split_strings() {
        let input = vec!["a", " b ", "A", "", "c"];
        assert_eq!(normalize_many(input), vec!["a", "b", "c"]);
    }

    #[test]
    fn normalize_splits_on_semicolons_and_mixed_separators() {
        assert_eq!(
            normalize("fiction; sci-fi ; space opera"),
            vec!["fiction", "sci-fi", "space opera"]
        );
        // Both separators may appear in the same list.
        assert_eq!(
            normalize("fiction, sci-fi; fantasy"),
            vec!["fiction", "sci-fi", "fantasy"]
        );
    }

    #[test]
    fn normalize_many_splits_embedded_separators_in_each_value() {
        // A single EPUB/MOBI subject carrying a `;`-delimited list becomes
        // several tags rather than one merged tag.
        let input = vec!["Fiction; Sci-Fi", "Drama"];
        assert_eq!(normalize_many(input), vec!["Fiction", "Sci-Fi", "Drama"]);
    }

    #[test]
    fn sync_inserts_and_reads_back() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("cat");
        let mut conn = catalog::init(&cat).unwrap();
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES (?1, ?2, 'epub', '')",
            params!["B", Option::<String>::None],
        )
        .unwrap();
        let book_id = conn.last_insert_rowid();

        let tx = conn.transaction().unwrap();
        sync(&tx, book_id, &["a".into(), "b".into()]).unwrap();
        tx.commit().unwrap();

        let tags = fetch_for_book(&conn, book_id).unwrap();
        assert_eq!(tags, vec!["a", "b"]);
    }

    #[test]
    fn sync_replaces_previous_set() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("cat");
        let mut conn = catalog::init(&cat).unwrap();
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES ('B', NULL, 'epub', '')",
            [],
        )
        .unwrap();
        let book_id = conn.last_insert_rowid();

        let tx = conn.transaction().unwrap();
        sync(&tx, book_id, &["a".into(), "b".into()]).unwrap();
        tx.commit().unwrap();

        let tx = conn.transaction().unwrap();
        sync(&tx, book_id, &["b".into(), "c".into()]).unwrap();
        tx.commit().unwrap();

        let tags = fetch_for_book(&conn, book_id).unwrap();
        assert_eq!(tags, vec!["b", "c"]);
    }

    #[test]
    fn add_for_book_reports_changed_and_unchanged() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("cat");
        let mut conn = catalog::init(&cat).unwrap();
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES ('B', NULL, 'epub', '')",
            [],
        )
        .unwrap();
        let book_id = conn.last_insert_rowid();

        let tx = conn.transaction().unwrap();
        let delta = add_for_book(&tx, book_id, &["sci-fi".into(), "classic".into()]).unwrap();
        tx.commit().unwrap();
        assert_eq!(delta.changed, vec!["sci-fi", "classic"]);
        assert!(delta.unchanged.is_empty());

        let tx = conn.transaction().unwrap();
        let delta = add_for_book(&tx, book_id, &["sci-fi".into(), "epic".into()]).unwrap();
        tx.commit().unwrap();
        assert_eq!(delta.changed, vec!["epic"]);
        assert_eq!(delta.unchanged, vec!["sci-fi"]);

        let tags = fetch_for_book(&conn, book_id).unwrap();
        assert_eq!(tags, vec!["classic", "epic", "sci-fi"]);
    }

    #[test]
    fn add_for_book_matches_existing_case_insensitive() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("cat");
        let mut conn = catalog::init(&cat).unwrap();
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES ('B', NULL, 'epub', '')",
            [],
        )
        .unwrap();
        let book_id = conn.last_insert_rowid();

        let tx = conn.transaction().unwrap();
        add_for_book(&tx, book_id, &["Sci-Fi".into()]).unwrap();
        tx.commit().unwrap();

        let tx = conn.transaction().unwrap();
        let delta = add_for_book(&tx, book_id, &["sci-fi".into()]).unwrap();
        tx.commit().unwrap();
        assert!(delta.changed.is_empty());
        assert_eq!(delta.unchanged, vec!["sci-fi"]);

        // Original case is preserved in storage.
        let tags = fetch_for_book(&conn, book_id).unwrap();
        assert_eq!(tags, vec!["Sci-Fi"]);
    }

    #[test]
    fn remove_for_book_reports_changed_and_unchanged() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("cat");
        let mut conn = catalog::init(&cat).unwrap();
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES ('B', NULL, 'epub', '')",
            [],
        )
        .unwrap();
        let book_id = conn.last_insert_rowid();

        let tx = conn.transaction().unwrap();
        add_for_book(&tx, book_id, &["a".into(), "b".into()]).unwrap();
        tx.commit().unwrap();

        let tx = conn.transaction().unwrap();
        let delta = remove_for_book(&tx, book_id, &["a".into(), "ghost".into()]).unwrap();
        tx.commit().unwrap();
        assert_eq!(delta.changed, vec!["a"]);
        assert_eq!(delta.unchanged, vec!["ghost"]);

        let tags = fetch_for_book(&conn, book_id).unwrap();
        assert_eq!(tags, vec!["b"]);
    }

    #[test]
    fn remove_for_book_is_case_insensitive() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("cat");
        let mut conn = catalog::init(&cat).unwrap();
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES ('B', NULL, 'epub', '')",
            [],
        )
        .unwrap();
        let book_id = conn.last_insert_rowid();

        let tx = conn.transaction().unwrap();
        add_for_book(&tx, book_id, &["Sci-Fi".into()]).unwrap();
        tx.commit().unwrap();

        let tx = conn.transaction().unwrap();
        let delta = remove_for_book(&tx, book_id, &["sci-fi".into()]).unwrap();
        tx.commit().unwrap();
        assert_eq!(delta.changed, vec!["sci-fi"]);
        assert!(delta.unchanged.is_empty());

        let tags = fetch_for_book(&conn, book_id).unwrap();
        assert!(tags.is_empty());
    }

    #[test]
    fn clear_for_book_returns_removed_names_sorted() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("cat");
        let mut conn = catalog::init(&cat).unwrap();
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES ('B', NULL, 'epub', '')",
            [],
        )
        .unwrap();
        let book_id = conn.last_insert_rowid();

        let tx = conn.transaction().unwrap();
        add_for_book(&tx, book_id, &["Zeta".into(), "alpha".into(), "Mu".into()]).unwrap();
        tx.commit().unwrap();

        let tx = conn.transaction().unwrap();
        let removed = clear_for_book(&tx, book_id).unwrap();
        tx.commit().unwrap();
        assert_eq!(removed, vec!["alpha", "Mu", "Zeta"]);
        assert!(fetch_for_book(&conn, book_id).unwrap().is_empty());
    }

    #[test]
    fn clear_for_book_on_empty_set_is_noop() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("cat");
        let mut conn = catalog::init(&cat).unwrap();
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES ('B', NULL, 'epub', '')",
            [],
        )
        .unwrap();
        let book_id = conn.last_insert_rowid();

        let tx = conn.transaction().unwrap();
        let removed = clear_for_book(&tx, book_id).unwrap();
        tx.commit().unwrap();
        assert!(removed.is_empty());
    }

    #[test]
    fn sync_empty_clears_all_book_tags() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("cat");
        let mut conn = catalog::init(&cat).unwrap();
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES ('B', NULL, 'epub', '')",
            [],
        )
        .unwrap();
        let book_id = conn.last_insert_rowid();

        let tx = conn.transaction().unwrap();
        sync(&tx, book_id, &["a".into()]).unwrap();
        tx.commit().unwrap();

        let tx = conn.transaction().unwrap();
        sync(&tx, book_id, &[]).unwrap();
        tx.commit().unwrap();

        let tags = fetch_for_book(&conn, book_id).unwrap();
        assert!(tags.is_empty());
    }
}
