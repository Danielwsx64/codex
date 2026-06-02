use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState,
};
use ratatui::Frame;

use crate::catalog::books::{self, Book, EmbedStatus};
use crate::catalog::columns::LibraryColumn;
use crate::catalog::settings;
use crate::catalog::{self};
use crate::config::Registry;
use crate::embed::job::Job;
use crate::embed::{self, EmbedOutcome};
use crate::import;
use crate::tui::help::{Binding, Section};
use crate::tui::widgets::{centered_rect, render_modal, StatusMessage};

const TABLE_BINDINGS: &[Binding] = &[
    Binding {
        keys: "↑↓ / j k",
        desc: "move selection",
    },
    Binding {
        keys: "i",
        desc: "inspect book",
    },
    Binding {
        keys: "e",
        desc: "edit metadata",
    },
    Binding {
        keys: "a",
        desc: "add files (file tree)",
    },
    Binding {
        keys: "d / Delete",
        desc: "remove book",
    },
    Binding {
        keys: "c",
        desc: "configure columns",
    },
    Binding {
        keys: "Ctrl+W",
        desc: "embed metadata into all files",
    },
];

const INSPECT_BINDINGS: &[Binding] = &[
    Binding {
        keys: "e",
        desc: "edit metadata",
    },
    Binding {
        keys: "w",
        desc: "embed into file",
    },
    Binding {
        keys: "Esc / Enter / i",
        desc: "close",
    },
];

const CONFIRM_RM_BINDINGS: &[Binding] = &[
    Binding {
        keys: "y / Enter",
        desc: "delete book and file",
    },
    Binding {
        keys: "k",
        desc: "delete row, keep file in cwd",
    },
    Binding {
        keys: "n / Esc",
        desc: "cancel",
    },
];

const ADD_TREE_BINDINGS: &[Binding] = &[
    Binding {
        keys: "↑↓ / j k",
        desc: "move cursor",
    },
    Binding {
        keys: "Space",
        desc: "toggle selection",
    },
    Binding {
        keys: "Enter",
        desc: "open dir / import file(s)",
    },
    Binding {
        keys: "Backspace",
        desc: "go to parent directory",
    },
];

const EDIT_BINDINGS: &[Binding] = &[
    Binding {
        keys: "Tab / ↓",
        desc: "next field",
    },
    Binding {
        keys: "Shift+Tab / ↑",
        desc: "previous field",
    },
    Binding {
        keys: "Enter (on Save)",
        desc: "commit changes",
    },
    Binding {
        keys: "Enter (on Cancel)",
        desc: "discard changes",
    },
];

const EDIT_RATING_BINDINGS: &[Binding] = &[
    Binding {
        keys: "←→ / h l",
        desc: "adjust rating by 1",
    },
    Binding {
        keys: "0-5",
        desc: "set rating directly",
    },
    Binding {
        keys: "Backspace",
        desc: "decrement rating",
    },
    Binding {
        keys: "Tab / ↓",
        desc: "next field",
    },
];

const EMBED_JOB_BINDINGS: &[Binding] = &[
    Binding {
        keys: "Esc",
        desc: "cancel job / close summary",
    },
    Binding {
        keys: "Enter",
        desc: "close summary (when done)",
    },
];

const ADD_RESULT_BINDINGS: &[Binding] = &[
    Binding {
        keys: "↑↓ / j k",
        desc: "scroll",
    },
    Binding {
        keys: "Esc / Enter",
        desc: "close summary",
    },
];

const COLUMNS_BINDINGS: &[Binding] = &[
    Binding {
        keys: "↑↓ / j k",
        desc: "move cursor",
    },
    Binding {
        keys: "Space",
        desc: "toggle column",
    },
    Binding {
        keys: "Enter",
        desc: "save",
    },
];

pub fn help_sections(state: &State) -> Vec<Section> {
    match state.overlay.as_ref() {
        None => vec![Section {
            title: "Library",
            bindings: TABLE_BINDINGS,
        }],
        Some(Overlay::Inspect { .. }) => vec![Section {
            title: "Inspect",
            bindings: INSPECT_BINDINGS,
        }],
        Some(Overlay::ConfirmRm { .. }) => vec![Section {
            title: "Confirm remove",
            bindings: CONFIRM_RM_BINDINGS,
        }],
        Some(Overlay::AddTree(_)) => vec![Section {
            title: "Add files",
            bindings: ADD_TREE_BINDINGS,
        }],
        Some(Overlay::AddResult(_)) => vec![Section {
            title: "Add summary",
            bindings: ADD_RESULT_BINDINGS,
        }],
        Some(Overlay::Edit(edit_state)) => {
            let mut sections = vec![Section {
                title: "Edit metadata",
                bindings: EDIT_BINDINGS,
            }];
            if edit_state.focus == edit::Focus::Rating {
                sections.push(Section {
                    title: "Rating field",
                    bindings: EDIT_RATING_BINDINGS,
                });
            }
            sections
        }
        Some(Overlay::EmbedJob(_)) => vec![Section {
            title: "Embed metadata",
            bindings: EMBED_JOB_BINDINGS,
        }],
        Some(Overlay::Columns(_)) => vec![Section {
            title: "Columns",
            bindings: COLUMNS_BINDINGS,
        }],
    }
}

pub mod add_result;
pub mod columns;
pub mod edit;
pub mod embed_job;

#[derive(Debug)]
pub struct State {
    pub catalog: Option<CatalogContext>,
    pub rows: Vec<Book>,
    pub cursor: usize,
    pub overlay: Option<Overlay>,
    pub load_error: Option<String>,
    pub cwd: PathBuf,
    pub columns: Vec<LibraryColumn>,
}

#[derive(Debug, Clone)]
pub struct CatalogContext {
    pub name: String,
    pub dir: PathBuf,
}

#[derive(Debug)]
pub enum Overlay {
    Inspect {
        book: Box<Book>,
        absolute_path: PathBuf,
    },
    ConfirmRm {
        id: i64,
        title: String,
    },
    AddTree(AddTreeState),
    AddResult(add_result::State),
    Edit(Box<edit::State>),
    EmbedJob(Box<Job>),
    Columns(columns::State),
}

#[derive(Debug)]
pub struct AddTreeState {
    pub cwd: PathBuf,
    pub entries: Vec<TreeEntry>,
    pub cursor: usize,
    pub selected: BTreeSet<PathBuf>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum TreeEntry {
    Parent,
    Dir { path: PathBuf, name: String },
    File { path: PathBuf, name: String },
}

#[derive(Debug)]
pub enum LibraryAction {
    None,
    Back,
    OpenPalette,
    Status(StatusMessage),
}

impl State {
    pub fn load(registry: &Registry) -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let mut state = Self {
            catalog: None,
            rows: Vec::new(),
            cursor: 0,
            overlay: None,
            load_error: None,
            cwd,
            columns: LibraryColumn::DEFAULT.to_vec(),
        };
        if let Ok(entry) = registry.resolve(None) {
            state.catalog = Some(CatalogContext {
                name: entry.name.clone(),
                dir: entry.path.clone(),
            });
            state.refresh();
            state.reload_columns();
        }
        state
    }

    fn refresh(&mut self) {
        let Some(ctx) = self.catalog.clone() else {
            return;
        };
        match list_rows(&ctx.dir) {
            Ok(rows) => {
                self.rows = rows;
                if self.cursor >= self.rows.len() {
                    self.cursor = self.rows.len().saturating_sub(1);
                }
                self.load_error = None;
            }
            Err(err) => {
                self.rows.clear();
                self.cursor = 0;
                self.load_error = Some(err);
            }
        }
    }

    fn reload_columns(&mut self) {
        let Some(ctx) = self.catalog.clone() else {
            return;
        };
        if let Ok(conn) = catalog::open_existing(&ctx.dir) {
            if let Ok(cols) = settings::load_library_columns(&conn) {
                self.columns = cols;
            }
        }
    }
}

