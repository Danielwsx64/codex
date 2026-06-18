use rusqlite::types::Value;
use rusqlite::Connection;

use crate::catalog::books::{row_to_book, Book, Error, Result};
use crate::catalog::tags;

// The metadata field a "folder" view groups books by. Author and rating are
// scalar columns on `books`; tags is many-to-many via `book_tags`, so a book
// can land in several tag groups at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupBy {
    Author,
    Tag,
    Rating,
}

impl GroupBy {
    pub fn as_str(self) -> &'static str {
        match self {
            GroupBy::Author => "author",
            GroupBy::Tag => "tag",
            GroupBy::Rating => "rating",
        }
    }
}

// One folder: the distinct value plus how many books fall under it. `value` is
// None for the catch-all group — books with no author / no tags / no rating.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub value: Option<String>,
    pub count: usize,
}

// Same column order as `handle_ls`, so `row_to_book` maps the row unchanged.
const BOOK_COLUMNS: &str = "id, title, author, format, file_path, added_at,
     description, series_name, series_index, rating,
     isbn, publisher, language, published_date,
     embed_status, embed_synced_at";

pub fn list_groups(conn: &Connection, by: GroupBy) -> Result<Vec<Group>> {
    match by {
        GroupBy::Author => list_scalar_groups(
            conn,
            "SELECT CASE WHEN author IS NULL OR TRIM(author) = '' THEN NULL ELSE author END AS grp,
                    COUNT(*) AS n
             FROM books
             GROUP BY grp
             ORDER BY grp IS NULL, LOWER(grp)",
        ),
        GroupBy::Rating => list_scalar_groups(
            conn,
            "SELECT CASE WHEN rating IS NULL THEN NULL ELSE CAST(rating AS TEXT) END AS grp,
                    COUNT(*) AS n
             FROM books
             GROUP BY rating
             ORDER BY rating IS NULL, rating DESC",
        ),
        GroupBy::Tag => list_tag_groups(conn),
    }
}

