use std::path::{Path, PathBuf};

use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

use crate::catalog::{self, devices};
use crate::config::Registry;
use crate::device;
use crate::device::books::Presence;
use crate::tui::help::{Binding, Section};
use crate::tui::widgets::{render_modal, StatusMessage};

const LIST_BINDINGS: &[Binding] = &[
    Binding {
        keys: "↑↓ / j k",
        desc: "move selection",
    },
    Binding {
        keys: "Enter",
        desc: "view books on device",
    },
    Binding {
        keys: "r",
        desc: "rename device alias",
    },
];

const BOOKS_BINDINGS: &[Binding] = &[
    Binding {
        keys: "↑↓ / j k",
        desc: "move selection",
    },
    Binding {
        keys: "Esc",
        desc: "back to device list",
    },
];

const RENAME_BINDINGS: &[Binding] = &[
    Binding {
        keys: "Enter",
        desc: "save alias",
    },
    Binding {
        keys: "Esc",
        desc: "cancel",
    },
];

pub fn help_sections(state: &State) -> Vec<Section> {
    if state.rename.is_some() {
        return vec![Section {
            title: "Rename device",
            bindings: RENAME_BINDINGS,
        }];
    }
    match state.view {
        View::List => vec![Section {
            title: "Devices",
            bindings: LIST_BINDINGS,
        }],
        View::Books(_) => vec![Section {
            title: "Device books",
            bindings: BOOKS_BINDINGS,
        }],
    }
}

#[derive(Debug)]
pub struct State {
    pub catalog: Option<CatalogContext>,
    pub rows: Vec<device::DeviceRow>,
    pub cursor: usize,
    pub view: View,
    pub rename: Option<RenameInput>,
    pub load_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CatalogContext {
    pub name: String,
    pub dir: PathBuf,
}

#[derive(Debug)]
pub enum View {
    List,
    Books(BooksView),
}

#[derive(Debug)]
pub struct BooksView {
    pub serial: String,
    pub alias: String,
    pub rows: Vec<device::books::DeviceBook>,
    pub cursor: usize,
}

#[derive(Debug, Clone)]
pub struct RenameInput {
    pub serial: String,
    pub input: Input,
    pub error: Option<String>,
}

impl State {
    pub fn load(registry: &Registry) -> Self {
        let mut state = Self {
            catalog: None,
            rows: Vec::new(),
            cursor: 0,
            view: View::List,
            rename: None,
            load_error: None,
        };
        match registry.resolve(None) {
            Ok(entry) => {
                state.catalog = Some(CatalogContext {
                    name: entry.name.clone(),
                    dir: entry.path.clone(),
                });
                state.refresh();
            }
            Err(err) => {
                state.load_error = Some(err.to_string());
            }
        }
        state
    }