pub fn handle_key(state: &mut State, key: KeyEvent) -> LibraryAction {
    if state.overlay.is_some() {
        return handle_overlay_key(state, key);
    }
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            if !state.rows.is_empty() {
                state.cursor = (state.cursor + 1) % state.rows.len();
            }
            LibraryAction::None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if !state.rows.is_empty() {
                state.cursor = (state.cursor + state.rows.len() - 1) % state.rows.len();
            }
            LibraryAction::None
        }
        KeyCode::Char('i') => open_inspect(state),
        KeyCode::Char('e') => open_edit_from_table(state),
        KeyCode::Char('c') => open_columns(state),
        KeyCode::Char('w')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            open_embed_job(state)
        }
        KeyCode::Char('a') => open_add_tree(state),
        KeyCode::Char('d') | KeyCode::Delete => open_confirm_rm(state),
        KeyCode::Esc => LibraryAction::Back,
        KeyCode::Char(':') => LibraryAction::OpenPalette,
        _ => LibraryAction::None,
    }
}

fn handle_overlay_key(state: &mut State, key: KeyEvent) -> LibraryAction {
    match state.overlay {
        Some(Overlay::Inspect { .. }) => match key.code {
            KeyCode::Char('e') => open_edit_from_inspect(state),
            KeyCode::Char('w') => embed_from_inspect(state),
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('i') | KeyCode::Char('q') => {
                state.overlay = None;
                LibraryAction::None
            }
            _ => LibraryAction::None,
        },
        Some(Overlay::ConfirmRm { id, .. }) => handle_confirm_key(state, key, id),
        Some(Overlay::AddTree(_)) => handle_tree_key(state, key),
        Some(Overlay::AddResult(_)) => handle_add_result_key(state, key),
        Some(Overlay::Edit(_)) => handle_edit_key(state, key),
        Some(Overlay::EmbedJob(_)) => handle_embed_job_key(state, key),
        Some(Overlay::Columns(_)) => handle_columns_key(state, key),
        None => LibraryAction::None,
    }
}

fn open_inspect(state: &mut State) -> LibraryAction {
    let Some(ctx) = state.catalog.clone() else {
        return LibraryAction::None;
    };
    let Some(book) = state.rows.get(state.cursor).cloned() else {
        return LibraryAction::None;
    };
    let absolute_path = ctx.dir.join(&book.file_path);
    state.overlay = Some(Overlay::Inspect {
        book: Box::new(book),
        absolute_path,
    });
    LibraryAction::None
}

fn open_edit_from_table(state: &mut State) -> LibraryAction {
    if state.catalog.is_none() {
        return LibraryAction::None;
    }
    let Some(book) = state.rows.get(state.cursor) else {
        return LibraryAction::None;
    };
    state.overlay = Some(Overlay::Edit(Box::new(edit::State::from_book(
        book,
        edit::Origin::Table,
    ))));
    LibraryAction::None
}

fn open_edit_from_inspect(state: &mut State) -> LibraryAction {
    let book = match state.overlay.as_ref() {
        Some(Overlay::Inspect { book, .. }) => book.as_ref().clone(),
        _ => return LibraryAction::None,
    };
    state.overlay = Some(Overlay::Edit(Box::new(edit::State::from_book(
        &book,
        edit::Origin::Inspect,
    ))));
    LibraryAction::None
}

fn handle_edit_key(state: &mut State, key: KeyEvent) -> LibraryAction {
    let Some(ctx) = state.catalog.clone() else {
        state.overlay = None;
        return LibraryAction::Status(StatusMessage::error("no catalog selected"));
    };
    let Some(Overlay::Edit(edit_state)) = state.overlay.as_mut() else {
        return LibraryAction::None;
    };
    let action = edit::handle_key(edit_state.as_mut(), key, &ctx.dir);
    match action {
        edit::EditAction::None => LibraryAction::None,
        edit::EditAction::Cancel => {
            let origin = edit_state.origin;
            close_edit(state, origin, None)
        }
        edit::EditAction::Saved(book) => {
            let origin = edit_state.origin;
            let title = book.title.clone();
            close_edit(state, origin, Some(*book));
            LibraryAction::Status(StatusMessage::info(format!("saved `{title}`")))
        }
    }
}

fn close_edit(state: &mut State, origin: edit::Origin, saved: Option<Book>) -> LibraryAction {
    state.refresh();
    let Some(ctx) = state.catalog.clone() else {
        state.overlay = None;
        return LibraryAction::None;
    };
    match (origin, saved) {
        (edit::Origin::Inspect, Some(book)) => {
            state.cursor = state
                .rows
                .iter()
                .position(|b| b.id == book.id)
                .unwrap_or(state.cursor);
            let absolute_path = ctx.dir.join(&book.file_path);
            state.overlay = Some(Overlay::Inspect {
                book: Box::new(book),
                absolute_path,
            });
        }
        (edit::Origin::Inspect, None) => {
            if let Some(book) = state.rows.get(state.cursor).cloned() {
                let absolute_path = ctx.dir.join(&book.file_path);
                state.overlay = Some(Overlay::Inspect {
                    book: Box::new(book),
                    absolute_path,
                });
            } else {
                state.overlay = None;
            }
        }
        (edit::Origin::Table, saved) => {
            if let Some(book) = saved {
                state.cursor = state
                    .rows
                    .iter()
                    .position(|b| b.id == book.id)
                    .unwrap_or(state.cursor);
            }
            state.overlay = None;
        }
    }
    LibraryAction::None
}

fn embed_from_inspect(state: &mut State) -> LibraryAction {
    let Some(ctx) = state.catalog.clone() else {
        return LibraryAction::Status(StatusMessage::error("no catalog selected"));
    };
    let (path, format, book) = match state.overlay.as_ref() {
        Some(Overlay::Inspect {
            book,
            absolute_path,
        }) => {
            let format = match import::Format::parse_label(&book.format) {
                Some(f) => f,
                None => {
                    return LibraryAction::Status(StatusMessage::error(format!(
                        "unknown format `{}`",
                        book.format
                    )));
                }
            };
            (absolute_path.clone(), format, book.as_ref().clone())
        }
        _ => return LibraryAction::None,
    };
    // Respect the persisted state: don't re-touch files already synced and
    // don't retry formats we've already classified as impossible to embed.
    match book.embed_status {
        EmbedStatus::Synced => {
            return LibraryAction::Status(StatusMessage::info(
                "already synced — edit metadata first to mark it pending",
            ));
        }
        EmbedStatus::Unsupported => {
            return LibraryAction::Status(StatusMessage::error(format!(
                "embed not supported for {}",
                format.label()
            )));
        }
        EmbedStatus::Pending => {}
    }
    match embed::embed_into_file(&path, format, &book) {
        Ok(EmbedOutcome::Written) => {
            if let Ok(conn) = catalog::open_existing(&ctx.dir) {
                let _ = books::mark_embed_synced(&conn, book.id);
            }
            state.refresh();
            LibraryAction::Status(StatusMessage::info(format!(
                "metadata embedded in {}",
                path.display()
            )))
        }
        Ok(EmbedOutcome::Unsupported { format }) => {
            if let Ok(conn) = catalog::open_existing(&ctx.dir) {
                let _ = books::mark_embed_unsupported(&conn, book.id);
            }
            state.refresh();
            LibraryAction::Status(StatusMessage::error(format!(
                "embed not supported for {}",
                format.label()
            )))
        }
        Err(err) => LibraryAction::Status(StatusMessage::error(format!("embed failed: {err}"))),
    }
}

pub fn captures_text_input(state: &State) -> bool {
    match state.overlay.as_ref() {
        Some(Overlay::Edit(edit_state)) => edit::captures_text_input(edit_state),
        _ => false,
    }
}

fn open_columns(state: &mut State) -> LibraryAction {
    if state.catalog.is_none() {
        return LibraryAction::Status(StatusMessage::error("no catalog selected"));
    }
    let picker = columns::State::from_active(&state.columns);
    state.overlay = Some(Overlay::Columns(picker));
    LibraryAction::None
}

