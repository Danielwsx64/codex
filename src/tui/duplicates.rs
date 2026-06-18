use std::collections::BTreeSet;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::catalog::{self, books};
use crate::config::Registry;
use crate::dedup::{self, DetectBy, SuggestionReason};
use crate::tui::confirm;
use crate::tui::help::{Binding, Section};
use crate::tui::widgets::StatusMessage;

const BINDINGS: &[Binding] = &[
    Binding {
        keys: "↑↓ / j k",
        desc: "move selection",
    },
    Binding {
        keys: "Space",
        desc: "mark / unmark copy",
    },
    Binding {
        keys: "Enter",
        desc: "remove marked copies",
    },
    Binding {
        keys: "K",
        desc: "toggle keep-file (move to cwd)",
    },
    Binding {
        keys: "Esc",
        desc: "back to welcome",
    },
];

const CONFIRM_BINDINGS: &[Binding] = &[
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

#[derive(Debug, Clone)]
pub struct CatalogContext {
    pub name: String,
    pub dir: PathBuf,
}

#[derive(Debug)]
pub struct State {
    pub catalog: Option<CatalogContext>,
    pub groups: Vec<GroupRow>,
    pub flat: Vec<FlatItem>,
    pub cursor: usize,
    pub selected: BTreeSet<i64>,
    pub keep: bool,
    pub confirm: Option<confirm::State>,
    pub load_error: Option<String>,
}

#[derive(Debug)]
pub struct GroupRow {
    pub members: Vec<MemberRow>,
    pub reason: SuggestionReason,
    pub linked_by_hash: bool,
    pub linked_by_meta: bool,
}

#[derive(Debug)]
pub struct MemberRow {
    pub id: i64,
    pub title: String,
    pub author: Option<String>,
    pub format: String,
    pub score: u32,
    pub suggested: bool,
}

// A flattened, navigable view of the grouped data: a header per group followed
// by its members. Selection is keyed by book id, so it survives the rebuild
// after a removal even though positions shift.
#[derive(Debug, Clone, Copy)]
pub enum FlatItem {
    Header(usize),
    Member { group: usize, id: i64 },
}

#[derive(Debug)]
pub enum DuplicatesAction {
    None,
    Back,
    OpenPalette,
    Status(StatusMessage),
}

impl State {
    pub fn load(registry: &Registry) -> Self {
        let mut state = Self {
            catalog: None,
            groups: Vec::new(),
            flat: Vec::new(),
            cursor: 0,
            selected: BTreeSet::new(),
            keep: false,
            confirm: None,
            load_error: None,
        };
        if let Ok(entry) = registry.resolve(None) {
            state.catalog = Some(CatalogContext {
                name: entry.name.clone(),
                dir: entry.path.clone(),
            });
            state.reload(true);
        }
        state
    }

    // Recompute groups from the catalog. `backfill` runs the (best-effort)
    // fingerprint backfill, only worth doing on the first load.
    fn reload(&mut self, backfill: bool) {
        let Some(ctx) = self.catalog.clone() else {
            return;
        };
        let conn = match catalog::open_existing(&ctx.dir) {
            Ok(conn) => conn,
            Err(err) => {
                self.load_error = Some(err.to_string());
                return;
            }
        };
        if backfill {
            if let Err(err) = books::ensure_fingerprints(&conn, &ctx.dir) {
                tracing::warn!(error = %err, "fingerprint backfill failed; hash detection may be incomplete");
            }
        }
        let everything = match books::handle_ls(&conn) {
            Ok(rows) => rows,
            Err(err) => {
                self.load_error = Some(err.to_string());
                return;
            }
        };
        let hashes = match books::load_all_hashes(&conn) {
            Ok(rows) => rows,
            Err(err) => {
                self.load_error = Some(err.to_string());
                return;
            }
        };
        let groups = dedup::find_duplicate_groups(&everything, &hashes, DetectBy::All);
        let by_id: std::collections::HashMap<i64, &books::Book> =
            everything.iter().map(|b| (b.id, b)).collect();

        self.groups = groups
            .iter()
            .map(|g| GroupRow {
                reason: g.reason,
                linked_by_hash: g.linked_by_hash,
                linked_by_meta: g.linked_by_meta,
                members: g
                    .members
                    .iter()
                    .filter_map(|id| by_id.get(id).copied())
                    .map(|b| MemberRow {
                        id: b.id,
                        title: b.title.clone(),
                        author: b.author.clone(),
                        format: b.format.clone(),
                        score: dedup::completeness_score(b),
                        suggested: b.id == g.suggested,
                    })
                    .collect(),
            })
            .collect();

        // Pre-mark every group's suggested copy: the screen owns its own
        // selection set, so accepting the suggestion is zero keystrokes.
        self.selected = self
            .groups
            .iter()
            .flat_map(|g| g.members.iter())
            .filter(|m| m.suggested)
            .map(|m| m.id)
            .collect();

        self.rebuild_flat();
        self.load_error = None;
    }

    fn rebuild_flat(&mut self) {
        let mut flat = Vec::new();
        for (gi, group) in self.groups.iter().enumerate() {
            flat.push(FlatItem::Header(gi));
            for m in &group.members {
                flat.push(FlatItem::Member {
                    group: gi,
                    id: m.id,
                });
            }
        }
        self.flat = flat;
        if self.cursor >= self.flat.len() {
            self.cursor = self.flat.len().saturating_sub(1);
        }
    }
}

pub fn captures_text_input(_state: &State) -> bool {
    false
}

pub fn help_sections(state: &State) -> Vec<Section> {
    if state.confirm.is_some() {
        return vec![Section {
            title: "Confirm removal",
            bindings: CONFIRM_BINDINGS,
        }];
    }
    vec![Section {
        title: "Duplicates",
        bindings: BINDINGS,
    }]
}

pub fn handle_key(state: &mut State, key: KeyEvent) -> DuplicatesAction {
    if state.confirm.is_some() {
        return handle_confirm_key(state, key);
    }
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            if !state.flat.is_empty() {
                state.cursor = (state.cursor + 1) % state.flat.len();
            }
            DuplicatesAction::None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if !state.flat.is_empty() {
                state.cursor = (state.cursor + state.flat.len() - 1) % state.flat.len();
            }
            DuplicatesAction::None
        }
        KeyCode::Char(' ') => {
            if let Some(FlatItem::Member { id, .. }) = state.flat.get(state.cursor).copied() {
                if !state.selected.remove(&id) {
                    state.selected.insert(id);
                }
            }
            DuplicatesAction::None
        }
        KeyCode::Char('K') => {
            state.keep = !state.keep;
            DuplicatesAction::None
        }
        KeyCode::Enter => {
            if state.selected.is_empty() {
                return DuplicatesAction::Status(StatusMessage::info(
                    "no copies marked — press Space to mark",
                ));
            }
            let action = if state.keep { "move to cwd" } else { "delete" };
            state.confirm = Some(confirm::State {
                title: "remove duplicates".to_string(),
                message: format!(
                    "{action} {} book(s) from the catalog?",
                    state.selected.len()
                ),
                ok_label: "[ Remove ]".to_string(),
                cancel_label: "[ Cancel ]".to_string(),
                focus: confirm::Button::Cancel,
            });
            DuplicatesAction::None
        }
        KeyCode::Esc => DuplicatesAction::Back,
        KeyCode::Char(':') => DuplicatesAction::OpenPalette,
        _ => DuplicatesAction::None,
    }
}