    fn refresh(&mut self) {
        let Some(dir) = self.catalog.as_ref().map(|c| c.dir.clone()) else {
            return;
        };
        match fetch_rows(&dir) {
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

// Mirror `cdx device ls`: persist every detected device so first-seen ones show
// up and `last_seen_at` refreshes, then merge the known (DB) set with live
// detection and fill in free space / book count.
fn fetch_rows(dir: &Path) -> Result<Vec<device::DeviceRow>, String> {
    let conn = catalog::open_existing(dir).map_err(|e| e.to_string())?;
    let detected = device::detect();
    for found in &detected {
        devices::record_seen(&conn, &found.serial).map_err(|e| e.to_string())?;
    }
    let known = devices::list(&conn).map_err(|e| e.to_string())?;
    let mut rows = device::build_device_rows(&detected, &known);
    device::enrich(&mut rows);
    Ok(rows)
}

pub fn captures_text_input(state: &State) -> bool {
    state.rename.is_some()
}

pub enum DevicesAction {
    None,
    Back,
    OpenPalette,
    Status(StatusMessage),
}

pub fn handle_key(state: &mut State, key: KeyEvent) -> DevicesAction {
    if state.rename.is_some() {
        return handle_rename_key(state, key);
    }
    if matches!(state.view, View::Books(_)) {
        return handle_books_key(state, key);
    }
    handle_list_key(state, key)
}

fn handle_list_key(state: &mut State, key: KeyEvent) -> DevicesAction {
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            if !state.rows.is_empty() {
                state.cursor = (state.cursor + 1) % state.rows.len();
            }
            DevicesAction::None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if !state.rows.is_empty() {
                state.cursor = (state.cursor + state.rows.len() - 1) % state.rows.len();
            }
            DevicesAction::None
        }
        KeyCode::Enter => open_books(state),
        KeyCode::Char('r') => open_rename(state),
        KeyCode::Esc => DevicesAction::Back,
        KeyCode::Char(':') => DevicesAction::OpenPalette,
        _ => DevicesAction::None,
    }
}

fn open_books(state: &mut State) -> DevicesAction {
    let Some(row) = state.rows.get(state.cursor).cloned() else {
        return DevicesAction::None;
    };
    let label = row.alias.clone().unwrap_or_else(|| row.serial.clone());
    let Some(mount) = row.mount_path.clone().filter(|_| row.connected) else {
        return DevicesAction::Status(StatusMessage::error(format!(
            "device `{label}` is not connected"
        )));
    };
    let Some(dir) = state.catalog.as_ref().map(|c| c.dir.clone()) else {
        return DevicesAction::None;
    };
    let conn = match catalog::open_existing(&dir) {
        Ok(conn) => conn,
        Err(err) => return DevicesAction::Status(StatusMessage::error(err.to_string())),
    };
    match device::books::list(&conn, &row.serial, &mount) {
        Ok(rows) => {
            state.view = View::Books(BooksView {
                serial: row.serial,
                alias: label,
                rows,
                cursor: 0,
            });
            DevicesAction::None
        }
        Err(err) => DevicesAction::Status(StatusMessage::error(err.to_string())),
    }
}

fn open_rename(state: &mut State) -> DevicesAction {
    let Some(row) = state.rows.get(state.cursor) else {
        return DevicesAction::None;
    };
    let seed = row.alias.clone().unwrap_or_default();
    state.rename = Some(RenameInput {
        serial: row.serial.clone(),
        input: Input::default().with_value(seed),
        error: None,
    });
    DevicesAction::None
}

fn handle_rename_key(state: &mut State, key: KeyEvent) -> DevicesAction {
    match key.code {
        KeyCode::Esc => {
            state.rename = None;
            DevicesAction::None
        }
        KeyCode::Enter => submit_rename(state),
        _ => {
            if let Some(rename) = state.rename.as_mut() {
                rename.error = None;
                rename.input.handle_event(&Event::Key(key));
            }
            DevicesAction::None
        }
    }
}

fn submit_rename(state: &mut State) -> DevicesAction {
    let Some(rename) = state.rename.clone() else {
        return DevicesAction::None;
    };
    let Some(dir) = state.catalog.as_ref().map(|c| c.dir.clone()) else {
        state.rename = None;
        return DevicesAction::None;
    };
    let new_alias = rename.input.value().trim().to_string();
    let conn = match catalog::open_existing(&dir) {
        Ok(conn) => conn,
        Err(err) => {
            set_rename_error(state, err.to_string());
            return DevicesAction::None;
        }
    };
    match devices::handle_alias(&conn, &rename.serial, &new_alias) {
        Ok(outcome) => {
            drop(conn);
            state.rename = None;
            state.refresh();
            DevicesAction::Status(StatusMessage::info(format!(
                "renamed device to `{}`",
                outcome.alias
            )))
        }
        Err(err) => {
            set_rename_error(state, err.to_string());
            DevicesAction::None
        }
    }
}

fn set_rename_error(state: &mut State, message: String) {
    if let Some(rename) = state.rename.as_mut() {
        rename.error = Some(message);
    }
}

fn handle_books_key(state: &mut State, key: KeyEvent) -> DevicesAction {
    let View::Books(view) = &mut state.view else {
        return DevicesAction::None;
    };
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            if !view.rows.is_empty() {
                view.cursor = (view.cursor + 1) % view.rows.len();
            }
            DevicesAction::None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if !view.rows.is_empty() {
                view.cursor = (view.cursor + view.rows.len() - 1) % view.rows.len();
            }
            DevicesAction::None
        }
        KeyCode::Esc => {
            state.view = View::List;
            DevicesAction::None
        }
        KeyCode::Char(':') => DevicesAction::OpenPalette,
        _ => DevicesAction::None,
    }
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &State) {
    match &state.view {
        View::List => render_list(frame, area, state),
        View::Books(view) => render_books(frame, area, view),
    }
    if let Some(rename) = &state.rename {
        render_rename(frame, area, rename);
    }
}

fn render_list(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    let title = match &state.catalog {
        Some(ctx) => format!("Devices — {}", ctx.name),
        None => "Devices".to_string(),
    };
    let header = Paragraph::new(Line::from(Span::styled(
        title,
        Style::default().add_modifier(Modifier::BOLD),
    )));
    frame.render_widget(header, layout[0]);

    if let Some(err) = &state.load_error {
        let p = Paragraph::new(Line::from(Span::styled(
            format!("  {err}"),
            Style::default().fg(Color::Red),
        )));
        frame.render_widget(p, layout[1]);
        return;
    }

    if state.rows.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            "  no devices known — connect a Kindle over USB",
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(empty, layout[1]);
        return;
    }

    let items: Vec<ListItem> = state
        .rows
        .iter()
        .map(|row| ListItem::new(device_row_line(row)))
        .collect();
    let list = List::new(items).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    let mut list_state = ListState::default();
    list_state.select(Some(state.cursor.min(state.rows.len() - 1)));
    frame.render_stateful_widget(list, layout[1], &mut list_state);
}