fn handle_columns_key(state: &mut State, key: KeyEvent) -> LibraryAction {
    let Some(ctx) = state.catalog.clone() else {
        state.overlay = None;
        return LibraryAction::None;
    };
    let Some(Overlay::Columns(picker)) = state.overlay.as_mut() else {
        return LibraryAction::None;
    };
    let action = columns::handle_key(picker, key, &ctx.dir);
    match action {
        columns::ColumnsAction::None => LibraryAction::None,
        columns::ColumnsAction::Cancel => {
            state.overlay = None;
            LibraryAction::None
        }
        columns::ColumnsAction::Saved(cols) => {
            state.columns = cols;
            state.overlay = None;
            LibraryAction::Status(StatusMessage::info("columns saved"))
        }
    }
}

fn open_embed_job(state: &mut State) -> LibraryAction {
    let Some(ctx) = state.catalog.clone() else {
        return LibraryAction::Status(StatusMessage::error("no catalog selected"));
    };
    let pending: Vec<Book> = state
        .rows
        .iter()
        .filter(|b| b.embed_status == EmbedStatus::Pending)
        .cloned()
        .collect();
    if pending.is_empty() {
        return LibraryAction::Status(StatusMessage::info(
            "nothing to embed (no books pending sync)",
        ));
    }
    let job = Job::from_books(&pending, &ctx.dir);
    state.overlay = Some(Overlay::EmbedJob(Box::new(job)));
    LibraryAction::None
}

fn handle_embed_job_key(state: &mut State, key: KeyEvent) -> LibraryAction {
    let Some(Overlay::EmbedJob(job)) = state.overlay.as_mut() else {
        return LibraryAction::None;
    };
    match key.code {
        KeyCode::Esc => {
            if job.done {
                state.overlay = None;
            } else {
                // cancel: stop pending work, mark done with current progress kept.
                job.queue.clear();
                job.done = true;
                job.current = None;
            }
            LibraryAction::None
        }
        KeyCode::Enter if job.done => {
            state.overlay = None;
            LibraryAction::None
        }
        _ => LibraryAction::None,
    }
}

pub fn has_pending_embed_job(state: &State) -> bool {
    matches!(state.overlay.as_ref(), Some(Overlay::EmbedJob(job)) if job.is_pending())
}

pub fn advance_embed_job(state: &mut State) {
    let Some(ctx) = state.catalog.clone() else {
        return;
    };
    let Some(Overlay::EmbedJob(job)) = state.overlay.as_mut() else {
        return;
    };
    let Ok(conn) = catalog::open_existing(&ctx.dir) else {
        return;
    };
    job.advance(&conn);
}

fn open_confirm_rm(state: &mut State) -> LibraryAction {
    let Some(book) = state.rows.get(state.cursor) else {
        return LibraryAction::None;
    };
    state.overlay = Some(Overlay::ConfirmRm {
        id: book.id,
        title: book.title.clone(),
    });
    LibraryAction::None
}

fn handle_confirm_key(state: &mut State, key: KeyEvent, id: i64) -> LibraryAction {
    match key.code {
        KeyCode::Char('y') | KeyCode::Enter => {
            state.overlay = None;
            do_remove(state, id, false)
        }
        KeyCode::Char('k') => {
            state.overlay = None;
            do_remove(state, id, true)
        }
        KeyCode::Char('n') | KeyCode::Esc => {
            state.overlay = None;
            LibraryAction::None
        }
        _ => LibraryAction::None,
    }
}

fn do_remove(state: &mut State, id: i64, keep: bool) -> LibraryAction {
    let Some(ctx) = state.catalog.clone() else {
        return LibraryAction::Status(StatusMessage::error("no catalog selected"));
    };
    match remove_book(&ctx.dir, id, keep) {
        Ok(out) => {
            state.refresh();
            let msg = match &out.kept_at {
                Some(p) => format!("removed `{}`; file kept at {}", out.book.title, p.display()),
                None => format!("removed `{}`", out.book.title),
            };
            LibraryAction::Status(StatusMessage::info(msg))
        }
        Err(err) => LibraryAction::Status(StatusMessage::error(err)),
    }
}

fn open_add_tree(state: &mut State) -> LibraryAction {
    if state.catalog.is_none() {
        return LibraryAction::Status(StatusMessage::error("no catalog selected"));
    }
    let cwd = state.cwd.clone();
    match read_tree(&cwd) {
        Ok(entries) => {
            state.overlay = Some(Overlay::AddTree(AddTreeState {
                cwd,
                entries,
                cursor: 0,
                selected: BTreeSet::new(),
                error: None,
            }));
            LibraryAction::None
        }
        Err(err) => LibraryAction::Status(StatusMessage::error(err)),
    }
}

fn handle_tree_key(state: &mut State, key: KeyEvent) -> LibraryAction {
    match key.code {
        KeyCode::Esc => {
            state.overlay = None;
            LibraryAction::None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(Overlay::AddTree(tree)) = state.overlay.as_mut() {
                if !tree.entries.is_empty() {
                    tree.cursor = (tree.cursor + 1) % tree.entries.len();
                }
            }
            LibraryAction::None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(Overlay::AddTree(tree)) = state.overlay.as_mut() {
                if !tree.entries.is_empty() {
                    tree.cursor = (tree.cursor + tree.entries.len() - 1) % tree.entries.len();
                }
            }
            LibraryAction::None
        }
        KeyCode::Char(' ') => {
            if let Some(Overlay::AddTree(tree)) = state.overlay.as_mut() {
                if let Some(TreeEntry::File { path, .. }) = tree.entries.get(tree.cursor) {
                    let p = path.clone();
                    if tree.selected.contains(&p) {
                        tree.selected.remove(&p);
                    } else {
                        tree.selected.insert(p);
                    }
                }
            }
            LibraryAction::None
        }
        KeyCode::Backspace => {
            if let Some(Overlay::AddTree(tree)) = state.overlay.as_mut() {
                navigate_to_parent(tree);
            }
            LibraryAction::None
        }
        KeyCode::Enter => {
            let entry = match state.overlay.as_ref() {
                Some(Overlay::AddTree(tree)) => tree.entries.get(tree.cursor).cloned(),
                _ => None,
            };
            match entry {
                Some(TreeEntry::Parent) => {
                    if let Some(Overlay::AddTree(tree)) = state.overlay.as_mut() {
                        navigate_to_parent(tree);
                    }
                    LibraryAction::None
                }
                Some(TreeEntry::Dir { path, .. }) => {
                    if let Some(Overlay::AddTree(tree)) = state.overlay.as_mut() {
                        navigate_to(tree, &path);
                    }
                    LibraryAction::None
                }
                Some(TreeEntry::File { path, .. }) => {
                    let paths = match state.overlay.as_ref() {
                        Some(Overlay::AddTree(tree)) if !tree.selected.is_empty() => {
                            tree.selected.iter().cloned().collect()
                        }
                        _ => vec![path],
                    };
                    perform_import(state, paths)
                }
                None => LibraryAction::None,
            }
        }
        _ => LibraryAction::None,
    }
}

fn navigate_to_parent(tree: &mut AddTreeState) {
    let Some(parent) = tree.cwd.parent().map(Path::to_path_buf) else {
        return;
    };
    navigate_to(tree, &parent);
}

fn navigate_to(tree: &mut AddTreeState, dir: &Path) {
    match read_tree(dir) {
        Ok(entries) => {
            tree.cwd = dir.to_path_buf();
            tree.entries = entries;
            tree.cursor = 0;
            tree.error = None;
        }
        Err(err) => {
            tree.error = Some(err);
        }
    }
}

fn perform_import(state: &mut State, paths: Vec<PathBuf>) -> LibraryAction {
    let Some(ctx) = state.catalog.clone() else {
        state.overlay = None;
        return LibraryAction::Status(StatusMessage::error("no catalog selected"));
    };
    let result = import_files(&ctx.dir, &paths);
    state.overlay = None;
    match result {
        Ok(outcome) => {
            state.refresh();
            let summary = add_result::State::from_outcome(&outcome);
            // Skips and failures deserve an explicit, dismissable list; a clean
            // all-imported run stays on the lightweight status line.
            if summary.is_noteworthy() {
                state.overlay = Some(Overlay::AddResult(summary));
                LibraryAction::None
            } else {
                let total = outcome.rows.len();
                LibraryAction::Status(StatusMessage::info(format!(
                    "imported {total} / {total} file(s)"
                )))
            }
        }
        Err(err) => LibraryAction::Status(StatusMessage::error(err)),
    }
}

