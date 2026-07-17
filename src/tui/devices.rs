use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

use crate::catalog::{self, devices, settings};
use crate::config::Registry;
use crate::device;
use crate::device::books::Presence;
use crate::device::sync::{self, Conflict, SyncItem};
use crate::device::sync_job::ApplyJob;
use crate::tui::confirm;
use crate::tui::help::{Binding, Section};
use crate::tui::widgets::{centered_rect, render_modal, StatusMessage};

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
        keys: "Space",
        desc: "mark / unmark book",
    },
    Binding {
        keys: "Enter",
        desc: "clean marked books",
    },
    Binding {
        keys: "s",
        desc: "sync with device",
    },
    Binding {
        keys: "Esc",
        desc: "back to device list",
    },
];

const BOOKS_CONFIRM_BINDINGS: &[Binding] = &[
    Binding {
        keys: "Enter",
        desc: "confirm",
    },
    Binding {
        keys: "←→ / Tab",
        desc: "switch button",
    },
    Binding {
        keys: "Esc",
        desc: "cancel",
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

const LIST_SYNC_HINT: &[Binding] = &[Binding {
    keys: "s",
    desc: "sync with device",
}];

const SYNC_BINDINGS: &[Binding] = &[
    Binding {
        keys: "↑↓ / j k",
        desc: "move selection",
    },
    Binding {
        keys: "Space",
        desc: "mark / unmark item",
    },
    Binding {
        keys: "a",
        desc: "mark / unmark all",
    },
    Binding {
        keys: "V",
        desc: "toggle full-hash verify",
    },
    Binding {
        keys: "Enter",
        desc: "apply marked items",
    },
    Binding {
        keys: "Esc",
        desc: "back to device list",
    },
];

const SYNC_RUNNING_BINDINGS: &[Binding] = &[Binding {
    keys: "Esc",
    desc: "cancel remaining",
}];

const SYNC_DONE_BINDINGS: &[Binding] = &[Binding {
    keys: "Esc / Enter",
    desc: "back to device list",
}];

pub fn help_sections(state: &State) -> Vec<Section> {
    if state.rename.is_some() {
        return vec![Section {
            title: "Rename device",
            bindings: RENAME_BINDINGS,
        }];
    }
    match &state.view {
        View::List => vec![
            Section {
                title: "Devices",
                bindings: LIST_BINDINGS,
            },
            Section {
                title: "Sync",
                bindings: LIST_SYNC_HINT,
            },
        ],
        View::Books(view) if view.confirm.is_some() => vec![Section {
            title: "Confirm clean",
            bindings: BOOKS_CONFIRM_BINDINGS,
        }],
        View::Books(_) => vec![Section {
            title: "Device books",
            bindings: BOOKS_BINDINGS,
        }],
        View::Sync(view) => match &view.job {
            Some(job) if job.is_pending() => vec![Section {
                title: "Sync running",
                bindings: SYNC_RUNNING_BINDINGS,
            }],
            Some(_) => vec![Section {
                title: "Sync done",
                bindings: SYNC_DONE_BINDINGS,
            }],
            None => vec![Section {
                title: "Sync plan",
                bindings: SYNC_BINDINGS,
            }],
        },
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
    Books(Box<BooksView>),
    Sync(Box<SyncView>),
}

#[derive(Debug)]
pub struct BooksView {
    pub serial: String,
    pub alias: String,
    pub mount: PathBuf,
    pub rows: Vec<device::books::DeviceBook>,
    pub cursor: usize,
    // Marked books keyed by `device_path` — stable across the re-sort that
    // `device::books::list` applies after a clean (positional indices are not).
    pub selected: BTreeSet<PathBuf>,
    pub confirm: Option<confirm::State>,
}

// The sync plan rendered as a checkbox list. The plan only ever holds items that
// need action (pushes/pulls/modified/missing) — books already in sync never appear.
#[derive(Debug)]
pub struct SyncView {
    pub serial: String,
    pub alias: String,
    pub mount: PathBuf,
    pub verify: bool,
    pub items: Vec<SyncItem>,
    pub conflicts: Vec<Conflict>,
    pub cursor: usize,
    // Marked item indices. The plan is computed once and never re-sorted while the
    // user marks, so positional indices are stable (unlike the clean flow's paths);
    // a not-on-device push carries an empty `device_path`, so paths aren't unique.
    pub selected: BTreeSet<usize>,
    // `Some` once applying starts; the loop polls it one item per tick.
    pub job: Option<Box<ApplyJob>>,
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
    device::mark_current(&conn, &mut rows);
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
    match state.view {
        View::Books(_) => handle_books_key(state, key),
        View::Sync(_) => handle_sync_key(state, key),
        View::List => handle_list_key(state, key),
    }
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
        KeyCode::Char('s') => open_sync(state),
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
    // Opening a device's books makes it the current device — the same "last used"
    // pointer the CLI maintains, so the Library presence indicators follow along.
    if let Err(error) = settings::save_current_device(&conn, &row.serial) {
        tracing::warn!(serial = %row.serial, %error, "failed to persist current device");
    }
    match device::books::list(&conn, &row.serial, &mount) {
        Ok(rows) => {
            state.view = View::Books(Box::new(BooksView {
                serial: row.serial,
                alias: label,
                mount,
                rows,
                cursor: 0,
                selected: BTreeSet::new(),
                confirm: None,
            }));
            DevicesAction::None
        }
        Err(err) => DevicesAction::Status(StatusMessage::error(err.to_string())),
    }
}

// Entry point for the Library screen: resolve the current/sole connected device
// (the same `--device`-implicit pointer the CLI and the `p` push action use) and
// open its sync plan. A non-empty plan leaves `state` in `View::Sync`; otherwise a
// Status is returned (no device, ambiguous, or already in sync) and the view is
// untouched, so the caller can stay where it is.
pub fn open_sync_current(state: &mut State) -> DevicesAction {
    let Some(dir) = state.catalog.as_ref().map(|c| c.dir.clone()) else {
        return DevicesAction::Status(StatusMessage::error("no catalog selected"));
    };
    let conn = match catalog::open_existing(&dir) {
        Ok(conn) => conn,
        Err(err) => return DevicesAction::Status(StatusMessage::error(err.to_string())),
    };
    let detected = device::detect();
    let dev = match device::resolve_target(&conn, &detected, None) {
        Ok(dev) => dev,
        Err(err) => return DevicesAction::Status(StatusMessage::error(err.to_string())),
    };
    let alias = alias_for(&conn, &dev.serial);
    drop(conn);
    open_sync_for(state, &dev.serial, &alias, &dev.mount_path)
}

// Alias when one is set, else the serial — mirrors the badge label elsewhere.
fn alias_for(conn: &rusqlite::Connection, serial: &str) -> String {
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

fn open_sync(state: &mut State) -> DevicesAction {
    let Some(row) = state.rows.get(state.cursor).cloned() else {
        return DevicesAction::None;
    };
    let label = row.alias.clone().unwrap_or_else(|| row.serial.clone());
    let Some(mount) = row.mount_path.clone().filter(|_| row.connected) else {
        return DevicesAction::Status(StatusMessage::error(format!(
            "device `{label}` is not connected"
        )));
    };
    open_sync_for(state, &row.serial, &label, &mount)
}

// Compute the plan for a specific device and drill into the sync view. Shared by
// the device list (`s` on a row), the per-device books view (`s`), and the Library
// screen (which resolves the current device first, then hands off here).
fn open_sync_for(state: &mut State, serial: &str, alias: &str, mount: &Path) -> DevicesAction {
    let Some(dir) = state.catalog.as_ref().map(|c| c.dir.clone()) else {
        return DevicesAction::None;
    };
    let conn = match catalog::open_existing(&dir) {
        Ok(conn) => conn,
        Err(err) => return DevicesAction::Status(StatusMessage::error(err.to_string())),
    };
    // Syncing makes this the current device — the same "last used" pointer the CLI
    // maintains, so Library presence indicators follow along.
    if let Err(error) = settings::save_current_device(&conn, serial) {
        tracing::warn!(%serial, %error, "failed to persist current device");
    }
    let plan = match sync::diff(&conn, serial, mount, false) {
        Ok(plan) => plan,
        Err(err) => return DevicesAction::Status(StatusMessage::error(err.to_string())),
    };
    if plan.is_empty() {
        return DevicesAction::Status(StatusMessage::info(format!("`{alias}` is already in sync")));
    }
    let selected = (0..plan.items.len()).collect();
    state.view = View::Sync(Box::new(SyncView {
        serial: serial.to_string(),
        alias: alias.to_string(),
        mount: mount.to_path_buf(),
        verify: false,
        items: plan.items,
        conflicts: plan.conflicts,
        cursor: 0,
        selected,
        job: None,
    }));
    DevicesAction::None
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
    // While the delete confirmation is open every key drives the dialog — this
    // also swallows `:`/Space/j/k so the palette can't open over a live confirm.
    if matches!(&state.view, View::Books(view) if view.confirm.is_some()) {
        return handle_books_confirm_key(state, key);
    }
    // `s` starts a sync for the device being viewed. Handle it via the read-only
    // borrow so the mutable navigation borrow below stays exclusive.
    if matches!(key.code, KeyCode::Char('s')) {
        if let View::Books(view) = &state.view {
            let (serial, alias, mount) =
                (view.serial.clone(), view.alias.clone(), view.mount.clone());
            return open_sync_for(state, &serial, &alias, &mount);
        }
    }
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
        KeyCode::Char(' ') => {
            if let Some(book) = view.rows.get(view.cursor) {
                let path = book.device_path.clone();
                if !view.selected.remove(&path) {
                    view.selected.insert(path);
                }
            }
            DevicesAction::None
        }
        KeyCode::Enter => {
            if view.selected.is_empty() {
                return DevicesAction::Status(StatusMessage::info(
                    "no books marked — press Space to mark",
                ));
            }
            view.confirm = Some(confirm::State {
                title: "delete from device".to_string(),
                message: format!(
                    "delete {} book(s) from {}?",
                    view.selected.len(),
                    view.alias
                ),
                ok_label: "[ Delete ]".to_string(),
                cancel_label: "[ Cancel ]".to_string(),
                // Destructive: default focus to Cancel so a bare Enter aborts.
                focus: confirm::Button::Cancel,
            });
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

fn handle_books_confirm_key(state: &mut State, key: KeyEvent) -> DevicesAction {
    let View::Books(view) = &mut state.view else {
        return DevicesAction::None;
    };
    let Some(dialog) = view.confirm.as_mut() else {
        return DevicesAction::None;
    };
    match confirm::handle_key(dialog, key) {
        confirm::ConfirmAction::None => DevicesAction::None,
        confirm::ConfirmAction::Cancel => {
            view.confirm = None;
            DevicesAction::None
        }
        confirm::ConfirmAction::Confirm => apply_clean(state),
    }
}

fn apply_clean(state: &mut State) -> DevicesAction {
    let View::Books(view) = &mut state.view else {
        return DevicesAction::None;
    };
    // Pull everything we need out of the view before borrowing `state.catalog`.
    let serial = view.serial.clone();
    let mount = view.mount.clone();
    let device_paths: Vec<PathBuf> = view.selected.iter().cloned().collect();
    view.confirm = None;

    let Some(dir) = state.catalog.as_ref().map(|c| c.dir.clone()) else {
        return DevicesAction::None;
    };
    let conn = match catalog::open_existing(&dir) {
        Ok(conn) => conn,
        Err(err) => return DevicesAction::Status(StatusMessage::error(err.to_string())),
    };
    let outcome = match device::clean::clean(&conn, &serial, &mount, &device_paths) {
        Ok(outcome) => outcome,
        Err(err) => return DevicesAction::Status(StatusMessage::error(err.to_string())),
    };
    let rows = match device::books::list(&conn, &serial, &mount) {
        Ok(rows) => rows,
        Err(err) => return DevicesAction::Status(StatusMessage::error(err.to_string())),
    };
    drop(conn);

    if let View::Books(view) = &mut state.view {
        view.rows = rows;
        view.selected.clear();
        view.cursor = view.cursor.min(view.rows.len().saturating_sub(1));
    }
    DevicesAction::Status(StatusMessage::info(format!(
        "removed {} books, freed {}",
        outcome.removed.len(),
        format_bytes(outcome.total_bytes)
    )))
}

fn handle_sync_key(state: &mut State, key: KeyEvent) -> DevicesAction {
    let View::Sync(view) = &mut state.view else {
        return DevicesAction::None;
    };
    // Once applying starts, every key drives the progress overlay (mirrors the embed
    // job): Esc cancels the remainder while running, or closes when done.
    if let Some(job) = view.job.as_mut() {
        match key.code {
            KeyCode::Esc if job.is_pending() => {
                job.queue.clear();
                job.current = None;
                job.done = true;
                DevicesAction::None
            }
            KeyCode::Esc | KeyCode::Enter => close_sync(state),
            _ => DevicesAction::None,
        }
    } else {
        handle_sync_plan_key(state, key)
    }
}

fn handle_sync_plan_key(state: &mut State, key: KeyEvent) -> DevicesAction {
    let View::Sync(view) = &mut state.view else {
        return DevicesAction::None;
    };
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            if !view.items.is_empty() {
                view.cursor = (view.cursor + 1) % view.items.len();
            }
            DevicesAction::None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if !view.items.is_empty() {
                view.cursor = (view.cursor + view.items.len() - 1) % view.items.len();
            }
            DevicesAction::None
        }
        KeyCode::Char(' ') => {
            if !view.selected.remove(&view.cursor) {
                view.selected.insert(view.cursor);
            }
            DevicesAction::None
        }
        KeyCode::Char('a') => {
            // Toggle-all: clear if everything is already marked, else mark all.
            if view.selected.len() == view.items.len() {
                view.selected.clear();
            } else {
                view.selected = (0..view.items.len()).collect();
            }
            DevicesAction::None
        }
        KeyCode::Char('V') => recompute_verify(state),
        KeyCode::Enter => start_sync(state),
        KeyCode::Esc => {
            state.view = View::List;
            DevicesAction::None
        }
        KeyCode::Char(':') => DevicesAction::OpenPalette,
        _ => DevicesAction::None,
    }
}

fn recompute_verify(state: &mut State) -> DevicesAction {
    let View::Sync(view) = &mut state.view else {
        return DevicesAction::None;
    };
    let serial = view.serial.clone();
    let mount = view.mount.clone();
    let Some(dir) = state.catalog.as_ref().map(|c| c.dir.clone()) else {
        return DevicesAction::None;
    };
    let conn = match catalog::open_existing(&dir) {
        Ok(conn) => conn,
        Err(err) => return DevicesAction::Status(StatusMessage::error(err.to_string())),
    };
    let plan = match sync::diff(&conn, &serial, &mount, true) {
        Ok(plan) => plan,
        Err(err) => return DevicesAction::Status(StatusMessage::error(err.to_string())),
    };
    let count = plan.items.len();
    if let View::Sync(view) = &mut state.view {
        view.verify = true;
        view.items = plan.items;
        view.conflicts = plan.conflicts;
        view.cursor = 0;
        view.selected = (0..count).collect();
    }
    DevicesAction::Status(StatusMessage::info(format!(
        "verified plan: {count} item(s)"
    )))
}

fn start_sync(state: &mut State) -> DevicesAction {
    let View::Sync(view) = &mut state.view else {
        return DevicesAction::None;
    };
    if view.selected.is_empty() {
        return DevicesAction::Status(StatusMessage::info("nothing marked — press Space to mark"));
    }
    let Some(dir) = state.catalog.as_ref().map(|c| c.dir.clone()) else {
        return DevicesAction::None;
    };
    let marked: Vec<SyncItem> = view
        .items
        .iter()
        .enumerate()
        .filter(|(i, _)| view.selected.contains(i))
        .map(|(_, item)| item.clone())
        .collect();
    view.job = Some(Box::new(ApplyJob::new(
        marked,
        &view.serial,
        &view.mount,
        &dir,
    )));
    DevicesAction::None
}

fn close_sync(state: &mut State) -> DevicesAction {
    // Sync changed device contents; rebuild the list so counts/presence are fresh.
    state.view = View::List;
    state.refresh();
    DevicesAction::None
}

pub fn has_pending_sync(state: &State) -> bool {
    matches!(&state.view, View::Sync(view) if view.job.as_ref().is_some_and(|j| j.is_pending()))
}

pub fn advance_sync(state: &mut State) {
    let Some(dir) = state.catalog.as_ref().map(|c| c.dir.clone()) else {
        return;
    };
    let View::Sync(view) = &mut state.view else {
        return;
    };
    let Some(job) = view.job.as_mut() else {
        return;
    };
    let Ok(mut conn) = catalog::open_existing(&dir) else {
        return;
    };
    job.advance(&mut conn);
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &State) {
    match &state.view {
        View::List => render_list(frame, area, state),
        View::Books(view) => render_books(frame, area, view),
        View::Sync(view) => render_sync(frame, area, view),
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

    let title = if view.selected.is_empty() {
        format!("Devices › {}", view.alias)
    } else {
        format!(
            "Devices › {}  ({} selected)",
            view.alias,
            view.selected.len()
        )
    };
    let header = Paragraph::new(Line::from(Span::styled(
        title,
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
        .map(|book| {
            ListItem::new(book_row_line(
                book,
                view.selected.contains(&book.device_path),
            ))
        })
        .collect();
    let list = List::new(items).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    let mut list_state = ListState::default();
    list_state.select(Some(view.cursor.min(view.rows.len() - 1)));
    frame.render_stateful_widget(list, layout[1], &mut list_state);

    if let Some(dialog) = &view.confirm {
        confirm::render(frame, area, dialog);
    }
}

fn render_sync(frame: &mut Frame<'_>, area: Rect, view: &SyncView) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    let marked = view.selected.len();
    let verify = if view.verify {
        "verify on"
    } else {
        "verify off"
    };
    let title = format!(
        "Devices › {} — sync  ({}/{} marked · {})",
        view.alias,
        marked,
        view.items.len(),
        verify
    );
    let header = Paragraph::new(Line::from(Span::styled(
        title,
        Style::default().add_modifier(Modifier::BOLD),
    )));
    frame.render_widget(header, layout[0]);

    if let Some(job) = &view.job {
        render_sync_progress(frame, layout[1], job);
        return;
    }

    // Reserve a block at the bottom for conflicts when there are any.
    let body = if view.conflicts.is_empty() {
        layout[1]
    } else {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(view.conflicts.len() as u16 + 1),
            ])
            .split(layout[1]);
        render_conflicts(frame, split[1], &view.conflicts);
        split[0]
    };

    let items: Vec<ListItem> = view
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| ListItem::new(sync_item_line(item, view.selected.contains(&i))))
        .collect();
    let list = List::new(items).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    let mut list_state = ListState::default();
    if !view.items.is_empty() {
        list_state.select(Some(view.cursor.min(view.items.len() - 1)));
    }
    frame.render_stateful_widget(list, body, &mut list_state);
}

fn sync_item_line(item: &SyncItem, selected: bool) -> Line<'static> {
    let check = if selected { "[x]" } else { "[ ]" };
    let (arrow, tag, tag_style) = match item.direction {
        sync::Direction::Pull => ("←", "pull".to_string(), Style::default().fg(Color::Cyan)),
        sync::Direction::Push => {
            let (label, color) = match item.push_reason {
                Some(sync::PushReason::NotOnDevice) | None => ("new", Color::Green),
                Some(sync::PushReason::Modified) => ("modified", Color::Yellow),
                Some(sync::PushReason::Missing) => ("missing", Color::Red),
            };
            ("→", label.to_string(), Style::default().fg(color))
        }
    };
    Line::from(vec![
        Span::raw(" "),
        Span::styled(
            check.to_string(),
            if selected {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ),
        Span::raw(" "),
        Span::styled(arrow.to_string(), Style::default().fg(Color::DarkGray)),
        Span::raw(" "),
        Span::styled(format!("[{tag}]"), tag_style),
        Span::raw("  "),
        Span::raw(item.title.clone()),
    ])
}

fn render_conflicts(frame: &mut Frame<'_>, area: Rect, conflicts: &[Conflict]) {
    let mut lines = vec![Line::from(Span::styled(
        format!("conflicts ({}) — resolve manually:", conflicts.len()),
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    ))];
    for c in conflicts {
        let ids = c
            .candidates
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(Line::from(Span::styled(
            format!("  {} — matches ids {ids}", c.title),
            Style::default().fg(Color::DarkGray),
        )));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn render_sync_progress(frame: &mut Frame<'_>, area: Rect, job: &ApplyJob) {
    let target_w = area.width.saturating_mul(4) / 5;
    let target_h = area.height.saturating_mul(4) / 5;
    let w = target_w.max(50).min(area.width);
    let h = target_h.max(12).min(area.height);
    let rect = centered_rect(w, h, area);
    frame.render_widget(Clear, rect);

    let title = if job.done {
        " sync: done "
    } else {
        " sync: running "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // current / status
            Constraint::Length(1), // counters
            Constraint::Length(1), // gauge
            Constraint::Length(1), // spacer
            Constraint::Length(1), // failures header
            Constraint::Min(1),    // failures list
            Constraint::Length(1), // hint
        ])
        .split(inner);

    let status_line = match (&job.current, job.done) {
        (_, true) => Line::from(vec![
            Span::styled(
                "Done — ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                "{} pushed, {} pulled, {} failed",
                job.pushed,
                job.pulled,
                job.failures.len()
            )),
        ]),
        (Some(item), false) => Line::from(vec![
            Span::styled("Up next: ", Style::default().fg(Color::Cyan)),
            Span::raw(item.title.clone()),
        ]),
        (None, false) => Line::from(Span::styled(
            "Working…",
            Style::default().fg(Color::DarkGray),
        )),
    };
    frame.render_widget(Paragraph::new(status_line), layout[0]);

    let counters = Paragraph::new(Line::from(Span::styled(
        format!("{} / {}", job.completed, job.total),
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(counters, layout[1]);

    let ratio = if job.total == 0 {
        1.0
    } else {
        job.completed as f64 / job.total as f64
    };
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(Color::Cyan).bg(Color::Black))
        .ratio(ratio.clamp(0.0, 1.0));
    frame.render_widget(gauge, layout[2]);

    let failures_label = if job.failures.is_empty() {
        "Failures: none"
    } else {
        "Failures:"
    };
    let label_style = if job.failures.is_empty() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(failures_label, label_style))),
        layout[4],
    );

    if !job.failures.is_empty() {
        let items: Vec<ListItem<'_>> = job
            .failures
            .iter()
            .map(|f| {
                ListItem::new(Line::from(vec![
                    Span::styled(format!("· {}", f.title), Style::default().fg(Color::White)),
                    Span::styled(
                        format!("  — {}", f.reason),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]))
            })
            .collect();
        frame.render_widget(List::new(items), layout[5]);
    }

    let hint = if job.done {
        "Esc/Enter close"
    } else {
        "Esc cancel"
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        ))),
        layout[6],
    );
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
    ];
    if row.is_current {
        spans.push(Span::styled(
            "  (current)".to_string(),
            Style::default().fg(Color::Cyan),
        ));
    }
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        row.serial.clone(),
        Style::default().fg(Color::DarkGray),
    ));
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

