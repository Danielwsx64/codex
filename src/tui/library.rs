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

use crate::catalog::books::{self, Book};
use crate::catalog::{self};
use crate::config::Registry;
use crate::import;
use crate::tui::widgets::{centered_rect, render_modal, StatusMessage};

#[derive(Debug)]
pub struct State {
    pub catalog: Option<CatalogContext>,
    pub rows: Vec<Book>,
    pub cursor: usize,
    pub overlay: Option<Overlay>,
    pub load_error: Option<String>,
    pub cwd: PathBuf,
}

#[derive(Debug, Clone)]
pub struct CatalogContext {
    pub name: String,
    pub dir: PathBuf,
}

#[derive(Debug)]
pub enum Overlay {
    Inspect { book: Book, absolute_path: PathBuf },
    ConfirmRm { id: i64, title: String },
    AddTree(AddTreeState),
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
        };
        if let Ok(entry) = registry.resolve(None) {
            state.catalog = Some(CatalogContext {
                name: entry.name.clone(),
                dir: entry.path.clone(),
            });
            state.refresh();
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
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('i') | KeyCode::Char('q') => {
                state.overlay = None;
                LibraryAction::None
            }
            _ => LibraryAction::None,
        },
        Some(Overlay::ConfirmRm { id, .. }) => handle_confirm_key(state, key, id),
        Some(Overlay::AddTree(_)) => handle_tree_key(state, key),
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
        book,
        absolute_path,
    });
    LibraryAction::None
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
            let total = outcome.rows.len();
            let ok = outcome
                .rows
                .iter()
                .filter(|r| matches!(r.status, books::AddStatus::Imported))
                .count();
            let msg = if ok == total {
                StatusMessage::info(format!("imported {ok} / {total} file(s)"))
            } else if ok == 0 {
                StatusMessage::error(format!("failed to import all {total} file(s)"))
            } else {
                StatusMessage::error(format!("imported {ok} / {total} file(s); some failed"))
            };
            LibraryAction::Status(msg)
        }
        Err(err) => LibraryAction::Status(StatusMessage::error(err)),
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
    Ok(books::handle_add(&mut conn, dir, paths))
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
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    let title = match &state.catalog {
        Some(ctx) => format!("Library — {}", ctx.name),
        None => "Library".to_string(),
    };
    let header = Paragraph::new(vec![
        Line::from(Span::styled(
            title,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "↑↓ select · i info · a add · d delete · Esc back",
            Style::default().fg(Color::DarkGray),
        )),
    ]);
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
        }) => render_inspect_modal(frame, area, book, absolute_path),
        Some(Overlay::ConfirmRm { id, title }) => render_confirm_modal(frame, area, *id, title),
        Some(Overlay::AddTree(tree)) => render_tree_modal(frame, area, tree),
        None => {}
    }
}

fn render_table(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let header = Row::new(vec!["id", "title", "author", "format"]).style(
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    let rows: Vec<Row<'_>> = state
        .rows
        .iter()
        .map(|b| {
            Row::new(vec![
                b.id.to_string(),
                b.title.clone(),
                b.author.clone().unwrap_or_else(|| "(unknown)".to_string()),
                b.format.clone(),
            ])
        })
        .collect();
    let widths = [
        Constraint::Length(5),
        Constraint::Percentage(55),
        Constraint::Percentage(30),
        Constraint::Length(6),
    ];
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
    let lines = vec![
        kv_line("id", &book.id.to_string()),
        kv_line("title", &book.title),
        kv_line("author", book.author.as_deref().unwrap_or("(unknown)")),
        kv_line("format", &book.format),
        kv_line("file", &absolute_path.display().to_string()),
        kv_line("added", &book.added_at),
    ];
    render_modal(frame, area, "inspect", lines);
}

fn kv_line(key: &str, value: &str) -> Line<'static> {
    Line::from(Span::raw(format!("{key:<8}{value}")))
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

    let hint = Paragraph::new(Line::from(Span::styled(
        "↑↓ move · Space toggle · Enter open/import · Backspace up · Esc cancel",
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(hint, layout[3]);
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
}