fn handle_add_result_key(state: &mut State, key: KeyEvent) -> LibraryAction {
    let Some(Overlay::AddResult(result)) = state.overlay.as_mut() else {
        return LibraryAction::None;
    };
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            result.scroll_down();
            LibraryAction::None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            result.scroll_up();
            LibraryAction::None
        }
        KeyCode::Esc | KeyCode::Enter => {
            state.overlay = None;
            LibraryAction::None
        }
        _ => LibraryAction::None,
    }
}

fn list_rows(dir: &Path) -> std::result::Result<Vec<Book>, String> {
    let conn = catalog::open_existing(dir).map_err(|e| e.to_string())?;
    books::handle_ls(&conn).map_err(|e| e.to_string())
}

fn remove_book(dir: &Path, id: i64, keep: bool) -> std::result::Result<books::RmOutcome, String> {
    let mut conn = catalog::open_existing(dir).map_err(|e| e.to_string())?;
    books::handle_rm(&mut conn, dir, &id.to_string(), keep).map_err(|e| e.to_string())
}

fn import_files(dir: &Path, paths: &[PathBuf]) -> std::result::Result<books::AddOutcome, String> {
    let mut conn = catalog::open_existing(dir).map_err(|e| e.to_string())?;
    // TUI imports never force; duplicates are reported in the status line.
    Ok(books::handle_add(&mut conn, dir, paths, false))
}