fn render_books(frame: &mut Frame<'_>, area: Rect, view: &BooksView) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    let header = Paragraph::new(Line::from(Span::styled(
        format!("Devices › {}", view.alias),
        Style::default().add_modifier(Modifier::BOLD),
    )));
    frame.render_widget(header, layout[0]);

    if view.rows.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            "  no books on device",
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(empty, layout[1]);
        return;
    }

    let items: Vec<ListItem> = view
        .rows
        .iter()
        .map(|book| ListItem::new(book_row_line(book)))
        .collect();
    let list = List::new(items).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    let mut list_state = ListState::default();
    list_state.select(Some(view.cursor.min(view.rows.len() - 1)));
    frame.render_stateful_widget(list, layout[1], &mut list_state);
}

fn render_rename(frame: &mut Frame<'_>, area: Rect, rename: &RenameInput) {
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let mut lines = vec![
        Line::from(Span::raw("New alias:")),
        Line::from(Span::styled(rename.input.value().to_string(), bold)),
    ];
    if let Some(err) = &rename.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            err.clone(),
            Style::default().fg(Color::Red),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Enter", bold),
        Span::raw(" — save   "),
        Span::styled("Esc", bold),
        Span::raw(" — cancel"),
    ]));
    render_modal(frame, area, "rename device", lines);
}

fn device_row_line(row: &device::DeviceRow) -> Line<'static> {
    let label = row.alias.clone().unwrap_or_else(|| row.serial.clone());
    let (marker, marker_style) = if row.connected {
        ("●", Style::default().fg(Color::Green))
    } else {
        ("○", Style::default().fg(Color::DarkGray))
    };
    let label_style = if row.connected {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(marker.to_string(), marker_style),
        Span::raw(" "),
        Span::styled(label, label_style),
        Span::raw("  "),
        Span::styled(row.serial.clone(), Style::default().fg(Color::DarkGray)),
    ];
    if row.connected {
        let free = row
            .free_bytes
            .map(format_bytes)
            .unwrap_or_else(|| "-".to_string());
        let books = row
            .book_count
            .map(|n| n.to_string())
            .unwrap_or_else(|| "-".to_string());
        spans.push(Span::styled(
            format!("  {free} free   {books} books"),
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        spans.push(Span::styled(
            "  (disconnected)".to_string(),
            Style::default().fg(Color::DarkGray),
        ));
    }
    Line::from(spans)
}

fn book_row_line(book: &device::books::DeviceBook) -> Line<'static> {
    let (tag, tag_style) = match book.presence {
        Presence::Both => ("both", Style::default().fg(Color::Green)),
        Presence::DeviceOnly => ("device only", Style::default().fg(Color::DarkGray)),
        Presence::Conflict => ("conflict", Style::default().fg(Color::Red)),
    };
    let title = book.title.clone().unwrap_or_else(|| "-".to_string());
    let author = book.author.clone().unwrap_or_else(|| "-".to_string());
    Line::from(vec![
        Span::raw(" "),
        Span::styled(format!("[{tag}]"), tag_style),
        Span::raw("  "),
        Span::raw(title),
        Span::styled(
            format!("  — {author}"),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            format!("  ({})", book.format),
            Style::default().fg(Color::DarkGray),
        ),
    ])
}