fn handle_confirm_key(state: &mut State, key: KeyEvent) -> DuplicatesAction {
    let Some(dialog) = state.confirm.as_mut() else {
        return DuplicatesAction::None;
    };
    match confirm::handle_key(dialog, key) {
        confirm::ConfirmAction::None => DuplicatesAction::None,
        confirm::ConfirmAction::Cancel => {
            state.confirm = None;
            DuplicatesAction::None
        }
        confirm::ConfirmAction::Confirm => apply_removal(state),
    }
}

fn apply_removal(state: &mut State) -> DuplicatesAction {
    state.confirm = None;
    let ids: Vec<i64> = state.selected.iter().copied().collect();
    let keep = state.keep;
    let Some(dir) = state.catalog.as_ref().map(|c| c.dir.clone()) else {
        return DuplicatesAction::None;
    };
    let mut conn = match catalog::open_existing(&dir) {
        Ok(conn) => conn,
        Err(err) => return DuplicatesAction::Status(StatusMessage::error(err.to_string())),
    };

    let mut removed = 0;
    for id in &ids {
        match books::handle_rm(&mut conn, &dir, &id.to_string(), keep) {
            Ok(_) => removed += 1,
            Err(err) => return DuplicatesAction::Status(StatusMessage::error(err.to_string())),
        }
    }
    drop(conn);

    state.reload(false);
    DuplicatesAction::Status(StatusMessage::info(format!(
        "removed {removed} duplicate book(s)"
    )))
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    let name = state.catalog.as_ref().map(|c| c.name.as_str());
    let mut title = match name {
        Some(name) => format!("Duplicates — {name}"),
        None => "Duplicates".to_string(),
    };
    if !state.selected.is_empty() {
        title.push_str(&format!("  ({} marked)", state.selected.len()));
    }
    if state.keep {
        title.push_str("  · keep-file");
    }
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

    if state.flat.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            "  no duplicate books found",
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(empty, layout[1]);
        return;
    }

    let items: Vec<ListItem> = state
        .flat
        .iter()
        .map(|item| ListItem::new(flat_line(state, item)))
        .collect();
    let list = List::new(items).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    let mut list_state = ListState::default();
    list_state.select(Some(state.cursor.min(state.flat.len() - 1)));
    frame.render_stateful_widget(list, layout[1], &mut list_state);

    if let Some(dialog) = &state.confirm {
        confirm::render(frame, area, dialog);
    }
}

