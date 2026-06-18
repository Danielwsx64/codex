use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::device::sync::{Direction, SyncItem};
use crate::device::{pull, push};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub title: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepOutcome {
    Pushed { title: String },
    Pulled { title: String },
    Error(Failure),
}

// A batch applier for a marked subset of a `SyncPlan`. Mirrors `embed::job::Job`:
// the TUI loop polls `advance` one item per tick so progress renders line by line
// and `Esc` can cancel the remainder between items (never mid-copy, matching the
// CLI's `q`). Conflicts never enter here — they are surfaced for manual resolution.
#[derive(Debug)]
pub struct ApplyJob {
    pub total: usize,
    pub completed: usize,
    pub pushed: usize,
    pub pulled: usize,
    pub queue: Vec<SyncItem>,
    pub current: Option<SyncItem>,
    pub failures: Vec<Failure>,
    pub done: bool,
    serial: String,
    mount: PathBuf,
    catalog_dir: PathBuf,
}

impl ApplyJob {
    pub fn new(items: Vec<SyncItem>, serial: &str, mount: &Path, catalog_dir: &Path) -> Self {
        let total = items.len();
        // The queue is consumed back-to-front; reverse so we apply in display order.
        let mut queue = items;
        queue.reverse();
        let done = queue.is_empty();
        let current = queue.last().cloned();
        Self {
            total,
            completed: 0,
            pushed: 0,
            pulled: 0,
            queue,
            current,
            failures: Vec::new(),
            done,
            serial: serial.to_string(),
            mount: mount.to_path_buf(),
            catalog_dir: catalog_dir.to_path_buf(),
        }
    }

    pub fn is_pending(&self) -> bool {
        !self.done
    }

    pub fn advance(&mut self, conn: &mut Connection) -> Option<StepOutcome> {
        if self.done {
            return None;
        }
        let Some(item) = self.queue.pop() else {
            self.done = true;
            self.current = None;
            return None;
        };
        let step = self.apply(conn, &item);
        self.completed += 1;
        self.current = self.queue.last().cloned();
        if self.queue.is_empty() {
            self.done = true;
        }
        Some(step)
    }

    fn apply(&mut self, conn: &mut Connection, item: &SyncItem) -> StepOutcome {
        match item.direction {
            Direction::Push => {
                let book_id = item
                    .book_id
                    .expect("sync push items always carry the catalog book id");
                match push::push(
                    conn,
                    &self.catalog_dir,
                    &self.serial,
                    &self.mount,
                    &book_id.to_string(),
                ) {
                    Ok(_) => {
                        self.pushed += 1;
                        StepOutcome::Pushed {
                            title: item.title.clone(),
                        }
                    }
                    Err(err) => self.fail(item, err.to_string()),
                }
            }
            Direction::Pull => {
                let arg = item.device_path.display().to_string();
                match pull::pull(
                    conn,
                    &self.catalog_dir,
                    &self.serial,
                    &self.mount,
                    &arg,
                    false,
                ) {
                    Ok(_) => {
                        self.pulled += 1;
                        StepOutcome::Pulled {
                            title: item.title.clone(),
                        }
                    }
                    Err(err) => self.fail(item, err.to_string()),
                }
            }
        }
    }