fn book_row_line(book: &device::books::DeviceBook, selected: bool) -> Line<'static> {
    let (tag, tag_style) = match book.presence {
        Presence::Both => ("both", Style::default().fg(Color::Green)),
        Presence::Modified => ("modified", Style::default().fg(Color::Yellow)),
        // `LocalOnly` never reaches a device listing; render it like a device-only
        // file rather than panicking on the exhaustive match.
        Presence::DeviceOnly | Presence::LocalOnly => {
            ("device only", Style::default().fg(Color::DarkGray))
        }
        Presence::Conflict => ("conflict", Style::default().fg(Color::Red)),
    };
    let title = book.title.clone().unwrap_or_else(|| "-".to_string());
    let author = book.author.clone().unwrap_or_else(|| "-".to_string());
    let check = if selected { "[x]" } else { "[ ]" };
    Line::from(vec![
        Span::raw(" "),
        Span::styled(
            check.to_string(),
            if selected {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ),
        Span::raw(" "),
        Span::styled(format!("[{tag}]"), tag_style),
        Span::raw("  "),
        Span::raw(title),
        Span::styled(
            format!("  — {author}"),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            format!(
                "  ({})",
                format_pair(&book.format, book.local_format.as_deref())
            ),
            Style::default().fg(Color::DarkGray),
        ),
    ])
}