fn linked_label(group: &GroupRow) -> String {
    let mut parts = Vec::new();
    if group.linked_by_hash {
        parts.push("hash");
    }
    if group.linked_by_meta {
        parts.push("meta");
    }
    parts.join(", ")
}

fn flat_line(state: &State, item: &FlatItem) -> Line<'static> {
    match item {
        FlatItem::Header(gi) => {
            let group = &state.groups[*gi];
            Line::from(Span::styled(
                format!("Group {} — linked by {}", gi + 1, linked_label(group)),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ))
        }
        FlatItem::Member { group, id } => {
            let g = &state.groups[*group];
            let m = g
                .members
                .iter()
                .find(|m| m.id == *id)
                .expect("flat member id always refers to a current group member");
            let check = if state.selected.contains(id) {
                "[x]"
            } else {
                "[ ]"
            };
            let suggested = if m.suggested {
                format!("  * suggested: {}", g.reason.label())
            } else {
                String::new()
            };
            let author = m.author.as_deref().unwrap_or("-");
            Line::from(Span::raw(format!(
                "  {check} id {id}  {title} — {author} ({fmt}, score {score}){suggested}",
                title = m.title,
                fmt = m.format,
                score = m.score,
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::handlers;
    use crate::config::Registry;
    use crossterm::event::{KeyEvent, KeyModifiers};
    use std::path::Path;
    use tempfile::tempdir;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    // Build a registry with a single current catalog and seed two metadata-
    // identical books (different formats), forming one duplicate group.
    fn registry_with_dupes(cfg: &Path, root: &Path) -> Registry {
        let mut reg = Registry::default();
        let cat = root.join("cat");
        handlers::handle_init(&mut reg, cfg, "main", &cat, None, true).unwrap();
        let conn = catalog::open_existing(&cat).unwrap();
        conn.execute(
            "INSERT INTO books (title, author, format, file_path, isbn) VALUES ('Dune', 'Frank Herbert', 'epub', 'books/1/a.epub', '123')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO books (title, author, format, file_path) VALUES ('Dune', 'Frank Herbert', 'pdf', 'books/2/b.pdf')",
            [],
        )
        .unwrap();
        reg
    }

    #[test]
    fn load_finds_group_and_premarks_suggestion() {
        let dir = tempdir().unwrap();
        let cfg = dir.path().join("cfg");
        std::fs::create_dir_all(&cfg).unwrap();
        let reg = registry_with_dupes(&cfg, dir.path());

        let state = State::load(&reg);
        assert_eq!(state.groups.len(), 1);
        // The book without isbn (id 2, lower score) is the suggested copy.
        assert_eq!(state.selected.iter().copied().collect::<Vec<_>>(), vec![2]);
        // Flat list: one header + two members.
        assert_eq!(state.flat.len(), 3);
    }

    #[test]
    fn space_toggles_member_selection() {
        let dir = tempdir().unwrap();
        let cfg = dir.path().join("cfg");
        std::fs::create_dir_all(&cfg).unwrap();
        let reg = registry_with_dupes(&cfg, dir.path());
        let mut state = State::load(&reg);

        // Move cursor to the first member (index 1; index 0 is the header).
        state.cursor = 1;
        let before = state.selected.contains(&1);
        handle_key(&mut state, key(KeyCode::Char(' ')));
        assert_ne!(state.selected.contains(&1), before);
    }

    #[test]
    fn space_on_header_is_noop() {
        let dir = tempdir().unwrap();
        let cfg = dir.path().join("cfg");
        std::fs::create_dir_all(&cfg).unwrap();
        let reg = registry_with_dupes(&cfg, dir.path());
        let mut state = State::load(&reg);
        state.cursor = 0; // header
        let before = state.selected.clone();
        handle_key(&mut state, key(KeyCode::Char(' ')));
        assert_eq!(state.selected, before);
    }

    #[test]
    fn enter_with_marks_opens_confirm_focused_on_cancel() {
        let dir = tempdir().unwrap();
        let cfg = dir.path().join("cfg");
        std::fs::create_dir_all(&cfg).unwrap();
        let reg = registry_with_dupes(&cfg, dir.path());
        let mut state = State::load(&reg);
        handle_key(&mut state, key(KeyCode::Enter));
        let dialog = state.confirm.as_ref().expect("confirm dialog opens");
        assert!(matches!(dialog.focus, confirm::Button::Cancel));
    }

    #[test]
    fn enter_without_marks_returns_status() {
        let dir = tempdir().unwrap();
        let cfg = dir.path().join("cfg");
        std::fs::create_dir_all(&cfg).unwrap();
        let reg = registry_with_dupes(&cfg, dir.path());
        let mut state = State::load(&reg);
        state.selected.clear();
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, DuplicatesAction::Status(_)));
        assert!(state.confirm.is_none());
    }

    #[test]
    fn confirm_removes_and_group_disappears() {
        let dir = tempdir().unwrap();
        let cfg = dir.path().join("cfg");
        std::fs::create_dir_all(&cfg).unwrap();
        let reg = registry_with_dupes(&cfg, dir.path());
        let mut state = State::load(&reg);
        // keep-file so handle_rm doesn't need the (absent) book dir on disk.
        state.keep = false;

        handle_key(&mut state, key(KeyCode::Enter)); // open confirm
                                                     // Move focus to OK and confirm.
        handle_key(&mut state, key(KeyCode::Left));
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, DuplicatesAction::Status(_)));
        // The remaining single book is no longer a duplicate group.
        assert!(state.groups.is_empty());
    }

    #[test]
    fn esc_returns_back() {
        let mut state = State {
            catalog: None,
            groups: Vec::new(),
            flat: Vec::new(),
            cursor: 0,
            selected: BTreeSet::new(),
            keep: false,
            confirm: None,
            load_error: None,
        };
        let action = handle_key(&mut state, key(KeyCode::Esc));
        assert!(matches!(action, DuplicatesAction::Back));
    }

    #[test]
    fn colon_opens_palette() {
        let mut state = State {
            catalog: None,
            groups: Vec::new(),
            flat: Vec::new(),
            cursor: 0,
            selected: BTreeSet::new(),
            keep: false,
            confirm: None,
            load_error: None,
        };
        let action = handle_key(&mut state, key(KeyCode::Char(':')));
        assert!(matches!(action, DuplicatesAction::OpenPalette));
    }
}