fn read_tree(dir: &Path) -> std::result::Result<Vec<TreeEntry>, String> {
    let read =
        std::fs::read_dir(dir).map_err(|e| format!("cannot read `{}`: {e}", dir.display()))?;
    let mut dirs: Vec<(String, PathBuf)> = Vec::new();
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    for entry in read.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            dirs.push((name, path));
        } else if file_type.is_file() {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase);
            if ext
                .as_deref()
                .and_then(import::Format::parse_label)
                .is_some()
            {
                files.push((name, path));
            }
        }
    }
    dirs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    files.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

    let mut out: Vec<TreeEntry> = Vec::with_capacity(dirs.len() + files.len() + 1);
    if dir.parent().is_some() {
        out.push(TreeEntry::Parent);
    }
    for (name, path) in dirs {
        out.push(TreeEntry::Dir { path, name });
    }
    for (name, path) in files {
        out.push(TreeEntry::File { path, name });
    }
    Ok(out)
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    let title = match &state.catalog {
        Some(ctx) => format!("Library — {}", ctx.name),
        None => "Library".to_string(),
    };
    let header = Paragraph::new(Line::from(Span::styled(
        title,
        Style::default().add_modifier(Modifier::BOLD),
    )));
    frame.render_widget(header, layout[0]);

    if let Some(err) = &state.load_error {
        let p = Paragraph::new(Line::from(Span::styled(
            format!("error: {err}"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(p, layout[1]);
    } else if state.catalog.is_none() {
        let p = Paragraph::new(Line::from(Span::styled(
            "no catalog selected — open Catalogs (`:catalogs`) to create or switch",
            Style::default().fg(Color::DarkGray),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(p, layout[1]);
    } else if state.rows.is_empty() {
        let p = Paragraph::new(Line::from(Span::styled(
            "no books yet — press `a` to import one",
            Style::default().fg(Color::DarkGray),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(p, layout[1]);
    } else {
        render_table(frame, layout[1], state);
    }

    match &state.overlay {
        Some(Overlay::Inspect {
            book,
            absolute_path,
        }) => render_inspect_modal(frame, area, book.as_ref(), absolute_path),
        Some(Overlay::ConfirmRm { id, title }) => render_confirm_modal(frame, area, *id, title),
        Some(Overlay::AddTree(tree)) => render_tree_modal(frame, area, tree),
        Some(Overlay::AddResult(result)) => add_result::render(frame, area, result),
        Some(Overlay::Edit(edit_state)) => edit::render(frame, area, edit_state.as_ref()),
        Some(Overlay::EmbedJob(job)) => embed_job::render(frame, area, job),
        Some(Overlay::Columns(picker)) => columns::render(frame, area, picker),
        None => {}
    }
}

fn render_table(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let columns: Vec<LibraryColumn> = if state.columns.is_empty() {
        LibraryColumn::DEFAULT.to_vec()
    } else {
        state.columns.clone()
    };
    let header_cells: Vec<&str> = columns.iter().map(|c| c.header()).collect();
    let header = Row::new(header_cells).style(
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    let rows: Vec<Row<'_>> = state
        .rows
        .iter()
        .map(|b| Row::new(columns.iter().map(|c| c.render(b)).collect::<Vec<_>>()))
        .collect();
    let widths: Vec<Constraint> = columns.iter().map(|c| c.width()).collect();
    let table = Table::new(rows, widths).header(header).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    let mut s = TableState::default();
    s.select(Some(state.cursor.min(state.rows.len().saturating_sub(1))));
    frame.render_stateful_widget(table, area, &mut s);
}

fn render_inspect_modal(frame: &mut Frame<'_>, area: Rect, book: &Book, absolute_path: &Path) {
    // Build sections: each is a Vec<(key, value)>. Empty sections are skipped.
    let identity: Vec<(&str, String)> = vec![
        ("id", book.id.to_string()),
        ("title", book.title.clone()),
        (
            "author",
            book.author.clone().unwrap_or_else(|| "(unknown)".into()),
        ),
    ];

    let catalog: Vec<(&str, String)> = vec![
        ("format", book.format.clone()),
        ("file", absolute_path.display().to_string()),
        ("added", book.added_at.clone()),
    ];

    let mut metadata: Vec<(&str, String)> = Vec::new();
    if !book.tags.is_empty() {
        metadata.push(("tags", book.tags.join(", ")));
    }
    if let Some(series) = &book.series_name {
        let v = match book.series_index {
            Some(idx) => format!("{series} #{}", format_index(idx)),
            None => series.clone(),
        };
        metadata.push(("series", v));
    }
    let rating_value = book.rating.unwrap_or(0);
    let rating_display = if rating_value == 0 {
        format!("{} (unrated)", stars(0))
    } else {
        format!("{} ({rating_value}/5)", stars(rating_value))
    };
    metadata.push(("rating", rating_display));
    if let Some(p) = &book.publisher {
        metadata.push(("publisher", p.clone()));
    }
    if let Some(l) = &book.language {
        metadata.push(("language", l.clone()));
    }
    if let Some(d) = &book.published_date {
        metadata.push(("published", d.clone()));
    }
    if let Some(i) = &book.isbn {
        metadata.push(("isbn", i.clone()));
    }

    let description = book.description.clone();

    // Compute modal size from longest value line (with sane bounds).
    let mut max_w: usize = 0;
    for (_k, v) in identity.iter().chain(catalog.iter()).chain(metadata.iter()) {
        let line_w = INSPECT_LEFT_PAD + INSPECT_LABEL_W + v.chars().count() + 2;
        max_w = max_w.max(line_w);
    }
    if let Some(desc) = &description {
        max_w = max_w.max(INSPECT_LEFT_PAD + 2 + desc.chars().count().min(80) + 2);
    }
    let title_w = " inspect ".len() + 4;
    let target_w = max_w.max(title_w).clamp(40, area.width as usize) as u16;

    // Count rows: identity + spacer + catalog + spacer + metadata
    // (if any) + description(2-4) + borders(2).
    let mut rows: u16 = identity.len() as u16 + 1 + catalog.len() as u16;
    if !metadata.is_empty() {
        rows += 1 + metadata.len() as u16;
    }
    let desc_lines: u16 = description
        .as_ref()
        .map(|d| {
            let inner_w = target_w
                .saturating_sub((INSPECT_LEFT_PAD as u16) + 4)
                .max(1) as usize;
            let count = (d.chars().count().max(1) + inner_w - 1) / inner_w;
            (count as u16).clamp(2, 6) + 1
        })
        .unwrap_or(0);
    rows += desc_lines;
    rows += 2; // borders
    let target_h = rows.clamp(10, area.height);

    let rect = centered_rect(target_w, target_h, area);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" inspect ")
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    // Build the layout dynamically based on present sections.
    let mut constraints: Vec<Constraint> = Vec::new();
    let mut row_kind: Vec<RowKind> = Vec::new();

    let add_section =
        |sec: &[(&str, String)], constraints: &mut Vec<Constraint>, row_kind: &mut Vec<RowKind>| {
            for (k, v) in sec {
                constraints.push(Constraint::Length(1));
                row_kind.push(RowKind::Kv {
                    key: (*k).to_string(),
                    value: v.clone(),
                });
            }
        };
    let add_spacer = |constraints: &mut Vec<Constraint>, row_kind: &mut Vec<RowKind>| {
        constraints.push(Constraint::Length(1));
        row_kind.push(RowKind::Blank);
    };

    add_section(&identity, &mut constraints, &mut row_kind);
    add_spacer(&mut constraints, &mut row_kind);
    add_section(&catalog, &mut constraints, &mut row_kind);
    if !metadata.is_empty() {
        add_spacer(&mut constraints, &mut row_kind);
        add_section(&metadata, &mut constraints, &mut row_kind);
    }
    if let Some(desc) = &description {
        add_spacer(&mut constraints, &mut row_kind);
        constraints.push(Constraint::Length(1));
        row_kind.push(RowKind::Kv {
            key: "description".to_string(),
            value: String::new(),
        });
        constraints.push(Constraint::Length(desc_lines.saturating_sub(1)));
        row_kind.push(RowKind::DescBody(desc.clone()));
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (i, kind) in row_kind.iter().enumerate() {
        match kind {
            RowKind::Blank => {}
            RowKind::Kv { key, value } => render_inspect_row(frame, layout[i], key, value),
            RowKind::DescBody(text) => render_inspect_desc(frame, layout[i], text),
        }
    }
}

const INSPECT_LEFT_PAD: usize = 2;
const INSPECT_LABEL_W: usize = 12;

enum RowKind {
    Blank,
    Kv { key: String, value: String },
    DescBody(String),
}

fn render_inspect_row(frame: &mut Frame<'_>, area: Rect, key: &str, value: &str) {
    let line = Line::from(vec![
        Span::raw(" ".repeat(INSPECT_LEFT_PAD)),
        Span::styled(
            format!("{key:<width$}", width = INSPECT_LABEL_W),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(value.to_string(), Style::default().fg(Color::White)),
    ]);
    let p = Paragraph::new(line);
    frame.render_widget(p, area);
}

fn render_inspect_desc(frame: &mut Frame<'_>, area: Rect, text: &str) {
    let body_area = Rect {
        x: area.x.saturating_add((INSPECT_LEFT_PAD + 2) as u16),
        y: area.y,
        width: area.width.saturating_sub((INSPECT_LEFT_PAD + 4) as u16),
        height: area.height,
    };
    let p = Paragraph::new(Span::styled(
        text.to_string(),
        Style::default().fg(Color::Gray),
    ))
    .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(p, body_area);
}

fn stars(value: u8) -> String {
    let v = value.min(5) as usize;
    let mut s = String::new();
    for i in 0..5 {
        s.push(if i < v { '★' } else { '☆' });
    }
    s
}

fn format_index(idx: f64) -> String {
    if idx.fract() == 0.0 {
        format!("{}", idx as i64)
    } else {
        format!("{idx}")
    }
}

fn render_confirm_modal(frame: &mut Frame<'_>, area: Rect, id: i64, title: &str) {
    let lines = vec![
        Line::from(Span::raw(format!("Delete book `{title}` (id {id})?"))),
        Line::from(""),
        Line::from(vec![
            Span::styled("y", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("/Enter — delete   "),
            Span::styled("k", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" — keep file in cwd   "),
            Span::styled("n", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("/Esc — cancel"),
        ]),
    ];
    render_modal(frame, area, "confirm", lines);
}

fn render_tree_modal(frame: &mut Frame<'_>, area: Rect, tree: &AddTreeState) {
    let target_w = area.width.saturating_mul(4) / 5;
    let target_h = area.height.saturating_mul(4) / 5;
    let w = target_w.max(40).min(area.width);
    let h = target_h.max(10).min(area.height);
    let rect = centered_rect(w, h, area);

    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" add ({} selected) ", tree.selected.len()))
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let header = Paragraph::new(Line::from(Span::styled(
        tree.cwd.display().to_string(),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    frame.render_widget(header, layout[0]);

    let items: Vec<ListItem> = tree
        .entries
        .iter()
        .map(|entry| {
            let line = match entry {
                TreeEntry::Parent => Line::from(Span::styled(
                    "..",
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::BOLD),
                )),
                TreeEntry::Dir { name, .. } => Line::from(Span::styled(
                    format!("{name}/"),
                    Style::default().fg(Color::Blue),
                )),
                TreeEntry::File { name, path } => {
                    let marker = if tree.selected.contains(path) {
                        "[*] "
                    } else {
                        "[ ] "
                    };
                    Line::from(Span::raw(format!("{marker}{name}")))
                }
            };
            ListItem::new(line)
        })
        .collect();
    let list = List::new(items).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    let mut list_state = ListState::default();
    if !tree.entries.is_empty() {
        list_state.select(Some(tree.cursor.min(tree.entries.len() - 1)));
    }
    frame.render_stateful_widget(list, layout[1], &mut list_state);

    if let Some(err) = &tree.error {
        let p = Paragraph::new(Line::from(Span::styled(
            err.clone(),
            Style::default().fg(Color::Red),
        )));
        frame.render_widget(p, layout[2]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::handlers;
    use crossterm::event::KeyModifiers;
    use rusqlite::params;
    use std::fs;
    use tempfile::tempdir;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn setup_with_catalog() -> (tempfile::TempDir, PathBuf, Registry) {
        let tmp = tempdir().unwrap();
        let cfg = tmp.path().join("cfg");
        fs::create_dir_all(&cfg).unwrap();
        let cat = tmp.path().join("lib");
        let mut reg = Registry::default();
        handlers::handle_init(&mut reg, &cfg, "main", &cat, None, false).unwrap();
        // canonical path stored in registry
        let canonical = cat.canonicalize().unwrap();
        (tmp, canonical, reg)
    }

    fn insert_book(dir: &Path, title: &str, author: Option<&str>) -> i64 {
        let conn = catalog::open_existing(dir).unwrap();
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES (?1, ?2, 'epub', '')",
            params![title, author],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn insert_book_with_file(dir: &Path, title: &str, author: &str) -> i64 {
        let conn = catalog::open_existing(dir).unwrap();
        let filename = format!(
            "{}_-_{}.epub",
            author.replace(' ', "_"),
            title.replace(' ', "_")
        );
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES (?1, ?2, 'epub', '')",
            params![title, author],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        let rel = format!("books/{id}/{filename}");
        let abs = dir.join(&rel);
        fs::create_dir_all(abs.parent().unwrap()).unwrap();
        fs::write(&abs, b"epub-stub").unwrap();
        conn.execute(
            "UPDATE books SET file_path = ?1 WHERE id = ?2",
            params![rel, id],
        )
        .unwrap();
        id
    }

    #[test]
    fn load_with_no_catalog_returns_empty_state() {
        let reg = Registry::default();
        let state = State::load(&reg);
        assert!(state.catalog.is_none());
        assert!(state.load_error.is_none());
        assert!(state.rows.is_empty());
    }

    #[test]
    fn load_lists_books_sorted_by_title() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book(&cat, "Charlie", None);
        insert_book(&cat, "alpha", None);
        insert_book(&cat, "Bravo", None);
        let state = State::load(&reg);
        let titles: Vec<_> = state.rows.iter().map(|b| b.title.as_str()).collect();
        assert_eq!(titles, vec!["alpha", "Bravo", "Charlie"]);
    }

    #[test]
    fn down_cycles_cursor() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book(&cat, "A", None);
        insert_book(&cat, "B", None);
        let mut state = State::load(&reg);
        assert_eq!(state.cursor, 0);
        handle_key(&mut state, key(KeyCode::Down));
        assert_eq!(state.cursor, 1);
        handle_key(&mut state, key(KeyCode::Down));
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn i_opens_inspect_overlay_then_esc_closes() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book(&cat, "Book", Some("Author"));
        let mut state = State::load(&reg);
        handle_key(&mut state, key(KeyCode::Char('i')));
        assert!(matches!(state.overlay, Some(Overlay::Inspect { .. })));
        handle_key(&mut state, key(KeyCode::Esc));
        assert!(state.overlay.is_none());
    }

    #[test]
    fn d_opens_confirm_and_y_removes() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book(&cat, "Doomed", None);
        let mut state = State::load(&reg);
        handle_key(&mut state, key(KeyCode::Char('d')));
        assert!(matches!(state.overlay, Some(Overlay::ConfirmRm { .. })));
        let action = handle_key(&mut state, key(KeyCode::Char('y')));
        assert!(matches!(action, LibraryAction::Status(_)));
        assert!(state.overlay.is_none());
        assert!(state.rows.is_empty());
    }

    #[test]
    fn confirm_n_cancels_keeping_row() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book(&cat, "Keep", None);
        let mut state = State::load(&reg);
        handle_key(&mut state, key(KeyCode::Char('d')));
        handle_key(&mut state, key(KeyCode::Char('n')));
        assert!(state.overlay.is_none());
        assert_eq!(state.rows.len(), 1);
    }

    #[test]
    fn esc_returns_back_action() {
        let reg = Registry::default();
        let mut state = State::load(&reg);
        let action = handle_key(&mut state, key(KeyCode::Esc));
        assert!(matches!(action, LibraryAction::Back));
    }

    #[test]
    fn colon_returns_open_palette() {
        let reg = Registry::default();
        let mut state = State::load(&reg);
        let action = handle_key(&mut state, key(KeyCode::Char(':')));
        assert!(matches!(action, LibraryAction::OpenPalette));
    }

    #[test]
    fn read_tree_includes_supported_files_and_dirs_only() {
        let tmp = tempdir().unwrap();
        fs::write(tmp.path().join("book.epub"), b"x").unwrap();
        fs::write(tmp.path().join("book.PDF"), b"x").unwrap();
        fs::write(tmp.path().join("note.txt"), b"x").unwrap();
        fs::create_dir(tmp.path().join("subdir")).unwrap();
        let entries = read_tree(tmp.path()).unwrap();
        let names: Vec<String> = entries.iter().map(label).collect();
        assert!(names.iter().any(|n| n == "book.epub"));
        assert!(names.iter().any(|n| n == "book.PDF"));
        assert!(!names.iter().any(|n| n == "note.txt"));
        assert!(names.iter().any(|n| n == "subdir"));
        assert!(matches!(entries.first(), Some(TreeEntry::Parent)));
    }

    fn label(entry: &TreeEntry) -> String {
        match entry {
            TreeEntry::Parent => "..".to_string(),
            TreeEntry::Dir { name, .. } => name.clone(),
            TreeEntry::File { name, .. } => name.clone(),
        }
    }

    #[test]
    fn add_tree_space_toggles_selection() {
        let (tmp, cat, reg) = setup_with_catalog();
        let workdir = tmp.path().join("incoming");
        fs::create_dir_all(&workdir).unwrap();
        fs::write(workdir.join("a.epub"), b"x").unwrap();
        let _ = cat; // unused
        let mut state = State::load(&reg);
        state.cwd = workdir.clone();

        handle_key(&mut state, key(KeyCode::Char('a')));
        // place cursor on file
        if let Some(Overlay::AddTree(tree)) = state.overlay.as_mut() {
            tree.cursor = tree
                .entries
                .iter()
                .position(|e| matches!(e, TreeEntry::File { .. }))
                .unwrap();
        }
        handle_key(&mut state, key(KeyCode::Char(' ')));
        if let Some(Overlay::AddTree(tree)) = state.overlay.as_ref() {
            assert_eq!(tree.selected.len(), 1);
        } else {
            panic!("expected AddTree overlay");
        }
        handle_key(&mut state, key(KeyCode::Char(' ')));
        if let Some(Overlay::AddTree(tree)) = state.overlay.as_ref() {
            assert_eq!(tree.selected.len(), 0);
        } else {
            panic!("expected AddTree overlay");
        }
    }

    #[test]
    fn add_tree_enter_on_dir_navigates_and_backspace_returns() {
        let (tmp, _cat, reg) = setup_with_catalog();
        let workdir = tmp.path().join("incoming");
        let sub = workdir.join("inner");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("a.epub"), b"x").unwrap();
        let mut state = State::load(&reg);
        state.cwd = workdir.clone();

        handle_key(&mut state, key(KeyCode::Char('a')));
        // cursor on "inner" dir entry
        if let Some(Overlay::AddTree(tree)) = state.overlay.as_mut() {
            tree.cursor = tree
                .entries
                .iter()
                .position(|e| matches!(e, TreeEntry::Dir { .. }))
                .unwrap();
        }
        handle_key(&mut state, key(KeyCode::Enter));
        if let Some(Overlay::AddTree(tree)) = state.overlay.as_ref() {
            assert_eq!(tree.cwd, sub);
        } else {
            panic!();
        }
        handle_key(&mut state, key(KeyCode::Backspace));
        if let Some(Overlay::AddTree(tree)) = state.overlay.as_ref() {
            assert_eq!(tree.cwd, workdir);
        } else {
            panic!();
        }
    }

    #[test]
    fn add_tree_enter_on_file_imports_into_catalog() {
        let (tmp, _cat, reg) = setup_with_catalog();
        let workdir = tmp.path().join("incoming");
        fs::create_dir_all(&workdir).unwrap();
        fs::write(workdir.join("solo.epub"), b"x").unwrap();
        let mut state = State::load(&reg);
        state.cwd = workdir.clone();

        handle_key(&mut state, key(KeyCode::Char('a')));
        if let Some(Overlay::AddTree(tree)) = state.overlay.as_mut() {
            tree.cursor = tree
                .entries
                .iter()
                .position(|e| matches!(e, TreeEntry::File { .. }))
                .unwrap();
        }
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, LibraryAction::Status(_)));
        assert!(state.overlay.is_none());
        assert_eq!(state.rows.len(), 1);
    }

    fn import_cursor_file(state: &mut State) -> LibraryAction {
        handle_key(state, key(KeyCode::Char('a')));
        if let Some(Overlay::AddTree(tree)) = state.overlay.as_mut() {
            tree.cursor = tree
                .entries
                .iter()
                .position(|e| matches!(e, TreeEntry::File { .. }))
                .unwrap();
        }
        handle_key(state, key(KeyCode::Enter))
    }

    #[test]
    fn add_tree_duplicate_opens_result_modal_and_esc_closes() {
        let (tmp, _cat, reg) = setup_with_catalog();
        let workdir = tmp.path().join("incoming");
        fs::create_dir_all(&workdir).unwrap();
        fs::write(workdir.join("solo.epub"), b"x").unwrap();
        let mut state = State::load(&reg);
        state.cwd = workdir.clone();

        // First import succeeds cleanly → lightweight status, no modal.
        let first = import_cursor_file(&mut state);
        assert!(matches!(first, LibraryAction::Status(_)));
        assert!(state.overlay.is_none());
        assert_eq!(state.rows.len(), 1);

        // Re-importing the same file is a duplicate → explicit result modal.
        let second = import_cursor_file(&mut state);
        assert!(matches!(second, LibraryAction::None));
        assert!(matches!(state.overlay, Some(Overlay::AddResult(_))));
        assert_eq!(state.rows.len(), 1, "duplicate must not add a book");

        // Esc dismisses the summary.
        handle_key(&mut state, key(KeyCode::Esc));
        assert!(state.overlay.is_none());
    }

    #[test]
    fn add_tree_enter_on_file_imports_batch_when_selected() {
        let (tmp, _cat, reg) = setup_with_catalog();
        let workdir = tmp.path().join("incoming");
        fs::create_dir_all(&workdir).unwrap();
        fs::write(workdir.join("a.epub"), b"x").unwrap();
        fs::write(workdir.join("b.epub"), b"y").unwrap();
        let mut state = State::load(&reg);
        state.cwd = workdir.clone();

        handle_key(&mut state, key(KeyCode::Char('a')));
        // mark both files
        if let Some(Overlay::AddTree(tree)) = state.overlay.as_mut() {
            for (i, e) in tree.entries.iter().enumerate() {
                if let TreeEntry::File { path, .. } = e {
                    tree.selected.insert(path.clone());
                    tree.cursor = i;
                }
            }
        }
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, LibraryAction::Status(_)));
        assert_eq!(state.rows.len(), 2);
    }

    #[test]
    fn e_on_table_opens_edit_overlay() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book_with_file(&cat, "Old", "Author");
        let mut state = State::load(&reg);
        handle_key(&mut state, key(KeyCode::Char('e')));
        match state.overlay.as_ref() {
            Some(Overlay::Edit(s)) => assert_eq!(s.origin, edit::Origin::Table),
            _ => panic!("expected Edit overlay"),
        }
    }

    #[test]
    fn e_inside_inspect_opens_edit_with_inspect_origin() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book_with_file(&cat, "Book", "Author");
        let mut state = State::load(&reg);
        handle_key(&mut state, key(KeyCode::Char('i')));
        assert!(matches!(state.overlay, Some(Overlay::Inspect { .. })));
        handle_key(&mut state, key(KeyCode::Char('e')));
        match state.overlay.as_ref() {
            Some(Overlay::Edit(s)) => assert_eq!(s.origin, edit::Origin::Inspect),
            _ => panic!("expected Edit overlay after `e` in Inspect"),
        }
    }

    #[test]
    fn edit_cancel_from_table_origin_closes_overlay() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book_with_file(&cat, "T", "A");
        let mut state = State::load(&reg);
        handle_key(&mut state, key(KeyCode::Char('e')));
        handle_key(&mut state, key(KeyCode::Esc));
        assert!(state.overlay.is_none());
    }

    #[test]
    fn edit_cancel_from_inspect_origin_returns_to_inspect() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book_with_file(&cat, "T", "A");
        let mut state = State::load(&reg);
        handle_key(&mut state, key(KeyCode::Char('i')));
        handle_key(&mut state, key(KeyCode::Char('e')));
        handle_key(&mut state, key(KeyCode::Esc));
        assert!(matches!(state.overlay, Some(Overlay::Inspect { .. })));
    }

    #[test]
    fn edit_save_with_empty_title_keeps_overlay_open_with_error() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book_with_file(&cat, "T", "A");
        let mut state = State::load(&reg);
        handle_key(&mut state, key(KeyCode::Char('e')));
        // clear title via Backspace (Title length 1)
        handle_key(&mut state, key(KeyCode::Backspace));
        // Tab through to Save (current = Title; 11 tabs land us on Save)
        for _ in 0..11 {
            handle_key(&mut state, key(KeyCode::Tab));
        }
        let edit_focus = match state.overlay.as_ref() {
            Some(Overlay::Edit(s)) => s.focus,
            _ => panic!("expected Edit overlay"),
        };
        assert_eq!(edit_focus, edit::Focus::Save);
        handle_key(&mut state, key(KeyCode::Enter));
        match state.overlay.as_ref() {
            Some(Overlay::Edit(s)) => {
                assert!(s.error.is_some(), "expected validation error to be set");
                assert_eq!(s.focus, edit::Focus::Title);
            }
            _ => panic!("Edit overlay must stay open on validation failure"),
        }
    }

    #[test]
    fn edit_save_happy_path_updates_row_and_reopens_inspect() {
        let (_tmp, cat, reg) = setup_with_catalog();
        let id = insert_book_with_file(&cat, "Initial", "Author");
        let mut state = State::load(&reg);
        handle_key(&mut state, key(KeyCode::Char('i')));
        handle_key(&mut state, key(KeyCode::Char('e')));
        // Append " v2" to title.
        for ch in " v2".chars() {
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            );
        }
        // Jump to Save.
        for _ in 0..11 {
            handle_key(&mut state, key(KeyCode::Tab));
        }
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, LibraryAction::Status(_)));
        match state.overlay.as_ref() {
            Some(Overlay::Inspect { book, .. }) => {
                assert_eq!(book.id, id);
                assert_eq!(book.title, "Initial v2");
            }
            other => panic!("expected Inspect overlay after save, got {other:?}"),
        }
        assert!(state.rows.iter().any(|b| b.title == "Initial v2"));
    }

    #[test]
    fn c_opens_columns_picker() {
        let (_tmp, cat, reg) = setup_with_catalog();
        let _ = cat;
        let mut state = State::load(&reg);
        handle_key(&mut state, key(KeyCode::Char('c')));
        assert!(matches!(state.overlay, Some(Overlay::Columns(_))));
    }

    #[test]
    fn columns_picker_saves_selection_to_settings() {
        let (_tmp, cat, reg) = setup_with_catalog();
        let mut state = State::load(&reg);
        // Defaults applied at load.
        assert_eq!(state.columns, LibraryColumn::DEFAULT.to_vec());

        handle_key(&mut state, key(KeyCode::Char('c')));
        // Toggle "rating" on (index 5 in ALL).
        for _ in 0..5 {
            handle_key(&mut state, key(KeyCode::Down));
        }
        handle_key(&mut state, key(KeyCode::Char(' ')));
        handle_key(&mut state, key(KeyCode::Enter));

        assert!(state.overlay.is_none());
        assert!(state.columns.contains(&LibraryColumn::Rating));

        // Persisted in settings table.
        let conn = catalog::open_existing(&cat).unwrap();
        let stored = settings::load_library_columns(&conn).unwrap();
        assert!(stored.contains(&LibraryColumn::Rating));
        assert_eq!(stored, state.columns);

        // A fresh State::load picks up the saved selection.
        let reloaded = State::load(&reg);
        assert_eq!(reloaded.columns, state.columns);
    }

    #[test]
    fn columns_picker_esc_keeps_existing_selection() {
        let (_tmp, _cat, reg) = setup_with_catalog();
        let mut state = State::load(&reg);
        let before = state.columns.clone();
        handle_key(&mut state, key(KeyCode::Char('c')));
        // Try to toggle off all defaults but then cancel.
        for _ in 0..LibraryColumn::DEFAULT.len() {
            handle_key(&mut state, key(KeyCode::Char(' ')));
            handle_key(&mut state, key(KeyCode::Down));
        }
        handle_key(&mut state, key(KeyCode::Esc));
        assert!(state.overlay.is_none());
        assert_eq!(state.columns, before);
    }

    #[test]
    fn ctrl_w_opens_embed_job_overlay() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book_with_file(&cat, "Book", "Author");
        let mut state = State::load(&reg);
        let action = handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
        );
        assert!(matches!(action, LibraryAction::None));
        assert!(matches!(state.overlay, Some(Overlay::EmbedJob(_))));
        assert!(has_pending_embed_job(&state));
    }

    #[test]
    fn embed_job_marks_mobi_as_unsupported_upfront() {
        let (_tmp, cat, reg) = setup_with_catalog();
        let conn = catalog::open_existing(&cat).unwrap();
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES ('M', 'A', 'mobi', 'books/1/A_-_M.mobi')",
            [],
        )
        .unwrap();
        drop(conn);
        let mut state = State::load(&reg);
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
        );
        match state.overlay.as_ref() {
            Some(Overlay::EmbedJob(job)) => {
                assert_eq!(job.failures.len(), 1);
                assert!(job.failures[0].reason.contains("not supported"));
                // No queue to advance because the only book is unsupported.
                assert!(job.done);
            }
            _ => panic!("expected EmbedJob overlay"),
        }
    }

    #[test]
    fn ctrl_w_skips_books_already_synced() {
        let (_tmp, cat, reg) = setup_with_catalog();
        let synced_id = insert_book_with_file(&cat, "Already Synced", "A");
        insert_book_with_file(&cat, "Still Pending", "A");
        let conn = catalog::open_existing(&cat).unwrap();
        books::mark_embed_synced(&conn, synced_id).unwrap();
        drop(conn);

        let mut state = State::load(&reg);
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
        );
        match state.overlay.as_ref() {
            Some(Overlay::EmbedJob(job)) => {
                assert_eq!(job.total, 1, "synced book must be excluded from the queue");
                assert_eq!(job.queue.len(), 1);
                assert_eq!(
                    job.queue[0].title, "Still Pending",
                    "only the pending book should be queued"
                );
            }
            _ => panic!("expected EmbedJob overlay"),
        }
    }

    #[test]
    fn ctrl_w_with_only_synced_books_reports_nothing_to_embed() {
        let (_tmp, cat, reg) = setup_with_catalog();
        let id = insert_book_with_file(&cat, "Only One", "A");
        let conn = catalog::open_existing(&cat).unwrap();
        books::mark_embed_synced(&conn, id).unwrap();
        drop(conn);

        let mut state = State::load(&reg);
        let action = handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
        );
        match action {
            LibraryAction::Status(s) => {
                assert!(
                    s.text.contains("nothing to embed"),
                    "got status: {}",
                    s.text
                );
            }
            other => panic!("expected status message, got {other:?}"),
        }
        assert!(state.overlay.is_none(), "overlay must not open");
    }

    #[test]
    fn w_on_inspect_synced_book_is_no_op() {
        let (_tmp, cat, reg) = setup_with_catalog();
        let id = insert_book_with_file(&cat, "Sync Me", "Author");
        let conn = catalog::open_existing(&cat).unwrap();
        books::mark_embed_synced(&conn, id).unwrap();
        let synced_at_before: Option<String> = conn
            .query_row("SELECT embed_synced_at FROM books WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .unwrap();
        drop(conn);

        let mut state = State::load(&reg);
        handle_key(&mut state, key(KeyCode::Char('i')));
        let action = handle_key(&mut state, key(KeyCode::Char('w')));
        match action {
            LibraryAction::Status(s) => {
                assert!(s.text.contains("already synced"), "got status: {}", s.text);
            }
            other => panic!("expected status message, got {other:?}"),
        }
        // Status row unchanged: still synced, same timestamp.
        let conn = catalog::open_existing(&cat).unwrap();
        let (status, synced_at): (String, Option<String>) = conn
            .query_row(
                "SELECT embed_status, embed_synced_at FROM books WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "synced");
        assert_eq!(synced_at, synced_at_before);
    }

    #[test]
    fn w_on_inspect_unsupported_book_does_not_retry() {
        let (_tmp, cat, reg) = setup_with_catalog();
        // Insert a mobi (unsupported format) and mark it unsupported explicitly.
        let conn = catalog::open_existing(&cat).unwrap();
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES ('M', 'A', 'mobi', 'books/1/A_-_M.mobi')",
            [],
        )
        .unwrap();
        let id: i64 = conn
            .query_row("SELECT id FROM books WHERE title='M'", [], |r| r.get(0))
            .unwrap();
        books::mark_embed_unsupported(&conn, id).unwrap();
        drop(conn);

        let mut state = State::load(&reg);
        handle_key(&mut state, key(KeyCode::Char('i')));
        let action = handle_key(&mut state, key(KeyCode::Char('w')));
        match action {
            LibraryAction::Status(s) => {
                assert!(s.text.contains("not supported"), "got status: {}", s.text);
            }
            other => panic!("expected status message, got {other:?}"),
        }
    }

    #[test]
    fn embed_job_esc_cancels_pending_work() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book_with_file(&cat, "T1", "A");
        insert_book_with_file(&cat, "T2", "A");
        let mut state = State::load(&reg);
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
        );
        // Before any advance, queue has 2 EPUBs and job is pending.
        assert!(has_pending_embed_job(&state));
        // Cancel.
        handle_key(&mut state, key(KeyCode::Esc));
        match state.overlay.as_ref() {
            Some(Overlay::EmbedJob(job)) => {
                assert!(job.done);
                assert!(job.queue.is_empty());
            }
            _ => panic!("expected EmbedJob overlay still showing summary"),
        }
        // Esc again closes.
        handle_key(&mut state, key(KeyCode::Esc));
        assert!(state.overlay.is_none());
    }

    #[test]
    fn advance_embed_job_progresses_one_book_at_a_time() {
        let (_tmp, cat, reg) = setup_with_catalog();
        // The dummy bytes are not real EPUB → embed fails. That's fine; we
        // just verify the queue advances and failures get recorded.
        insert_book_with_file(&cat, "T1", "A");
        insert_book_with_file(&cat, "T2", "A");
        let mut state = State::load(&reg);
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
        );
        // Start: 0 completed, 2 in queue.
        match state.overlay.as_ref() {
            Some(Overlay::EmbedJob(job)) => {
                assert_eq!(job.completed, 0);
                assert_eq!(job.total, 2);
                assert_eq!(job.queue.len(), 2);
                assert!(!job.done);
            }
            _ => panic!(),
        }
        advance_embed_job(&mut state);
        match state.overlay.as_ref() {
            Some(Overlay::EmbedJob(job)) => {
                assert_eq!(job.completed, 1);
                assert_eq!(job.queue.len(), 1);
            }
            _ => panic!(),
        }
        advance_embed_job(&mut state);
        match state.overlay.as_ref() {
            Some(Overlay::EmbedJob(job)) => {
                assert_eq!(job.completed, 2);
                assert!(job.done);
            }
            _ => panic!(),
        }
        assert!(!has_pending_embed_job(&state));
    }

    #[test]
    fn help_sections_match_overlay_state() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book_with_file(&cat, "Book", "Author");
        let mut state = State::load(&reg);

        assert_eq!(help_sections(&state)[0].title, "Library");

        handle_key(&mut state, key(KeyCode::Char('i')));
        assert_eq!(help_sections(&state)[0].title, "Inspect");

        handle_key(&mut state, key(KeyCode::Esc));
        handle_key(&mut state, key(KeyCode::Char('d')));
        assert_eq!(help_sections(&state)[0].title, "Confirm remove");

        handle_key(&mut state, key(KeyCode::Char('n')));
        handle_key(&mut state, key(KeyCode::Char('c')));
        assert_eq!(help_sections(&state)[0].title, "Columns");

        handle_key(&mut state, key(KeyCode::Esc));
        handle_key(&mut state, key(KeyCode::Char('e')));
        let sections = help_sections(&state);
        assert_eq!(sections[0].title, "Edit metadata");
        assert_eq!(sections.len(), 1, "Rating field section only when focused");

        // Land focus on Rating: Title→Author→Tags→Series→Index→Rating = 5 tabs.
        for _ in 0..5 {
            handle_key(&mut state, key(KeyCode::Tab));
        }
        let sections = help_sections(&state);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[1].title, "Rating field");
    }

    #[test]
    fn w_on_inspect_mobi_returns_unsupported_status() {
        let (_tmp, cat, reg) = setup_with_catalog();
        // Insert directly as mobi.
        let conn = catalog::open_existing(&cat).unwrap();
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES ('M', 'A', 'mobi', 'books/1/A_-_M.mobi')",
            [],
        )
        .unwrap();
        drop(conn);
        let mut state = State::load(&reg);
        handle_key(&mut state, key(KeyCode::Char('i')));
        let action = handle_key(&mut state, key(KeyCode::Char('w')));
        match action {
            LibraryAction::Status(s) => {
                assert!(s.text.contains("not supported"), "got status: {}", s.text)
            }
            other => panic!("expected error status, got {other:?}"),
        }
    }
}
