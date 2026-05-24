use rusqlite::{params, Transaction};

pub fn normalize(input: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for piece in input.split(',') {
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
    out
}

pub fn normalize_many<I, S>(values: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out: Vec<String> = Vec::new();
    for v in values {
        let trimmed = v.as_ref().trim();
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
    out
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