// Binary units, mirroring the human renderer in `catalog::render` so the TUI and
// `cdx device ls` agree on how free space reads.
fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use std::fs;
    use tempfile::tempdir;

    use crate::catalog::handlers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn type_text(state: &mut State, text: &str) {
        for ch in text.chars() {
            handle_key(state, key(KeyCode::Char(ch)));
        }
    }

    // Two known-but-disconnected devices in a fresh catalog. The device scan is
    // disabled so the suite never depends on whatever USB hardware is plugged in.
    fn setup() -> (tempfile::TempDir, Registry) {
        std::env::set_var(device::DISABLE_SCAN_ENV, "1");
        let dir = tempdir().unwrap();
        let cfg = dir.path().join("cfg");
        fs::create_dir_all(&cfg).unwrap();
        let cat = dir.path().join("lib");
        let mut reg = Registry::default();
        handlers::handle_init(&mut reg, &cfg, "main", &cat, None, false).unwrap();

        let entry = reg.resolve(None).unwrap();
        let conn = catalog::open_existing(&entry.path).unwrap();
        devices::record_seen(&conn, "AAA").unwrap();
        devices::record_seen(&conn, "BBB").unwrap();
        devices::handle_alias(&conn, "AAA", "zeta").unwrap();
        devices::handle_alias(&conn, "BBB", "alpha").unwrap();
        (dir, reg)
    }

    #[test]
    fn load_lists_known_devices() {
        let (_tmp, reg) = setup();
        let state = State::load(&reg);
        assert_eq!(state.rows.len(), 2);
        assert!(state.load_error.is_none());
        // Ordered by alias (case-insensitive): "alpha" then "zeta".
        assert_eq!(state.rows[0].alias.as_deref(), Some("alpha"));
        assert_eq!(state.rows[1].alias.as_deref(), Some("zeta"));
        assert!(state.rows.iter().all(|r| !r.connected));
    }

    #[test]
    fn load_without_current_catalog_sets_error() {
        std::env::set_var(device::DISABLE_SCAN_ENV, "1");
        let reg = Registry::default();
        let state = State::load(&reg);
        assert!(state.rows.is_empty());
        assert!(state.load_error.is_some());
    }

    #[test]
    fn down_cycles_through_rows() {
        let (_tmp, reg) = setup();
        let mut state = State::load(&reg);
        assert_eq!(state.cursor, 0);
        handle_key(&mut state, key(KeyCode::Down));
        assert_eq!(state.cursor, 1);
        handle_key(&mut state, key(KeyCode::Down));
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn r_opens_rename_seeded_with_current_alias() {
        let (_tmp, reg) = setup();
        let mut state = State::load(&reg);
        handle_key(&mut state, key(KeyCode::Char('r')));
        let rename = state.rename.as_ref().expect("r opens the rename overlay");
        // Cursor sits on the first row ("alpha").
        assert_eq!(rename.input.value(), "alpha");
    }

    #[test]
    fn rename_submit_changes_alias_and_refreshes() {
        let (_tmp, reg) = setup();
        let mut state = State::load(&reg);
        handle_key(&mut state, key(KeyCode::Char('r')));
        // Clear the seeded value, then type a fresh alias.
        for _ in 0.."alpha".len() {
            handle_key(&mut state, key(KeyCode::Backspace));
        }
        type_text(&mut state, "kitchen");
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, DevicesAction::Status(_)));
        assert!(state.rename.is_none());
        assert!(state
            .rows
            .iter()
            .any(|r| r.alias.as_deref() == Some("kitchen")));
    }

    #[test]
    fn rename_to_taken_alias_keeps_overlay_with_error() {
        let (_tmp, reg) = setup();
        let mut state = State::load(&reg);
        // Cursor on "alpha"; try to rename it to "zeta" (held by the other device).
        handle_key(&mut state, key(KeyCode::Char('r')));
        for _ in 0.."alpha".len() {
            handle_key(&mut state, key(KeyCode::Backspace));
        }
        type_text(&mut state, "zeta");
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, DevicesAction::None));
        let rename = state.rename.as_ref().expect("overlay stays open on error");
        assert!(rename.error.is_some());
    }

    #[test]
    fn rename_esc_cancels() {
        let (_tmp, reg) = setup();
        let mut state = State::load(&reg);
        handle_key(&mut state, key(KeyCode::Char('r')));
        assert!(state.rename.is_some());
        handle_key(&mut state, key(KeyCode::Esc));
        assert!(state.rename.is_none());
    }

    #[test]
    fn enter_on_disconnected_device_returns_status_error() {
        let (_tmp, reg) = setup();
        let mut state = State::load(&reg);
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, DevicesAction::Status(_)));
        assert!(matches!(state.view, View::List), "must not drill in");
    }

    #[test]
    fn esc_from_list_returns_back() {
        let (_tmp, reg) = setup();
        let mut state = State::load(&reg);
        let action = handle_key(&mut state, key(KeyCode::Esc));
        assert!(matches!(action, DevicesAction::Back));
    }

    #[test]
    fn esc_from_books_returns_to_list() {
        let (_tmp, reg) = setup();
        let mut state = State::load(&reg);
        state.view = View::Books(BooksView {
            serial: "AAA".to_string(),
            alias: "zeta".to_string(),
            rows: Vec::new(),
            cursor: 0,
        });
        let action = handle_key(&mut state, key(KeyCode::Esc));
        assert!(matches!(action, DevicesAction::None));
        assert!(matches!(state.view, View::List));
    }

    #[test]
    fn colon_returns_open_palette() {
        let (_tmp, reg) = setup();
        let mut state = State::load(&reg);
        let action = handle_key(&mut state, key(KeyCode::Char(':')));
        assert!(matches!(action, DevicesAction::OpenPalette));
    }

    #[test]
    fn captures_text_input_only_while_renaming() {
        let (_tmp, reg) = setup();
        let mut state = State::load(&reg);
        assert!(!captures_text_input(&state));
        handle_key(&mut state, key(KeyCode::Char('r')));
        assert!(captures_text_input(&state));
    }
}