// "epub", or "azw3 → epub" when the device and catalog formats differ — the
// device format leads since this is the device view.
fn format_pair(device_format: &str, local_format: Option<&str>) -> String {
    match local_format {
        Some(local) if !local.eq_ignore_ascii_case(device_format) => {
            format!("{device_format} → {local}")
        }
        _ => device_format.to_string(),
    }
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
    use rusqlite::params;
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
    fn open_books_persists_current_device() {
        let (_tmp, reg) = setup();
        let mount = tempdir().unwrap();
        fs::create_dir_all(mount.path().join("documents")).unwrap();
        let mut state = State::load(&reg);
        // Point the cursor at a connected device backed by a real temp mount.
        state.rows = vec![device::DeviceRow {
            alias: Some("zeta".to_string()),
            serial: "AAA".to_string(),
            connected: true,
            mount_path: Some(mount.path().to_path_buf()),
            free_bytes: None,
            book_count: None,
            last_seen_at: "2026-06-08 12:00:00".to_string(),
            is_current: false,
        }];
        state.cursor = 0;

        handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(state.view, View::Books(_)));

        let entry = reg.resolve(None).unwrap();
        let conn = catalog::open_existing(&entry.path).unwrap();
        assert_eq!(
            settings::load_current_device(&conn).unwrap().as_deref(),
            Some("AAA")
        );
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
        state.view = View::Books(Box::new(BooksView {
            serial: "AAA".to_string(),
            alias: "zeta".to_string(),
            mount: PathBuf::from("/nonexistent"),
            rows: Vec::new(),
            cursor: 0,
            selected: BTreeSet::new(),
            confirm: None,
        }));
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

    // Build a Devices state sitting on a books view backed by a real temp mount
    // with two synced epubs. `setup()` only makes disconnected devices and the
    // USB scan is disabled, so we construct the `View::Books` directly (as the
    // `esc_from_books_*` test does) after seeding the catalog + device files.
    fn books_view(mount: &Path, reg: &Registry) -> State {
        let entry = reg.resolve(None).unwrap();
        let conn = catalog::open_existing(&entry.path).unwrap();
        let docs = mount.join("documents");
        fs::create_dir_all(&docs).unwrap();
        devices::record_seen(&conn, "AAA").unwrap();
        for (i, name) in ["Dune.epub", "Hyperion.epub"].iter().enumerate() {
            fs::write(docs.join(name), b"book bytes").unwrap();
            conn.execute(
                "INSERT INTO books (title, author, format, file_path) VALUES (?1, 'A', 'epub', '')",
                params![format!("title {i}")],
            )
            .unwrap();
            let book_id = conn.last_insert_rowid();
            devices::record_sync(
                &conn,
                "AAA",
                book_id,
                &PathBuf::from("documents").join(name),
                "h",
                9,
                1,
            )
            .unwrap();
        }
        let rows = device::books::list(&conn, "AAA", mount).unwrap();
        drop(conn);

        let mut state = State::load(reg);
        state.view = View::Books(Box::new(BooksView {
            serial: "AAA".to_string(),
            alias: "zeta".to_string(),
            mount: mount.to_path_buf(),
            rows,
            cursor: 0,
            selected: BTreeSet::new(),
            confirm: None,
        }));
        state
    }

    fn books(state: &State) -> &BooksView {
        match &state.view {
            View::Books(v) => v,
            _ => panic!("expected a books view"),
        }
    }

    #[test]
    fn space_toggles_selection() {
        let (_tmp, reg) = setup();
        let mount = tempdir().unwrap();
        let mut state = books_view(mount.path(), &reg);
        let path = books(&state).rows[0].device_path.clone();

        handle_key(&mut state, key(KeyCode::Char(' ')));
        assert!(books(&state).selected.contains(&path));
        handle_key(&mut state, key(KeyCode::Char(' ')));
        assert!(books(&state).selected.is_empty());
    }

    #[test]
    fn enter_with_empty_selection_is_noop_status() {
        let (_tmp, reg) = setup();
        let mount = tempdir().unwrap();
        let mut state = books_view(mount.path(), &reg);

        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, DevicesAction::Status(_)));
        assert!(books(&state).confirm.is_none());
        assert_eq!(books(&state).rows.len(), 2, "no books removed");
    }

    #[test]
    fn enter_with_selection_opens_confirm() {
        let (_tmp, reg) = setup();
        let mount = tempdir().unwrap();
        let mut state = books_view(mount.path(), &reg);

        handle_key(&mut state, key(KeyCode::Char(' ')));
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, DevicesAction::None));
        let dialog = books(&state).confirm.as_ref().expect("confirm opens");
        assert_eq!(dialog.focus, confirm::Button::Cancel);
    }

    #[test]
    fn confirm_cancel_dismisses_without_deleting() {
        let (_tmp, reg) = setup();
        let mount = tempdir().unwrap();
        let mut state = books_view(mount.path(), &reg);

        handle_key(&mut state, key(KeyCode::Char(' ')));
        handle_key(&mut state, key(KeyCode::Enter));
        handle_key(&mut state, key(KeyCode::Esc));

        assert!(books(&state).confirm.is_none());
        assert!(mount.path().join("documents/Dune.epub").exists());
        assert!(
            !books(&state).selected.is_empty(),
            "cancel keeps the marks so the user can retry"
        );
    }

    #[test]
    fn confirm_delete_removes_file_and_sync_row_and_refreshes() {
        let (_tmp, reg) = setup();
        let mount = tempdir().unwrap();
        let mut state = books_view(mount.path(), &reg);
        let target = books(&state).rows[0].device_path.clone();

        handle_key(&mut state, key(KeyCode::Char(' ')));
        handle_key(&mut state, key(KeyCode::Enter));
        // Default focus is Cancel; Tab to Delete, then confirm.
        handle_key(&mut state, key(KeyCode::Tab));
        let action = handle_key(&mut state, key(KeyCode::Enter));

        assert!(matches!(action, DevicesAction::Status(_)));
        let view = books(&state);
        assert!(view.confirm.is_none());
        assert!(view.selected.is_empty());
        assert_eq!(view.rows.len(), 1, "the cleaned book is gone from the list");
        assert!(view.cursor < view.rows.len());
        assert!(!mount.path().join(&target).exists());

        let entry = reg.resolve(None).unwrap();
        let conn = catalog::open_existing(&entry.path).unwrap();
        let synced = devices::synced_paths(&conn, "AAA").unwrap();
        assert!(!synced.contains_key(&target), "sync row cleared");
    }

    #[test]
    fn confirm_delete_reports_freed_bytes() {
        let (_tmp, reg) = setup();
        let mount = tempdir().unwrap();
        let mut state = books_view(mount.path(), &reg);

        handle_key(&mut state, key(KeyCode::Char(' ')));
        handle_key(&mut state, key(KeyCode::Enter));
        handle_key(&mut state, key(KeyCode::Tab));
        let action = handle_key(&mut state, key(KeyCode::Enter));

        let DevicesAction::Status(status) = action else {
            panic!("expected a status message");
        };
        // Each seeded file holds `b"book bytes"` (10 bytes).
        assert!(status.text.contains("removed 1 books"));
        assert!(status.text.contains("10 B"));
    }

    fn line_text(line: &Line<'static>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn book_row_line_shows_modified_tag() {
        let book = device::books::DeviceBook {
            title: Some("Dune".to_string()),
            author: Some("Herbert".to_string()),
            format: "epub".to_string(),
            device_path: PathBuf::from("documents/Dune.epub"),
            presence: Presence::Modified,
            matched_book_id: Some(1),
            matched_title: Some("Dune".to_string()),
            local_format: None,
        };
        assert!(line_text(&book_row_line(&book, false)).contains("[modified]"));
    }

    #[test]
    fn book_row_line_shows_format_pair_when_formats_differ() {
        let book = device::books::DeviceBook {
            title: Some("Dune".to_string()),
            author: None,
            format: "azw3".to_string(),
            device_path: PathBuf::from("documents/Dune.azw3"),
            presence: Presence::Both,
            matched_book_id: Some(1),
            matched_title: Some("Dune".to_string()),
            local_format: Some("epub".to_string()),
        };
        assert!(line_text(&book_row_line(&book, false)).contains("(azw3 → epub)"));
    }

    #[test]
    fn colon_swallowed_while_confirm_open() {
        let (_tmp, reg) = setup();
        let mount = tempdir().unwrap();
        let mut state = books_view(mount.path(), &reg);

        handle_key(&mut state, key(KeyCode::Char(' ')));
        handle_key(&mut state, key(KeyCode::Enter));
        let action = handle_key(&mut state, key(KeyCode::Char(':')));
        assert!(matches!(action, DevicesAction::None));
        assert!(books(&state).confirm.is_some());
    }

    // --- Sync flow ---------------------------------------------------------

    fn push_item(id: i64, title: &str) -> SyncItem {
        SyncItem {
            direction: sync::Direction::Push,
            book_id: Some(id),
            title: title.to_string(),
            device_path: PathBuf::new(),
            push_reason: Some(sync::PushReason::NotOnDevice),
            bytes: None,
        }
    }

    // A Devices state sitting on a synthetic sync plan (no real device needed) for
    // the pure key-handling tests. All items start marked, mirroring `open_sync`.
    fn sync_state(reg: &Registry, items: Vec<SyncItem>, conflicts: Vec<Conflict>) -> State {
        let selected = (0..items.len()).collect();
        let mut state = State::load(reg);
        state.view = View::Sync(Box::new(SyncView {
            serial: "AAA".to_string(),
            alias: "zeta".to_string(),
            mount: PathBuf::from("/nonexistent"),
            verify: false,
            items,
            conflicts,
            cursor: 0,
            selected,
            job: None,
        }));
        state
    }

    fn sync(state: &State) -> &SyncView {
        match &state.view {
            View::Sync(v) => v,
            _ => panic!("expected a sync view"),
        }
    }

    // A catalog holding one local-only book (real stored file) plus a connected
    // device backed by a temp mount, with the cursor parked on that device.
    fn connected_with_local_book() -> (tempfile::TempDir, Registry, tempfile::TempDir, State) {
        let (tmp, reg) = setup();
        let entry = reg.resolve(None).unwrap();
        let cat = entry.path.clone();
        let conn = catalog::open_existing(&cat).unwrap();
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES ('Dune', 'A', 'epub', '')",
            [],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        let rel = format!("books/{id}/Dune.epub");
        let abs = cat.join(&rel);
        fs::create_dir_all(abs.parent().unwrap()).unwrap();
        fs::write(&abs, b"book bytes").unwrap();
        conn.execute(
            "UPDATE books SET file_path = ?1 WHERE id = ?2",
            params![rel, id],
        )
        .unwrap();
        drop(conn);

        let mount = tempdir().unwrap();
        fs::create_dir_all(mount.path().join("documents")).unwrap();
        let mut state = State::load(&reg);
        state.rows = vec![device::DeviceRow {
            alias: Some("zeta".to_string()),
            serial: "AAA".to_string(),
            connected: true,
            mount_path: Some(mount.path().to_path_buf()),
            free_bytes: None,
            book_count: None,
            last_seen_at: "2026-06-08 12:00:00".to_string(),
            is_current: false,
        }];
        state.cursor = 0;
        (tmp, reg, mount, state)
    }

    #[test]
    fn space_toggles_sync_item() {
        let (_tmp, reg) = setup();
        let mut state = sync_state(&reg, vec![push_item(1, "Dune")], Vec::new());
        // Starts marked; Space unmarks, Space re-marks.
        handle_key(&mut state, key(KeyCode::Char(' ')));
        assert!(sync(&state).selected.is_empty());
        handle_key(&mut state, key(KeyCode::Char(' ')));
        assert!(sync(&state).selected.contains(&0));
    }

    #[test]
    fn a_toggles_all_marks() {
        let (_tmp, reg) = setup();
        let mut state = sync_state(&reg, vec![push_item(1, "A"), push_item(2, "B")], Vec::new());
        // All marked → `a` clears.
        handle_key(&mut state, key(KeyCode::Char('a')));
        assert!(sync(&state).selected.is_empty());
        // None marked → `a` marks all.
        handle_key(&mut state, key(KeyCode::Char('a')));
        assert_eq!(sync(&state).selected.len(), 2);
    }

    #[test]
    fn enter_with_nothing_marked_is_noop_status() {
        let (_tmp, reg) = setup();
        let mut state = sync_state(&reg, vec![push_item(1, "Dune")], Vec::new());
        handle_key(&mut state, key(KeyCode::Char(' '))); // unmark the only item
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, DevicesAction::Status(_)));
        assert!(sync(&state).job.is_none(), "no job started");
    }

    #[test]
    fn enter_starts_the_job() {
        let (_tmp, reg) = setup();
        let mut state = sync_state(&reg, vec![push_item(1, "Dune")], Vec::new());
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, DevicesAction::None));
        let job = sync(&state).job.as_ref().expect("Enter starts a job");
        assert!(job.is_pending());
        assert_eq!(job.total, 1);
    }

    #[test]
    fn esc_from_plan_returns_to_list() {
        let (_tmp, reg) = setup();
        let mut state = sync_state(&reg, vec![push_item(1, "Dune")], Vec::new());
        handle_key(&mut state, key(KeyCode::Esc));
        assert!(matches!(state.view, View::List));
    }

    #[test]
    fn conflicts_are_not_part_of_selection() {
        let (_tmp, reg) = setup();
        let conflicts = vec![Conflict {
            device_path: PathBuf::from("documents/Ambiguous.epub"),
            title: "Ambiguous".to_string(),
            candidates: vec![1, 2],
        }];
        let mut state = sync_state(&reg, vec![push_item(1, "Dune")], conflicts);
        // `a` (mark all) only ever covers the actionable items.
        handle_key(&mut state, key(KeyCode::Char('a'))); // clear
        handle_key(&mut state, key(KeyCode::Char('a'))); // mark all
        assert_eq!(sync(&state).selected.len(), 1);
    }

    #[test]
    fn s_opens_sync_plan_with_all_items_marked() {
        let (_tmp, _reg, _mount, mut state) = connected_with_local_book();
        let action = handle_key(&mut state, key(KeyCode::Char('s')));
        assert!(matches!(action, DevicesAction::None));
        let view = sync(&state);
        assert_eq!(view.items.len(), 1, "the local-only book becomes a push");
        assert_eq!(view.items[0].direction, sync::Direction::Push);
        assert_eq!(view.selected.len(), 1, "everything starts marked");
        assert!(!view.verify);
    }

    #[test]
    fn open_sync_current_without_a_connected_device_returns_status() {
        // The USB scan is disabled in tests, so no device resolves — the Library
        // entry point must surface a Status and leave the view untouched.
        let (_tmp, reg) = setup();
        let mut state = State::load(&reg);
        let action = open_sync_current(&mut state);
        assert!(matches!(action, DevicesAction::Status(_)));
        assert!(matches!(state.view, View::List));
    }

    #[test]
    fn s_from_books_view_opens_sync_plan() {
        let (_tmp, _reg, _mount, mut state) = connected_with_local_book();
        // Drill into the device's books view (the empty mount lists no books yet).
        handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(state.view, View::Books(_)));
        // `s` from the books view computes the plan for that same device.
        let action = handle_key(&mut state, key(KeyCode::Char('s')));
        assert!(matches!(action, DevicesAction::None));
        let view = sync(&state);
        assert_eq!(view.items.len(), 1, "the local-only book becomes a push");
        assert_eq!(view.alias, "zeta");
    }

    #[test]
    fn s_on_empty_plan_stays_on_list() {
        let (_tmp, reg, mount, mut state) = connected_with_local_book();
        // Push the only book first so the catalog and device are already in sync.
        let entry = reg.resolve(None).unwrap();
        let conn = catalog::open_existing(&entry.path).unwrap();
        device::push::push(&conn, &entry.path, "AAA", mount.path(), "Dune").unwrap();
        drop(conn);

        let action = handle_key(&mut state, key(KeyCode::Char('s')));
        assert!(matches!(action, DevicesAction::Status(_)));
        assert!(matches!(state.view, View::List), "no plan → no drill-in");
    }

    #[test]
    fn advance_sync_applies_the_marked_push_to_completion() {
        let (_tmp, _reg, mount, mut state) = connected_with_local_book();
        handle_key(&mut state, key(KeyCode::Char('s')));
        handle_key(&mut state, key(KeyCode::Enter)); // start the job

        assert!(has_pending_sync(&state));
        advance_sync(&mut state);
        assert!(
            !has_pending_sync(&state),
            "single item completes in one tick"
        );

        let view = sync(&state);
        let job = view.job.as_ref().unwrap();
        assert!(job.done);
        assert_eq!(job.pushed, 1);
        assert!(job.failures.is_empty());
        assert!(mount.path().join("documents/Dune.epub").is_file());
    }

    #[test]
    fn esc_cancels_a_running_job() {
        let (_tmp, reg) = setup();
        let mut state = sync_state(&reg, vec![push_item(1, "A"), push_item(2, "B")], Vec::new());
        handle_key(&mut state, key(KeyCode::Enter)); // start
        assert!(has_pending_sync(&state));
        // Esc cancels the remainder without copying anything (job never advanced).
        handle_key(&mut state, key(KeyCode::Esc));
        let job = sync(&state).job.as_ref().unwrap();
        assert!(job.done);
        assert!(!has_pending_sync(&state));
    }

    #[test]
    fn esc_on_done_job_returns_to_list() {
        let (_tmp, _reg, _mount, mut state) = connected_with_local_book();
        handle_key(&mut state, key(KeyCode::Char('s')));
        handle_key(&mut state, key(KeyCode::Enter));
        advance_sync(&mut state); // job done
        handle_key(&mut state, key(KeyCode::Esc));
        assert!(matches!(state.view, View::List));
    }

    #[test]
    fn capital_v_recomputes_plan_with_verify() {
        let (_tmp, _reg, _mount, mut state) = connected_with_local_book();
        handle_key(&mut state, key(KeyCode::Char('s')));
        assert!(!sync(&state).verify);
        let action = handle_key(&mut state, key(KeyCode::Char('V')));
        assert!(matches!(action, DevicesAction::Status(_)));
        assert!(sync(&state).verify);
        assert_eq!(sync(&state).items.len(), 1);
    }
}