    fn fail(&mut self, item: &SyncItem, reason: String) -> StepOutcome {
        let failure = Failure {
            title: item.title.clone(),
            reason,
        };
        self.failures.push(failure.clone());
        StepOutcome::Error(failure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;
    use crate::catalog::devices;
    use crate::device::sync::PushReason;
    use rusqlite::params;
    use std::fs;
    use tempfile::tempdir;

    // Register a book with a real stored file under `<catalog_dir>/books/<id>/`.
    fn add_book(conn: &Connection, catalog_dir: &Path, title: &str, filename: &str) -> i64 {
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES (?1, 'A', 'epub', '')",
            params![title],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        let rel = format!("books/{id}/{filename}");
        let abs = catalog_dir.join(&rel);
        fs::create_dir_all(abs.parent().unwrap()).unwrap();
        fs::write(&abs, b"book bytes").unwrap();
        conn.execute(
            "UPDATE books SET file_path = ?1 WHERE id = ?2",
            params![rel, id],
        )
        .unwrap();
        id
    }

    fn push_item(book_id: i64, title: &str) -> SyncItem {
        SyncItem {
            direction: Direction::Push,
            book_id: Some(book_id),
            title: title.to_string(),
            device_path: PathBuf::new(),
            push_reason: Some(PushReason::NotOnDevice),
            bytes: None,
        }
    }

    #[test]
    fn applies_a_push_and_records_progress() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("cat");
        let mut conn = catalog::init(&cat).unwrap();
        devices::record_seen(&conn, "AAA").unwrap();
        let id = add_book(&conn, &cat, "Dune", "Dune.epub");
        let mount = tempdir().unwrap();

        let mut job = ApplyJob::new(vec![push_item(id, "Dune")], "AAA", mount.path(), &cat);
        assert!(job.is_pending());
        let step = job.advance(&mut conn);

        assert!(matches!(step, Some(StepOutcome::Pushed { .. })));
        assert!(job.done);
        assert_eq!(job.completed, 1);
        assert_eq!(job.pushed, 1);
        assert!(job.failures.is_empty());
        assert!(mount.path().join("documents/Dune.epub").is_file());
    }

    #[test]
    fn pulls_a_device_file_into_the_catalog() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("cat");
        let mut conn = catalog::init(&cat).unwrap();
        devices::record_seen(&conn, "AAA").unwrap();
        let mount = tempdir().unwrap();
        let docs = mount.path().join("documents");
        fs::create_dir_all(&docs).unwrap();
        // A minimal text book the `cdx add` pipeline accepts.
        fs::write(docs.join("Notes.txt"), b"hello world").unwrap();

        let item = SyncItem {
            direction: Direction::Pull,
            book_id: None,
            title: "Notes".to_string(),
            device_path: PathBuf::from("documents/Notes.txt"),
            push_reason: None,
            bytes: None,
        };
        let mut job = ApplyJob::new(vec![item], "AAA", mount.path(), &cat);
        let step = job.advance(&mut conn);

        assert!(matches!(step, Some(StepOutcome::Pulled { .. })));
        assert_eq!(job.pulled, 1);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM books", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn failure_is_recorded_and_batch_continues() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("cat");
        let mut conn = catalog::init(&cat).unwrap();
        devices::record_seen(&conn, "AAA").unwrap();
        // First book has no stored file on disk → push fails.
        conn.execute(
            "INSERT INTO books (title, author, format, file_path)
             VALUES ('Ghost', 'A', 'epub', 'books/1/Ghost.epub')",
            [],
        )
        .unwrap();
        let ghost = conn.last_insert_rowid();
        let good = add_book(&conn, &cat, "Dune", "Dune.epub");
        let mount = tempdir().unwrap();

        let mut job = ApplyJob::new(
            vec![push_item(ghost, "Ghost"), push_item(good, "Dune")],
            "AAA",
            mount.path(),
            &cat,
        );
        let first = job.advance(&mut conn);
        assert!(matches!(first, Some(StepOutcome::Error(_))));
        assert!(!job.done, "the batch keeps going after a failure");
        let second = job.advance(&mut conn);
        assert!(matches!(second, Some(StepOutcome::Pushed { .. })));

        assert!(job.done);
        assert_eq!(job.completed, 2);
        assert_eq!(job.pushed, 1);
        assert_eq!(job.failures.len(), 1);
        assert_eq!(job.failures[0].title, "Ghost");
    }

    #[test]
    fn empty_job_starts_done() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("cat");
        let mount = tempdir().unwrap();
        let job = ApplyJob::new(Vec::new(), "AAA", mount.path(), &cat);
        assert!(job.done);
        assert!(!job.is_pending());
    }
}
