use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::catalog::books::{self, Book};
use crate::embed::{self, EmbedOutcome};
use crate::import::Format;

#[derive(Debug, Clone)]
pub struct Item {
    pub id: i64,
    pub title: String,
    pub abs_path: PathBuf,
    pub format: Format,
    pub book: Book,
}

#[derive(Debug, Clone)]
pub struct Failure {
    pub id: i64,
    pub title: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub enum StepOutcome {
    Synced {
        id: i64,
        title: String,
    },
    Unsupported {
        id: i64,
        title: String,
        format: Format,
    },
    Error(Failure),
}

#[derive(Debug)]
pub struct Job {
    pub total: usize,
    pub completed: usize,
    pub succeeded: usize,
    pub queue: Vec<Item>,
    pub current: Option<Item>,
    pub failures: Vec<Failure>,
    pub done: bool,
}

impl Job {
    pub fn from_books(rows: &[Book], catalog_dir: &Path) -> Self {
        let total = rows.len();
        let mut queue: Vec<Item> = Vec::new();
        let mut failures: Vec<Failure> = Vec::new();
        for b in rows {
            let abs_path = catalog_dir.join(&b.file_path);
            match Format::parse_label(&b.format) {
                Some(format @ (Format::Epub | Format::Pdf)) => {
                    queue.push(Item {
                        id: b.id,
                        title: b.title.clone(),
                        abs_path,
                        format,
                        book: b.clone(),
                    });
                }
                Some(other) => failures.push(Failure {
                    id: b.id,
                    title: b.title.clone(),
                    reason: format!("embed not supported for {}", other.label()),
                }),
                None => failures.push(Failure {
                    id: b.id,
                    title: b.title.clone(),
                    reason: format!("unknown format `{}`", b.format),
                }),
            }
        }
        // pre-counted failures are already "completed" — they contribute to the bar.
        let completed = failures.len();
        // queue is consumed back-to-front; reverse so we visit by display order.
        queue.reverse();
        let done = queue.is_empty();
        let current = queue.last().cloned();
        Self {
            total,
            completed,
            succeeded: 0,
            queue,
            current,
            failures,
            done,
        }
    }

    pub fn is_pending(&self) -> bool {
        !self.done
    }

    pub fn advance(&mut self, conn: &Connection) -> Option<StepOutcome> {
        if self.done {
            return None;
        }
        let Some(item) = self.queue.pop() else {
            self.done = true;
            self.current = None;
            return None;
        };
        let step = match embed::embed_into_file(&item.abs_path, item.format, &item.book) {
            Ok(EmbedOutcome::Written) => {
                let _ = books::mark_embed_synced(conn, item.id);
                // The embed rewrote the file, so its whole-file hash changed.
                // Record the post-embed hash too, so re-importing this stored
                // copy is recognized as a duplicate. Best-effort.
                if let Ok(hash) = crate::fingerprint::hash_full(&item.abs_path) {
                    let _ = books::record_fingerprints(
                        conn,
                        item.id,
                        &[crate::fingerprint::Fingerprint {
                            kind: crate::fingerprint::Kind::Full,
                            hash,
                        }],
                    );
                }
                self.succeeded += 1;
                StepOutcome::Synced {
                    id: item.id,
                    title: item.title.clone(),
                }
            }
            Ok(EmbedOutcome::Unsupported { format }) => {
                let _ = books::mark_embed_unsupported(conn, item.id);
                self.failures.push(Failure {
                    id: item.id,
                    title: item.title.clone(),
                    reason: format!("embed not supported for {}", format.label()),
                });
                StepOutcome::Unsupported {
                    id: item.id,
                    title: item.title.clone(),
                    format,
                }
            }
            Err(err) => {
                let failure = Failure {
                    id: item.id,
                    title: item.title.clone(),
                    reason: err.to_string(),
                };
                self.failures.push(failure.clone());
                StepOutcome::Error(failure)
            }
        };
        self.completed += 1;
        self.current = self.queue.last().cloned();
        if self.queue.is_empty() {
            self.done = true;
        }
        Some(step)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;
    use rusqlite::params;
    use tempfile::tempdir;

    fn insert_book(conn: &Connection, title: &str, format: &str, file_path: &str) -> i64 {
        conn.execute(
            "INSERT INTO books (title, format, file_path) VALUES (?1, ?2, ?3)",
            params![title, format, file_path],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn from_books_classifies_unsupported_format_upfront() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        let conn = catalog::init(&cat).unwrap();
        let id = insert_book(&conn, "Old MOBI", "mobi", "books/1/old.mobi");
        let book = books::handle_inspect(&conn, &id.to_string()).unwrap();
        let job = Job::from_books(&[book], &cat);
        assert!(job.done);
        assert_eq!(job.failures.len(), 1);
        assert!(job.failures[0].reason.contains("not supported"));
    }

    #[test]
    fn advance_on_missing_file_records_error_and_continues() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        let conn = catalog::init(&cat).unwrap();
        let id = insert_book(&conn, "Ghost", "epub", "books/1/ghost.epub");
        let book = books::handle_inspect(&conn, &id.to_string()).unwrap();
        let mut job = Job::from_books(&[book], &cat);
        let step = job.advance(&conn);
        assert!(matches!(step, Some(StepOutcome::Error(_))));
        assert!(job.done);
        assert_eq!(job.failures.len(), 1);
    }
}
