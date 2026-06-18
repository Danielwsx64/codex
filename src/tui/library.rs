use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState,
};
use ratatui::Frame;
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

use crate::catalog::books::{self, Book, EmbedStatus};
use crate::catalog::columns::LibraryColumn;
use crate::catalog::settings;
use crate::catalog::{self, devices};
use crate::config::Registry;
use crate::device;
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
        keys: "Enter",
        desc: "actions menu for selected book",
    },
    Binding {
        keys: "i",
        desc: "inspect book",
    },
    Binding {
        keys: "o",
        desc: "open book in reader",
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
    Binding {
        keys: "p",
        desc: "push selected book to device",
    },
    Binding {
        keys: "s",
        desc: "sync with current device",
    },
    Binding {
        keys: "D",
        desc: "find duplicate books",
    },
    Binding {
        keys: "/",
        desc: "quick filter (Esc clears when filtered)",
    },
    Binding {
        keys: ":search",
        desc: "advanced filter wizard",
    },
];

const FILTER_BINDINGS: &[Binding] = &[
    Binding {
        keys: "Enter",
        desc: "apply filter (empty clears)",
    },
    Binding {
        keys: "Esc",
        desc: "cancel editing the query",
    },
];

const SEARCH_BINDINGS: &[Binding] = &[
    Binding {
        keys: "Tab / ↓",
        desc: "next field",
    },
    Binding {
        keys: "Shift+Tab / ↑",
        desc: "previous field",
    },
    Binding {
        keys: "Enter",
        desc: "next field (apply on Apply button)",
    },
    Binding {
        keys: "Ctrl+S",
        desc: "apply filter",
    },
    Binding {
        keys: "Esc",
        desc: "cancel",
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

const CONTEXT_MENU_BINDINGS: &[Binding] = &[
    Binding {
        keys: "↑↓ / j k",
        desc: "move selection",
    },
    Binding {
        keys: "Enter",
        desc: "run action",
    },
    Binding {
        keys: "Esc",
        desc: "close menu",
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

const CONFIRM_PUSH_BINDINGS: &[Binding] = &[
    Binding {
        keys: "y / Enter",
        desc: "push book to device",
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
        keys: "Enter",
        desc: "next field (Save/Cancel on those buttons)",
    },
    Binding {
        keys: "Ctrl+S",
        desc: "save changes",
    },
    Binding {
        keys: "Esc",
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
        Some(Overlay::ContextMenu { .. }) => vec![Section {
            title: "Actions",
            bindings: CONTEXT_MENU_BINDINGS,
        }],
        Some(Overlay::ConfirmRm { .. }) => vec![Section {
            title: "Confirm remove",
            bindings: CONFIRM_RM_BINDINGS,
        }],
        Some(Overlay::ConfirmPush { .. }) => vec![Section {
            title: "Confirm push",
            bindings: CONFIRM_PUSH_BINDINGS,
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
        Some(Overlay::Filter(_)) => vec![Section {
            title: "Filter",
            bindings: FILTER_BINDINGS,
        }],
        Some(Overlay::Search(_)) => vec![Section {
            title: "Search",
            bindings: SEARCH_BINDINGS,
        }],
    }
}

pub mod add_result;
pub mod columns;
pub mod edit;
pub mod embed_job;
pub mod search;

#[derive(Debug)]
pub struct State {
    pub catalog: Option<CatalogContext>,
    pub rows: Vec<Book>,
    pub cursor: usize,
    pub overlay: Option<Overlay>,
    pub load_error: Option<String>,
    pub cwd: PathBuf,
    pub columns: Vec<LibraryColumn>,
    pub filter: Option<ActiveFilter>,
    // Presence of each book against the current device, keyed by book id. Empty
    // unless a device is current+connected; drives the leading indicator column.
    pub device_presence: HashMap<i64, device::presence::LibraryPresence>,
    // Alias (or serial) of the current+connected device, for the header badge.
    // None when no single device is current+connected. Set during refresh()
    // alongside device_presence; render reads it without touching the DB.
    pub current_device_label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CatalogContext {
    pub name: String,
    pub dir: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct FilterCriteria {
    pub query: Option<String>,
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub series: Option<String>,
    pub rating: Option<books::RatingRange>,
}

impl FilterCriteria {
    fn as_filters(&self) -> books::SearchFilters<'_> {
        books::SearchFilters {
            query: self.query.as_deref(),
            author: self.author.as_deref(),
            tags: &self.tags,
            series: self.series.as_deref(),
            rating: self.rating,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.query.is_none()
            && self.author.is_none()
            && self.tags.is_empty()
            && self.series.is_none()
            && self.rating.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterKind {
    Quick,
    Advanced,
}

#[derive(Debug, Clone)]
pub struct ActiveFilter {
    pub criteria: FilterCriteria,
    pub kind: FilterKind,
}

#[derive(Debug)]
pub enum Overlay {
    ContextMenu {
        cursor: usize,
    },
    Inspect {
        book: Box<Book>,
        absolute_path: PathBuf,
    },
    ConfirmRm {
        id: i64,
        title: String,
    },
    ConfirmPush {
        id: i64,
        title: String,
    },
    AddTree(AddTreeState),
    AddResult(add_result::State),
    Edit(Box<edit::State>),
    EmbedJob(Box<Job>),
    Columns(columns::State),
    Filter(Input),
    Search(Box<search::State>),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    Open,
    Inspect,
    Edit,
    SyncEmbed,
    Push,
    Delete,
}

// Order defines the menu layout; Open first so plain Enter-Enter reads a book.
const MENU_ITEMS: &[(MenuItem, &str)] = &[
    (MenuItem::Open, "Open in reader"),
    (MenuItem::Inspect, "Inspect"),
    (MenuItem::Edit, "Edit metadata"),
    (MenuItem::SyncEmbed, "Sync embed"),
    (MenuItem::Push, "Push to device"),
    (MenuItem::Delete, "Delete"),
];

#[derive(Debug)]
pub enum LibraryAction {
    None,
    Back,
    OpenPalette,
    Status(StatusMessage),
    OpenReader {
        catalog_dir: PathBuf,
        book: Box<Book>,
    },
    // Sync is device-wide, not book-specific: hand off to the Devices screen,
    // which resolves the current device and renders the sync plan.
    OpenDeviceSync,
    // Curation is catalog-wide: open the dedicated Duplicates screen.
    OpenDuplicates,
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
            filter: None,
            device_presence: HashMap::new(),
            current_device_label: None,
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
        let result = match &self.filter {
            Some(f) => search_rows(&ctx.dir, &f.criteria),
            None => list_rows(&ctx.dir),
        };
        match result {
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
        let snapshot = fetch_device_presence(&ctx.dir);
        self.device_presence = snapshot.presence;
        self.current_device_label = snapshot.label;
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
        KeyCode::Enter => open_context_menu(state),
        KeyCode::Char('i') => open_inspect(state),
        KeyCode::Char('o') => open_reader_from_table(state),
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
        KeyCode::Char('p') => open_confirm_push(state),
        KeyCode::Char('s') => LibraryAction::OpenDeviceSync,
        KeyCode::Char('d') | KeyCode::Delete => open_confirm_rm(state),
        KeyCode::Char('D') => LibraryAction::OpenDuplicates,
        KeyCode::Char('/') => open_filter_input(state),
        KeyCode::Esc => {
            // In filtered mode, Esc drops the filter and returns to the full
            // list; only an unfiltered Library hands control back to Welcome.
            if state.filter.is_some() {
                state.filter = None;
                state.refresh();
                LibraryAction::None
            } else {
                LibraryAction::Back
            }
        }
        KeyCode::Char(':') => LibraryAction::OpenPalette,
        _ => LibraryAction::None,
    }
}

fn open_filter_input(state: &mut State) -> LibraryAction {
    if state.catalog.is_none() {
        return LibraryAction::Status(StatusMessage::error("no catalog selected"));
    }
    // Re-opening `/` on a quick filter lets the user edit the current term.
    // An advanced filter is abandoned: `/` starts a fresh quick query.
    let prefill = match &state.filter {
        Some(ActiveFilter {
            criteria,
            kind: FilterKind::Quick,
        }) => criteria.query.clone().unwrap_or_default(),
        _ => String::new(),
    };
    state.overlay = Some(Overlay::Filter(Input::default().with_value(prefill)));
    LibraryAction::None
}

pub fn open_search_wizard(state: &mut State) {
    state.overlay = Some(Overlay::Search(Box::new(search::State::from_filter(
        state.filter.as_ref(),
    ))));
}

fn handle_overlay_key(state: &mut State, key: KeyEvent) -> LibraryAction {
    match state.overlay {
        Some(Overlay::ContextMenu { .. }) => handle_context_menu_key(state, key),
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
        Some(Overlay::ConfirmPush { id, .. }) => handle_confirm_push_key(state, key, id),
        Some(Overlay::AddTree(_)) => handle_tree_key(state, key),
        Some(Overlay::AddResult(_)) => handle_add_result_key(state, key),
        Some(Overlay::Edit(_)) => handle_edit_key(state, key),
        Some(Overlay::EmbedJob(_)) => handle_embed_job_key(state, key),
        Some(Overlay::Columns(_)) => handle_columns_key(state, key),
        Some(Overlay::Filter(_)) => handle_filter_key(state, key),
        Some(Overlay::Search(_)) => handle_search_key(state, key),
        None => LibraryAction::None,
    }
}

fn handle_filter_key(state: &mut State, key: KeyEvent) -> LibraryAction {
    let Some(Overlay::Filter(input)) = state.overlay.as_mut() else {
        return LibraryAction::None;
    };
    match key.code {
        // Esc abandons the edit, leaving any prior filter untouched.
        KeyCode::Esc => {
            state.overlay = None;
            LibraryAction::None
        }
        KeyCode::Enter => {
            let query = input.value().trim().to_string();
            state.overlay = None;
            if query.is_empty() {
                // Committing an empty query clears the filter entirely.
                state.filter = None;
            } else {
                state.filter = Some(ActiveFilter {
                    criteria: FilterCriteria {
                        query: Some(query),
                        ..FilterCriteria::default()
                    },
                    kind: FilterKind::Quick,
                });
            }
            state.refresh();
            LibraryAction::None
        }
        _ => {
            input.handle_event(&crossterm::event::Event::Key(key));
            LibraryAction::None
        }
    }
}

fn handle_search_key(state: &mut State, key: KeyEvent) -> LibraryAction {
    let Some(Overlay::Search(wizard)) = state.overlay.as_mut() else {
        return LibraryAction::None;
    };
    match search::handle_key(wizard.as_mut(), key) {
        search::SearchAction::None => LibraryAction::None,
        search::SearchAction::Cancel => {
            state.overlay = None;
            LibraryAction::None
        }
        search::SearchAction::Apply(criteria) => {
            state.overlay = None;
            state.filter = Some(ActiveFilter {
                criteria,
                kind: FilterKind::Advanced,
            });
            state.refresh();
            LibraryAction::None
        }
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

fn open_context_menu(state: &mut State) -> LibraryAction {
    if state.rows.get(state.cursor).is_none() {
        return LibraryAction::None;
    }
    state.overlay = Some(Overlay::ContextMenu { cursor: 0 });
    LibraryAction::None
}

fn handle_context_menu_key(state: &mut State, key: KeyEvent) -> LibraryAction {
    let Some(Overlay::ContextMenu { cursor }) = state.overlay.as_mut() else {
        return LibraryAction::None;
    };
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            *cursor = (*cursor + 1) % MENU_ITEMS.len();
            LibraryAction::None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            *cursor = (*cursor + MENU_ITEMS.len() - 1) % MENU_ITEMS.len();
            LibraryAction::None
        }
        KeyCode::Esc => {
            state.overlay = None;
            LibraryAction::None
        }
        KeyCode::Enter => {
            let item = MENU_ITEMS[*cursor].0;
            state.overlay = None;
            run_menu_item(state, item)
        }
        _ => LibraryAction::None,
    }
}

// Each item reuses the exact flow its table/inspect shortcut triggers.
fn run_menu_item(state: &mut State, item: MenuItem) -> LibraryAction {
    match item {
        MenuItem::Open => open_reader_from_table(state),
        MenuItem::Inspect => open_inspect(state),
        MenuItem::Edit => open_edit_from_table(state),
        MenuItem::SyncEmbed => embed_from_table(state),
        MenuItem::Push => open_confirm_push(state),
        MenuItem::Delete => open_confirm_rm(state),
    }
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

fn open_reader_from_table(state: &mut State) -> LibraryAction {
    let Some(ctx) = state.catalog.clone() else {
        return LibraryAction::None;
    };
    let Some(book) = state.rows.get(state.cursor).cloned() else {
        return LibraryAction::None;
    };
    LibraryAction::OpenReader {
        catalog_dir: ctx.dir,
        book: Box::new(book),
    }
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
    let (book, path) = match state.overlay.as_ref() {
        Some(Overlay::Inspect {
            book,
            absolute_path,
        }) => (book.as_ref().clone(), absolute_path.clone()),
        _ => return LibraryAction::None,
    };
    embed_book(state, book, path)
}

fn embed_from_table(state: &mut State) -> LibraryAction {
    let Some(ctx) = state.catalog.clone() else {
        return LibraryAction::Status(StatusMessage::error("no catalog selected"));
    };
    let Some(book) = state.rows.get(state.cursor).cloned() else {
        return LibraryAction::None;
    };
    let path = ctx.dir.join(&book.file_path);
    embed_book(state, book, path)
}

fn embed_book(state: &mut State, book: Book, path: PathBuf) -> LibraryAction {
    let Some(ctx) = state.catalog.clone() else {
        return LibraryAction::Status(StatusMessage::error("no catalog selected"));
    };
    let format = match import::Format::parse_label(&book.format) {
        Some(f) => f,
        None => {
            return LibraryAction::Status(StatusMessage::error(format!(
                "unknown format `{}`",
                book.format
            )));
        }
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
        Some(Overlay::Search(wizard)) => search::captures_text_input(wizard),
        Some(Overlay::Filter(_)) => true,
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

fn open_confirm_push(state: &mut State) -> LibraryAction {
    let Some(book) = state.rows.get(state.cursor) else {
        return LibraryAction::None;
    };
    state.overlay = Some(Overlay::ConfirmPush {
        id: book.id,
        title: book.title.clone(),
    });
    LibraryAction::None
}

fn handle_confirm_push_key(state: &mut State, key: KeyEvent, id: i64) -> LibraryAction {
    match key.code {
        KeyCode::Char('y') | KeyCode::Enter => {
            state.overlay = None;
            do_push(state, id)
        }
        KeyCode::Char('n') | KeyCode::Esc => {
            state.overlay = None;
            LibraryAction::None
        }
        _ => LibraryAction::None,
    }
}

fn do_push(state: &mut State, id: i64) -> LibraryAction {
    let Some(ctx) = state.catalog.clone() else {
        return LibraryAction::Status(StatusMessage::error("no catalog selected"));
    };
    match push_book(&ctx.dir, id) {
        Ok(out) => {
            // Refresh flips the pushed row's indicator (○ → ●) and updates the badge.
            state.refresh();
            LibraryAction::Status(StatusMessage::info(format!(
                "pushed `{}` → {}",
                out.title,
                out.device_path.display()
            )))
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

fn search_rows(dir: &Path, criteria: &FilterCriteria) -> std::result::Result<Vec<Book>, String> {
    let conn = catalog::open_existing(dir).map_err(|e| e.to_string())?;
    books::handle_search(&conn, &criteria.as_filters()).map_err(|e| e.to_string())
}

// Presence of every catalog book against the current device, or an empty map
// when no single device is current+connected. Advisory display only: any error
// (no catalog, scan failure, presence query) collapses to empty so the library
// always renders.
#[derive(Default)]
struct DevicePresenceSnapshot {
    presence: HashMap<i64, device::presence::LibraryPresence>,
    label: Option<String>,
}

fn fetch_device_presence(dir: &Path) -> DevicePresenceSnapshot {
    let Ok(conn) = catalog::open_existing(dir) else {
        return DevicePresenceSnapshot::default();
    };
    let detected = device::detect();
    for found in &detected {
        let _ = devices::record_seen(&conn, &found.serial);
    }
    let Some(dev) = device::current_connected(&conn, &detected) else {
        return DevicePresenceSnapshot::default();
    };
    DevicePresenceSnapshot {
        presence: device::presence::library_presence(&conn, &dev.serial, &dev.mount_path)
            .unwrap_or_default(),
        label: Some(device_label(&conn, &dev.serial)),
    }
}

// Alias when one is set, else the serial — the badge label for a device.
fn device_label(conn: &rusqlite::Connection, serial: &str) -> String {
    devices::list(conn)
        .ok()
        .and_then(|known| {
            known
                .into_iter()
                .find(|d| d.serial == serial)
                .and_then(|d| d.alias)
        })
        .unwrap_or_else(|| serial.to_string())
}

fn remove_book(dir: &Path, id: i64, keep: bool) -> std::result::Result<books::RmOutcome, String> {
    let mut conn = catalog::open_existing(dir).map_err(|e| e.to_string())?;
    books::handle_rm(&mut conn, dir, &id.to_string(), keep).map_err(|e| e.to_string())
}

// Resolve the current/sole device (remembering it as current) and copy the book
// onto it. Both error types are flattened to display strings for the status line.
fn push_book(dir: &Path, id: i64) -> std::result::Result<device::push::PushOutcome, String> {
    let conn = catalog::open_existing(dir).map_err(|e| e.to_string())?;
    let detected = device::detect();
    let dev = device::resolve_target(&conn, &detected, None).map_err(|e| e.to_string())?;
    device::push::push(&conn, dir, &dev.serial, &dev.mount_path, &id.to_string())
        .map_err(|e| e.to_string())
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
    let mut header_spans = vec![Span::styled(
        title,
        Style::default().add_modifier(Modifier::BOLD),
    )];
    if let Some(filter) = &state.filter {
        header_spans.push(Span::styled(
            format!(
                " · filtered ({}): {}",
                state.rows.len(),
                filter_summary(filter)
            ),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }
    // Badge appears only when a current device is connected (● in green,
    // matching the Presence::Both glyph/color vocabulary).
    if let Some(label) = &state.current_device_label {
        header_spans.push(Span::styled(
            format!("  ● {label}"),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
    }
    let header = Paragraph::new(Line::from(header_spans));
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
        let msg = if state.filter.is_some() {
            "no matches for filter — Esc to clear"
        } else {
            "no books yet — press `a` to import one"
        };
        let p = Paragraph::new(Line::from(Span::styled(
            msg,
            Style::default().fg(Color::DarkGray),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(p, layout[1]);
    } else {
        render_table(frame, layout[1], state);
    }

    match &state.overlay {
        Some(Overlay::ContextMenu { cursor }) => render_context_menu(frame, area, state, *cursor),
        Some(Overlay::Inspect {
            book,
            absolute_path,
        }) => render_inspect_modal(frame, area, book.as_ref(), absolute_path),
        Some(Overlay::ConfirmRm { id, title }) => render_confirm_modal(frame, area, *id, title),
        Some(Overlay::ConfirmPush { id, title }) => {
            render_confirm_push_modal(frame, area, *id, title)
        }
        Some(Overlay::AddTree(tree)) => render_tree_modal(frame, area, tree),
        Some(Overlay::AddResult(result)) => add_result::render(frame, area, result),
        Some(Overlay::Edit(edit_state)) => edit::render(frame, area, edit_state.as_ref()),
        Some(Overlay::EmbedJob(job)) => embed_job::render(frame, area, job),
        Some(Overlay::Columns(picker)) => columns::render(frame, area, picker),
        Some(Overlay::Filter(input)) => render_filter_bar(frame, area, input),
        Some(Overlay::Search(wizard)) => search::render(frame, area, wizard.as_ref()),
        None => {}
    }
}

fn filter_summary(filter: &ActiveFilter) -> String {
    let c = &filter.criteria;
    match filter.kind {
        FilterKind::Quick => format!("/{}", c.query.as_deref().unwrap_or_default()),
        FilterKind::Advanced => {
            let mut parts: Vec<String> = Vec::new();
            if let Some(q) = &c.query {
                parts.push(q.clone());
            }
            if let Some(a) = &c.author {
                parts.push(format!("author={a}"));
            }
            for t in &c.tags {
                parts.push(format!("tag={t}"));
            }
            if let Some(s) = &c.series {
                parts.push(format!("series={s}"));
            }
            if let Some(r) = &c.rating {
                if r.min == r.max {
                    parts.push(format!("rating={}", r.min));
                } else {
                    parts.push(format!("rating={}..{}", r.min, r.max));
                }
            }
            parts.join(" ")
        }
    }
}

fn render_filter_bar(frame: &mut Frame<'_>, area: Rect, input: &Input) {
    // A single-line vim-style input pinned to the bottom row of the Library
    // pane, rather than a centered modal.
    let bar = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    frame.render_widget(Clear, bar);
    let prefix = "/";
    let p = Paragraph::new(Line::from(vec![
        Span::styled(prefix, Style::default().fg(Color::Yellow)),
        Span::styled(
            input.value().to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
    ]));
    frame.render_widget(p, bar);
    let cx = bar
        .x
        .saturating_add(prefix.len() as u16)
        .saturating_add(input.visual_cursor() as u16)
        .min(bar.x + bar.width.saturating_sub(1));
    frame.set_cursor_position((cx, bar.y));
}

fn render_table(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let columns: Vec<LibraryColumn> = if state.columns.is_empty() {
        LibraryColumn::DEFAULT.to_vec()
    } else {
        state.columns.clone()
    };
    // The presence indicator is a synthetic leading column — it only shows when a
    // device is current+connected, and is never part of the persisted columns.
    let show_presence = !state.device_presence.is_empty();

    let mut header_cells: Vec<Cell> = Vec::with_capacity(columns.len() + 1);
    if show_presence {
        header_cells.push(Cell::from(""));
    }
    header_cells.extend(columns.iter().map(|c| Cell::from(c.header())));
    let header = Row::new(header_cells).style(
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row<'_>> = state
        .rows
        .iter()
        .map(|b| {
            let presence = state.device_presence.get(&b.id);
            let mut cells: Vec<Cell> = Vec::with_capacity(columns.len() + 1);
            if show_presence {
                cells.push(indicator_cell(presence));
            }
            for c in &columns {
                if *c == LibraryColumn::Title {
                    cells.push(title_cell(b, presence));
                } else {
                    cells.push(Cell::from(c.render(b)));
                }
            }
            Row::new(cells)
        })
        .collect();

    let mut widths: Vec<Constraint> = Vec::with_capacity(columns.len() + 1);
    if show_presence {
        widths.push(Constraint::Length(2));
    }
    widths.extend(columns.iter().map(|c| c.width()));

    let table = Table::new(rows, widths).header(header).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    let mut s = TableState::default();
    s.select(Some(state.cursor.min(state.rows.len().saturating_sub(1))));
    frame.render_stateful_widget(table, area, &mut s);
}

// Glyph + color for a book's presence against the current device. `None` (a book
// somehow absent from the map) and the device-side variants render blank.
fn indicator(presence: &device::presence::LibraryPresence) -> (&'static str, Color) {
    use device::books::Presence;
    match presence.state {
        Presence::Both => ("●", Color::Green),
        Presence::Modified => ("▲", Color::Yellow),
        Presence::LocalOnly => ("○", Color::DarkGray),
        Presence::DeviceOnly | Presence::Conflict => ("", Color::Reset),
    }
}

fn indicator_cell(presence: Option<&device::presence::LibraryPresence>) -> Cell<'static> {
    match presence {
        Some(p) => {
            let (glyph, color) = indicator(p);
            Cell::from(Span::styled(glyph, Style::default().fg(color)))
        }
        None => Cell::from(""),
    }
}

// The Title cell, with a dim "(local → device)" suffix when the device copy is a
// different format from the catalog file.
fn title_cell(book: &Book, presence: Option<&device::presence::LibraryPresence>) -> Cell<'static> {
    match presence.and_then(|p| p.device_format.as_deref()) {
        Some(device_format) => Cell::from(Line::from(vec![
            Span::raw(book.title.clone()),
            Span::styled(
                format!(" ({} → {})", book.format, device_format),
                Style::default().fg(Color::DarkGray),
            ),
        ])),
        None => Cell::from(book.title.clone()),
    }
}

fn render_context_menu(frame: &mut Frame<'_>, area: Rect, state: &State, cursor: usize) {
    let book_title = state
        .rows
        .get(state.cursor)
        .map(|b| b.title.as_str())
        .unwrap_or_default();
    let modal_title = format!(" {book_title} ");
    let max_label = MENU_ITEMS
        .iter()
        .map(|(_, label)| label.chars().count())
        .max()
        .unwrap_or(0);
    let target_w = (max_label + 6)
        .max(modal_title.chars().count() + 4)
        .min(area.width as usize) as u16;
    let target_h = (MENU_ITEMS.len() as u16 + 2).min(area.height);
    let rect = centered_rect(target_w, target_h, area);

    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(modal_title)
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let items: Vec<ListItem> = MENU_ITEMS
        .iter()
        .map(|(_, label)| ListItem::new(Line::from(format!(" {label}"))))
        .collect();
    let list = List::new(items).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    let mut list_state = ListState::default();
    list_state.select(Some(cursor.min(MENU_ITEMS.len() - 1)));
    frame.render_stateful_widget(list, inner, &mut list_state);
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

fn render_confirm_push_modal(frame: &mut Frame<'_>, area: Rect, id: i64, title: &str) {
    let lines = vec![
        Line::from(Span::raw(format!(
            "Push book `{title}` (id {id}) to current device?"
        ))),
        Line::from(""),
        Line::from(vec![
            Span::styled("y", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("/Enter — push   "),
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
        // The library refresh now scans for devices; keep tests host-independent.
        std::env::set_var(device::DISABLE_SCAN_ENV, "1");
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
    fn indicator_glyphs_per_presence_state() {
        use device::books::Presence;
        let lp = |state| device::presence::LibraryPresence {
            state,
            device_format: None,
        };
        assert_eq!(indicator(&lp(Presence::Both)).0, "●");
        assert_eq!(indicator(&lp(Presence::Modified)).0, "▲");
        assert_eq!(indicator(&lp(Presence::LocalOnly)).0, "○");
        // Device-side variants never appear in the library map; render blank.
        assert_eq!(indicator(&lp(Presence::DeviceOnly)).0, "");
    }

    #[test]
    fn device_label_prefers_alias_then_serial() {
        let (_tmp, dir, _reg) = setup_with_catalog();
        let conn = catalog::open_existing(&dir).unwrap();
        devices::record_seen(&conn, "SERIAL-1").unwrap();
        devices::record_seen(&conn, "SERIAL-2").unwrap();
        devices::set_alias(&conn, "SERIAL-1", "kindle").unwrap();

        assert_eq!(device_label(&conn, "SERIAL-1"), "kindle");
        // No alias set → falls back to the serial.
        assert_eq!(device_label(&conn, "SERIAL-2"), "SERIAL-2");
    }

    #[test]
    fn fetch_device_presence_empty_without_device() {
        // setup_with_catalog sets DISABLE_SCAN_ENV, so detect() finds nothing.
        let (_tmp, dir, _reg) = setup_with_catalog();
        let snapshot = fetch_device_presence(&dir);
        assert!(snapshot.presence.is_empty());
        assert!(snapshot.label.is_none());
    }

    #[test]
    fn push_book_surfaces_no_device_error() {
        // With the device scan disabled, resolve_target reports "no device".
        let (_tmp, dir, _reg) = setup_with_catalog();
        let id = insert_book_with_file(&dir, "Dune", "Frank Herbert");
        let err = push_book(&dir, id).unwrap_err();
        assert!(err.contains("no device"), "unexpected error: {err}");
    }

    #[test]
    fn load_without_connected_device_has_empty_presence() {
        // No device is connected (scan disabled), so the indicator column is off.
        let (_tmp, dir, reg) = setup_with_catalog();
        insert_book(&dir, "Dune", Some("Herbert"));
        let state = State::load(&reg);
        assert!(state.device_presence.is_empty());
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

    fn type_chars(state: &mut State, text: &str) {
        for ch in text.chars() {
            handle_key(state, key(KeyCode::Char(ch)));
        }
    }

    #[test]
    fn slash_opens_filter_and_enter_applies_quick() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book(&cat, "Dune", Some("Herbert"));
        insert_book(&cat, "Hyperion", Some("Simmons"));
        let mut state = State::load(&reg);
        assert_eq!(state.rows.len(), 2);

        handle_key(&mut state, key(KeyCode::Char('/')));
        assert!(matches!(state.overlay, Some(Overlay::Filter(_))));
        type_chars(&mut state, "dune");
        handle_key(&mut state, key(KeyCode::Enter));

        assert!(state.overlay.is_none());
        assert!(matches!(
            state.filter,
            Some(ActiveFilter {
                kind: FilterKind::Quick,
                ..
            })
        ));
        assert_eq!(state.rows.len(), 1);
        assert_eq!(state.rows[0].title, "Dune");
    }

    #[test]
    fn esc_clears_filter_then_returns_back() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book(&cat, "Dune", None);
        insert_book(&cat, "Hyperion", None);
        let mut state = State::load(&reg);

        handle_key(&mut state, key(KeyCode::Char('/')));
        type_chars(&mut state, "dune");
        handle_key(&mut state, key(KeyCode::Enter));
        assert_eq!(state.rows.len(), 1);

        // First Esc clears the filter and restores the full list.
        let action = handle_key(&mut state, key(KeyCode::Esc));
        assert!(matches!(action, LibraryAction::None));
        assert!(state.filter.is_none());
        assert_eq!(state.rows.len(), 2);

        // Second Esc (normal mode) hands back to Welcome.
        let action = handle_key(&mut state, key(KeyCode::Esc));
        assert!(matches!(action, LibraryAction::Back));
    }

    #[test]
    fn empty_query_on_enter_clears_filter() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book(&cat, "Dune", None);
        let mut state = State::load(&reg);
        handle_key(&mut state, key(KeyCode::Char('/')));
        type_chars(&mut state, "dune");
        handle_key(&mut state, key(KeyCode::Enter));
        assert!(state.filter.is_some());

        // Re-open and commit an empty query.
        handle_key(&mut state, key(KeyCode::Char('/')));
        handle_key(&mut state, key(KeyCode::Backspace));
        handle_key(&mut state, key(KeyCode::Backspace));
        handle_key(&mut state, key(KeyCode::Backspace));
        handle_key(&mut state, key(KeyCode::Backspace));
        handle_key(&mut state, key(KeyCode::Enter));
        assert!(state.filter.is_none());
    }

    #[test]
    fn slash_prefills_quick_term_but_not_advanced() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book(&cat, "Dune", None);
        let mut state = State::load(&reg);

        handle_key(&mut state, key(KeyCode::Char('/')));
        type_chars(&mut state, "dune");
        handle_key(&mut state, key(KeyCode::Enter));

        handle_key(&mut state, key(KeyCode::Char('/')));
        match &state.overlay {
            Some(Overlay::Filter(input)) => assert_eq!(input.value(), "dune"),
            _ => panic!("expected filter overlay"),
        }
        handle_key(&mut state, key(KeyCode::Esc));

        // An advanced filter is abandoned: `/` starts fresh.
        state.filter = Some(ActiveFilter {
            criteria: FilterCriteria {
                author: Some("Herbert".to_string()),
                ..FilterCriteria::default()
            },
            kind: FilterKind::Advanced,
        });
        handle_key(&mut state, key(KeyCode::Char('/')));
        match &state.overlay {
            Some(Overlay::Filter(input)) => assert_eq!(input.value(), ""),
            _ => panic!("expected filter overlay"),
        }
    }

    #[test]
    fn open_search_wizard_prefills_from_filter() {
        let (_tmp, _cat, reg) = setup_with_catalog();
        let mut state = State::load(&reg);
        state.filter = Some(ActiveFilter {
            criteria: FilterCriteria {
                query: Some("dune".to_string()),
                ..FilterCriteria::default()
            },
            kind: FilterKind::Quick,
        });
        open_search_wizard(&mut state);
        match &state.overlay {
            Some(Overlay::Search(w)) => {
                assert_eq!(w.query.value(), "dune");
                assert_eq!(w.author.value(), "");
            }
            _ => panic!("expected search overlay"),
        }
    }

    #[test]
    fn search_wizard_apply_filters_rows_advanced() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book(&cat, "Dune", Some("Herbert"));
        insert_book(&cat, "Hyperion", Some("Simmons"));
        let mut state = State::load(&reg);

        open_search_wizard(&mut state);
        if let Some(Overlay::Search(w)) = state.overlay.as_mut() {
            w.focus = search::Focus::Author;
        }
        type_chars(&mut state, "Herbert");
        if let Some(Overlay::Search(w)) = state.overlay.as_mut() {
            w.focus = search::Focus::Apply;
        }
        handle_key(&mut state, key(KeyCode::Enter));

        assert!(state.overlay.is_none());
        assert!(matches!(
            state.filter,
            Some(ActiveFilter {
                kind: FilterKind::Advanced,
                ..
            })
        ));
        assert_eq!(state.rows.len(), 1);
        assert_eq!(state.rows[0].title, "Dune");
    }

    #[test]
    fn read_tree_includes_supported_files_and_dirs_only() {
        let tmp = tempdir().unwrap();
        fs::write(tmp.path().join("book.epub"), b"x").unwrap();
        fs::write(tmp.path().join("book.PDF"), b"x").unwrap();
        fs::write(tmp.path().join("note.txt"), b"x").unwrap();
        fs::write(tmp.path().join("ignored.doc"), b"x").unwrap();
        fs::create_dir(tmp.path().join("subdir")).unwrap();
        let entries = read_tree(tmp.path()).unwrap();
        let names: Vec<String> = entries.iter().map(label).collect();
        assert!(names.iter().any(|n| n == "book.epub"));
        assert!(names.iter().any(|n| n == "book.PDF"));
        assert!(names.iter().any(|n| n == "note.txt"));
        assert!(!names.iter().any(|n| n == "ignored.doc"));
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
    fn s_requests_device_sync() {
        let (_tmp, _cat, reg) = setup_with_catalog();
        let mut state = State::load(&reg);
        let action = handle_key(&mut state, key(KeyCode::Char('s')));
        assert!(matches!(action, LibraryAction::OpenDeviceSync));
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
    fn enter_on_table_opens_context_menu_with_cursor_on_open() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book_with_file(&cat, "Book", "Author");
        let mut state = State::load(&reg);
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, LibraryAction::None));
        match state.overlay.as_ref() {
            Some(Overlay::ContextMenu { cursor }) => {
                assert_eq!(*cursor, 0);
                assert_eq!(MENU_ITEMS[*cursor].0, MenuItem::Open);
            }
            other => panic!("expected ContextMenu overlay, got {other:?}"),
        }
        assert_eq!(help_sections(&state)[0].title, "Actions");
    }

    #[test]
    fn enter_on_empty_table_does_nothing() {
        let (_tmp, _cat, reg) = setup_with_catalog();
        let mut state = State::load(&reg);
        handle_key(&mut state, key(KeyCode::Enter));
        assert!(state.overlay.is_none());
    }

    #[test]
    fn context_menu_cursor_cycles_and_esc_closes() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book_with_file(&cat, "Book", "Author");
        let mut state = State::load(&reg);
        handle_key(&mut state, key(KeyCode::Enter));

        handle_key(&mut state, key(KeyCode::Char('j')));
        match state.overlay.as_ref() {
            Some(Overlay::ContextMenu { cursor }) => assert_eq!(*cursor, 1),
            _ => panic!("expected ContextMenu overlay"),
        }
        handle_key(&mut state, key(KeyCode::Char('k')));
        handle_key(&mut state, key(KeyCode::Char('k')));
        match state.overlay.as_ref() {
            Some(Overlay::ContextMenu { cursor }) => {
                assert_eq!(*cursor, MENU_ITEMS.len() - 1, "k on first item wraps")
            }
            _ => panic!("expected ContextMenu overlay"),
        }
        handle_key(&mut state, key(KeyCode::Esc));
        assert!(state.overlay.is_none());
    }

    fn select_menu_item(state: &mut State, item: MenuItem) -> LibraryAction {
        handle_key(state, key(KeyCode::Enter));
        let target = MENU_ITEMS.iter().position(|(i, _)| *i == item).unwrap();
        for _ in 0..target {
            handle_key(state, key(KeyCode::Down));
        }
        handle_key(state, key(KeyCode::Enter))
    }

    #[test]
    fn context_menu_open_returns_open_reader_action() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book_with_file(&cat, "Book", "Author");
        let mut state = State::load(&reg);
        let action = select_menu_item(&mut state, MenuItem::Open);
        match action {
            LibraryAction::OpenReader { book, .. } => assert_eq!(book.title, "Book"),
            other => panic!("expected OpenReader, got {other:?}"),
        }
        assert!(state.overlay.is_none());
    }

    #[test]
    fn context_menu_inspect_opens_inspect_overlay() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book_with_file(&cat, "Book", "Author");
        let mut state = State::load(&reg);
        select_menu_item(&mut state, MenuItem::Inspect);
        assert!(matches!(state.overlay, Some(Overlay::Inspect { .. })));
    }

    #[test]
    fn context_menu_edit_opens_edit_with_table_origin() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book_with_file(&cat, "Book", "Author");
        let mut state = State::load(&reg);
        select_menu_item(&mut state, MenuItem::Edit);
        match state.overlay.as_ref() {
            Some(Overlay::Edit(s)) => assert_eq!(s.origin, edit::Origin::Table),
            other => panic!("expected Edit overlay, got {other:?}"),
        }
    }

    #[test]
    fn context_menu_delete_opens_confirm_rm() {
        let (_tmp, cat, reg) = setup_with_catalog();
        insert_book_with_file(&cat, "Doomed", "Author");
        let mut state = State::load(&reg);
        select_menu_item(&mut state, MenuItem::Delete);
        match state.overlay.as_ref() {
            Some(Overlay::ConfirmRm { title, .. }) => assert_eq!(title, "Doomed"),
            other => panic!("expected ConfirmRm overlay, got {other:?}"),
        }
    }

    #[test]
    fn context_menu_sync_embed_respects_synced_status() {
        let (_tmp, cat, reg) = setup_with_catalog();
        let id = insert_book_with_file(&cat, "Synced", "Author");
        let conn = catalog::open_existing(&cat).unwrap();
        books::mark_embed_synced(&conn, id).unwrap();
        drop(conn);

        let mut state = State::load(&reg);
        let action = select_menu_item(&mut state, MenuItem::SyncEmbed);
        match action {
            LibraryAction::Status(s) => {
                assert!(s.text.contains("already synced"), "got status: {}", s.text);
            }
            other => panic!("expected status message, got {other:?}"),
        }
        assert!(state.overlay.is_none());
    }

    #[test]
    fn context_menu_sync_embed_unsupported_format_reports_error() {
        let (_tmp, cat, reg) = setup_with_catalog();
        let conn = catalog::open_existing(&cat).unwrap();
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES ('M', 'A', 'mobi', 'books/1/A_-_M.mobi')",
            [],
        )
        .unwrap();
        drop(conn);

        let mut state = State::load(&reg);
        let action = select_menu_item(&mut state, MenuItem::SyncEmbed);
        match action {
            LibraryAction::Status(s) => {
                assert!(s.text.contains("not supported"), "got status: {}", s.text);
            }
            other => panic!("expected status message, got {other:?}"),
        }
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