fn list_scalar_groups(conn: &Connection, sql: &str) -> Result<Vec<Group>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |r| {
        let value: Option<String> = r.get(0)?;
        let count: i64 = r.get(1)?;
        Ok(Group {
            value,
            count: count.max(0) as usize,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn list_tag_groups(conn: &Connection) -> Result<Vec<Group>> {
    let mut groups: Vec<Group> = {
        let mut stmt = conn.prepare(
            "SELECT t.name AS grp, COUNT(DISTINCT bt.book_id) AS n
             FROM tags t
             JOIN book_tags bt ON bt.tag_id = t.id
             GROUP BY t.id
             ORDER BY LOWER(t.name)",
        )?;
        let rows = stmt.query_map([], |r| {
            let value: String = r.get(0)?;
            let count: i64 = r.get(1)?;
            Ok(Group {
                value: Some(value),
                count: count.max(0) as usize,
            })
        })?;
        rows.collect::<rusqlite::Result<_>>()?
    };

    // Untagged books form the catch-all group, listed last.
    let untagged: i64 = conn.query_row(
        "SELECT COUNT(*) FROM books b
         WHERE NOT EXISTS (SELECT 1 FROM book_tags bt WHERE bt.book_id = b.id)",
        [],
        |r| r.get(0),
    )?;
    if untagged > 0 {
        groups.push(Group {
            value: None,
            count: untagged as usize,
        });
    }
    Ok(groups)
}

// Exact membership of a single group — not the substring matching of
// `handle_search`. `value` None selects the catch-all group.
pub fn books_in_group(conn: &Connection, by: GroupBy, value: Option<&str>) -> Result<Vec<Book>> {
    let (clause, param): (&str, Option<Value>) = match (by, value) {
        (GroupBy::Author, Some(a)) => ("b.author = ?1", Some(Value::Text(a.to_string()))),
        (GroupBy::Author, None) => ("b.author IS NULL OR TRIM(b.author) = ''", None),
        (GroupBy::Rating, Some(r)) => {
            let n: i64 = r.parse().map_err(|_| Error::Validation {
                field: "rating",
                reason: format!("`{r}` is not a valid rating"),
            })?;
            ("b.rating = ?1", Some(Value::Integer(n)))
        }
        (GroupBy::Rating, None) => ("b.rating IS NULL", None),
        (GroupBy::Tag, Some(name)) => (
            "EXISTS (SELECT 1 FROM book_tags bt JOIN tags t ON t.id = bt.tag_id \
             WHERE bt.book_id = b.id AND t.name = ?1 COLLATE NOCASE)",
            Some(Value::Text(name.to_string())),
        ),
        (GroupBy::Tag, None) => (
            "NOT EXISTS (SELECT 1 FROM book_tags bt WHERE bt.book_id = b.id)",
            None,
        ),
    };

    let sql =
        format!("SELECT {BOOK_COLUMNS} FROM books b WHERE {clause} ORDER BY LOWER(title), id");
    let mut stmt = conn.prepare(&sql)?;
    let mut books: Vec<Book> = match param {
        Some(p) => stmt
            .query_map([p], row_to_book)?
            .collect::<rusqlite::Result<_>>()?,
        None => stmt
            .query_map([], row_to_book)?
            .collect::<rusqlite::Result<_>>()?,
    };
    for b in &mut books {
        b.tags = tags::fetch_for_book(conn, b.id)?;
    }
    Ok(books)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;
    use rusqlite::params;
    use tempfile::tempdir;

    fn open_fresh() -> (tempfile::TempDir, Connection) {
        let dir = tempdir().unwrap();
        let conn = catalog::init(&dir.path().join("cat")).unwrap();
        (dir, conn)
    }

    fn insert(conn: &Connection, title: &str, author: Option<&str>, rating: Option<i64>) -> i64 {
        conn.execute(
            "INSERT INTO books (title, author, format, file_path, rating)
             VALUES (?1, ?2, 'epub', '', ?3)",
            params![title, author, rating],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn add_tag(conn: &Connection, book_id: i64, name: &str) {
        conn.execute(
            "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
            params![name],
        )
        .unwrap();
        let tag_id: i64 = conn
            .query_row(
                "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
                params![name],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO book_tags (book_id, tag_id) VALUES (?1, ?2)",
            params![book_id, tag_id],
        )
        .unwrap();
    }

    #[test]
    fn author_groups_collapse_null_and_empty() {
        let (_d, conn) = open_fresh();
        insert(&conn, "A", Some("Jane Austen"), None);
        insert(&conn, "B", Some("Jane Austen"), None);
        insert(&conn, "C", None, None);
        insert(&conn, "D", Some(""), None);

        let groups = list_groups(&conn, GroupBy::Author).unwrap();
        // Two named-author rows collapse into one group; null + "" into the None group.
        assert_eq!(
            groups,
            vec![
                Group {
                    value: Some("Jane Austen".into()),
                    count: 2
                },
                Group {
                    value: None,
                    count: 2
                },
            ]
        );

        let books = books_in_group(&conn, GroupBy::Author, Some("Jane Austen")).unwrap();
        assert_eq!(books.len(), 2);
        let none = books_in_group(&conn, GroupBy::Author, None).unwrap();
        assert_eq!(none.len(), 2);
    }

    #[test]
    fn tag_groups_count_multi_tagged_books_in_each_folder() {
        let (_d, conn) = open_fresh();
        let a = insert(&conn, "A", Some("X"), None);
        let b = insert(&conn, "B", Some("Y"), None);
        insert(&conn, "C", Some("Z"), None); // untagged
        add_tag(&conn, a, "fiction");
        add_tag(&conn, a, "sci-fi");
        add_tag(&conn, b, "fiction");

        let groups = list_groups(&conn, GroupBy::Tag).unwrap();
        assert_eq!(
            groups,
            vec![
                Group {
                    value: Some("fiction".into()),
                    count: 2
                },
                Group {
                    value: Some("sci-fi".into()),
                    count: 1
                },
                Group {
                    value: None,
                    count: 1
                },
            ]
        );

        // Book A appears under both "fiction" and "sci-fi".
        assert_eq!(
            books_in_group(&conn, GroupBy::Tag, Some("fiction"))
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            books_in_group(&conn, GroupBy::Tag, Some("sci-fi"))
                .unwrap()
                .len(),
            1
        );
        assert_eq!(books_in_group(&conn, GroupBy::Tag, None).unwrap().len(), 1);
    }

    #[test]
    fn rating_groups_sort_desc_with_unrated_last() {
        let (_d, conn) = open_fresh();
        insert(&conn, "A", Some("X"), Some(5));
        insert(&conn, "B", Some("Y"), Some(3));
        insert(&conn, "C", Some("Z"), Some(5));
        insert(&conn, "D", Some("W"), None);

        let groups = list_groups(&conn, GroupBy::Rating).unwrap();
        assert_eq!(
            groups,
            vec![
                Group {
                    value: Some("5".into()),
                    count: 2
                },
                Group {
                    value: Some("3".into()),
                    count: 1
                },
                Group {
                    value: None,
                    count: 1
                },
            ]
        );

        assert_eq!(
            books_in_group(&conn, GroupBy::Rating, Some("5"))
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            books_in_group(&conn, GroupBy::Rating, None).unwrap().len(),
            1
        );
    }

    #[test]
    fn empty_catalog_has_no_groups() {
        let (_d, conn) = open_fresh();
        assert!(list_groups(&conn, GroupBy::Author).unwrap().is_empty());
        assert!(list_groups(&conn, GroupBy::Tag).unwrap().is_empty());
        assert!(list_groups(&conn, GroupBy::Rating).unwrap().is_empty());
    }
}
