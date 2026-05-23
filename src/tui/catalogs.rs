use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::catalog::handlers::{self, CatalogRow};
use crate::config::Registry;
use crate::tui::widgets::{render_modal, StatusMessage};

#[derive(Debug)]
pub struct State {
    pub rows: Vec<CatalogRow>,
    pub cursor: usize,
    pub confirm: Option<ConfirmRm>,
}

#[derive(Debug, Clone)]
pub struct ConfirmRm {
    pub name: String,
    pub purge: bool,
}

impl State {
    pub fn from_registry(registry: &Registry) -> Self {
        let rows = handlers::handle_ls(registry);
        Self {
            rows,
            cursor: 0,
            confirm: None,
        }
    }

    pub fn refresh(&mut self, registry: &Registry) {
        self.rows = handlers::handle_ls(registry);
        if self.cursor >= self.rows.len() {
            self.cursor = self.rows.len().saturating_sub(1);
        }
    }
}

pub enum CatalogsAction {
    None,
    Back,
    OpenWizard,
    OpenPalette,
    Status(StatusMessage),
}

pub fn handle_key(
    state: &mut State,
    key: KeyEvent,
    registry: &mut Registry,
    config_dir: &Path,
) -> CatalogsAction {
    if state.confirm.is_some() {
        return handle_confirm_key(state, key, registry, config_dir);
    }

    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            if !state.rows.is_empty() {
                state.cursor = (state.cursor + 1) % state.rows.len();
            }
            CatalogsAction::None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if !state.rows.is_empty() {
                state.cursor = (state.cursor + state.rows.len() - 1) % state.rows.len();
            }
            CatalogsAction::None
        }
        KeyCode::Enter => use_selected(state, registry, config_dir),
        KeyCode::Char('d') | KeyCode::Delete => {
            if let Some(row) = state.rows.get(state.cursor) {
                state.confirm = Some(ConfirmRm {
                    name: row.name.clone(),
                    purge: false,
                });
            }
            CatalogsAction::None
        }
        KeyCode::Char('n') => CatalogsAction::OpenWizard,
        KeyCode::Esc => CatalogsAction::Back,
        KeyCode::Char(':') => CatalogsAction::OpenPalette,
        _ => CatalogsAction::None,
    }
}

fn use_selected(state: &mut State, registry: &mut Registry, config_dir: &Path) -> CatalogsAction {
    let Some(row) = state.rows.get(state.cursor) else {
        return CatalogsAction::None;
    };
    if row.missing {
        return CatalogsAction::Status(StatusMessage::error(format!(
            "catalog `{}` is missing on disk; cannot switch",
            row.name
        )));
    }
    let name = row.name.clone();
    match handlers::handle_use(registry, config_dir, &name) {
        Ok(out) => {
            state.refresh(registry);
            CatalogsAction::Status(StatusMessage::info(format!("switched to `{}`", out.name)))
        }
        Err(err) => CatalogsAction::Status(StatusMessage::error(err.to_string())),
    }
}

fn handle_confirm_key(
    state: &mut State,
    key: KeyEvent,
    registry: &mut Registry,
    config_dir: &Path,
) -> CatalogsAction {
    let Some(confirm) = state.confirm.clone() else {
        return CatalogsAction::None;
    };
    match key.code {
        KeyCode::Char('y') | KeyCode::Enter => {
            state.confirm = None;
            do_remove(state, registry, config_dir, &confirm.name, false)
        }
        KeyCode::Char('p') => {
            state.confirm = None;
            do_remove(state, registry, config_dir, &confirm.name, true)
        }
        KeyCode::Char('n') | KeyCode::Esc => {
            state.confirm = None;
            CatalogsAction::None
        }
        _ => CatalogsAction::None,
    }
}

fn do_remove(
    state: &mut State,
    registry: &mut Registry,
    config_dir: &Path,
    name: &str,
    purge: bool,
) -> CatalogsAction {
    match handlers::handle_rm(registry, config_dir, name, purge) {
        Ok(out) => {
            state.refresh(registry);
            let suffix = if out.purged { " (purged)" } else { "" };
            CatalogsAction::Status(StatusMessage::info(format!(
                "removed `{}`{suffix}",
                out.name
            )))
        }
        Err(err) => CatalogsAction::Status(StatusMessage::error(err.to_string())),
    }
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    let header = Paragraph::new(vec![
        Line::from(Span::styled(
            "Catalogs",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Enter switch · n new · d delete · Esc back",
            Style::default().fg(Color::DarkGray),
        )),
    ]);
    frame.render_widget(header, layout[0]);

    let items: Vec<ListItem> = state
        .rows
        .iter()
        .map(|row| ListItem::new(row_line(row)))
        .collect();

    let list = List::new(items).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    let mut list_state = ListState::default();
    if !state.rows.is_empty() {
        list_state.select(Some(state.cursor.min(state.rows.len() - 1)));
    }
    frame.render_stateful_widget(list, layout[1], &mut list_state);

    if state.rows.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            "  no catalogs registered — press `n` to create one",
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(empty, layout[1]);
    }

    if let Some(confirm) = &state.confirm {
        let row = state.rows.iter().find(|r| r.name == confirm.name);
        let path_line = row
            .map(|r| {
                Line::from(Span::styled(
                    format!("  path: {}", r.path.display()),
                    Style::default().fg(Color::DarkGray),
                ))
            })
            .unwrap_or_else(|| Line::from(""));
        let lines = vec![
            Line::from(Span::raw(format!("Delete catalog `{}`?", confirm.name))),
            path_line,
            Line::from(""),
            Line::from(vec![
                Span::styled("y", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw("/Enter — unregister   "),
                Span::styled("p", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" — purge files   "),
                Span::styled("n", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw("/Esc — cancel"),
            ]),
        ];
        render_modal(frame, area, "confirm", lines);
    }
}

fn row_line(row: &CatalogRow) -> Line<'_> {
    let marker = if row.current { "*" } else { " " };
    let mut name_style = Style::default();
    if row.current {
        name_style = name_style.add_modifier(Modifier::BOLD);
    }
    if row.missing {
        name_style = name_style.fg(Color::Red);
    }
    let path = row.path.display().to_string();
    let mut spans = vec![
        Span::raw(format!(" {marker} ")),
        Span::styled(row.name.clone(), name_style),
        Span::raw("  "),
        Span::styled(path, Style::default().fg(Color::DarkGray)),
    ];
    if row.missing {
        spans.push(Span::styled("  (missing)", Style::default().fg(Color::Red)));
    }
    if let Some(desc) = &row.description {
        spans.push(Span::styled(
            format!("  — {desc}"),
            Style::default().fg(Color::DarkGray),
        ));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use std::fs;
    use tempfile::tempdir;

    use crate::catalog;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn setup() -> (tempfile::TempDir, std::path::PathBuf, Registry) {
        let dir = tempdir().unwrap();
        let cfg = dir.path().join("cfg");
        fs::create_dir_all(&cfg).unwrap();
        let cat1 = dir.path().join("lib1");
        let cat2 = dir.path().join("lib2");
        let mut reg = Registry::default();
        handlers::handle_init(&mut reg, &cfg, "one", &cat1, None, false).unwrap();
        handlers::handle_init(&mut reg, &cfg, "two", &cat2, None, true).unwrap();
        (dir, cfg, reg)
    }

    #[test]
    fn state_from_registry_lists_all_rows() {
        let (_tmp, _cfg, reg) = setup();
        let state = State::from_registry(&reg);
        assert_eq!(state.rows.len(), 2);
    }

    #[test]
    fn down_cycles_through_rows() {
        let (_tmp, cfg, mut reg) = setup();
        let mut state = State::from_registry(&reg);
        assert_eq!(state.cursor, 0);
        handle_key(&mut state, key(KeyCode::Down), &mut reg, &cfg);
        assert_eq!(state.cursor, 1);
        handle_key(&mut state, key(KeyCode::Down), &mut reg, &cfg);
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn enter_switches_current() {
        let (_tmp, cfg, mut reg) = setup();
        let mut state = State::from_registry(&reg);
        // cursor is on first row "one" but "two" is current; pressing enter on "one" switches.
        let action = handle_key(&mut state, key(KeyCode::Enter), &mut reg, &cfg);
        assert!(matches!(action, CatalogsAction::Status(_)));
        assert_eq!(reg.current.as_deref(), Some("one"));
    }

    #[test]
    fn enter_on_missing_returns_error_status() {
        let (tmp, cfg, mut reg) = setup();
        fs::remove_dir_all(tmp.path().join("lib1")).unwrap();
        let mut state = State::from_registry(&reg);
        let action = handle_key(&mut state, key(KeyCode::Enter), &mut reg, &cfg);
        match action {
            CatalogsAction::Status(s) => {
                assert!(matches!(s.kind, crate::tui::widgets::StatusKind::Error));
            }
            _ => panic!("expected error status"),
        }
    }

    #[test]
    fn d_opens_confirm_then_y_removes() {
        let (_tmp, cfg, mut reg) = setup();
        let mut state = State::from_registry(&reg);
        handle_key(&mut state, key(KeyCode::Char('d')), &mut reg, &cfg);
        assert!(state.confirm.is_some());
        handle_key(&mut state, key(KeyCode::Char('y')), &mut reg, &cfg);
        assert!(state.confirm.is_none());
        assert_eq!(reg.catalogs.len(), 1);
        assert_eq!(state.rows.len(), 1);
    }

    #[test]
    fn p_purges_files() {
        let (tmp, cfg, mut reg) = setup();
        let mut state = State::from_registry(&reg);
        handle_key(&mut state, key(KeyCode::Char('d')), &mut reg, &cfg);
        handle_key(&mut state, key(KeyCode::Char('p')), &mut reg, &cfg);
        assert!(!tmp.path().join("lib1").exists());
    }

    #[test]
    fn n_in_confirm_cancels() {
        let (_tmp, cfg, mut reg) = setup();
        let mut state = State::from_registry(&reg);
        handle_key(&mut state, key(KeyCode::Char('d')), &mut reg, &cfg);
        handle_key(&mut state, key(KeyCode::Char('n')), &mut reg, &cfg);
        assert!(state.confirm.is_none());
        assert_eq!(reg.catalogs.len(), 2);
    }

    #[test]
    fn esc_returns_back_action() {
        let (_tmp, cfg, mut reg) = setup();
        let mut state = State::from_registry(&reg);
        let action = handle_key(&mut state, key(KeyCode::Esc), &mut reg, &cfg);
        assert!(matches!(action, CatalogsAction::Back));
    }

    #[test]
    fn colon_returns_open_palette() {
        let (_tmp, cfg, mut reg) = setup();
        let mut state = State::from_registry(&reg);
        let action = handle_key(&mut state, key(KeyCode::Char(':')), &mut reg, &cfg);
        assert!(matches!(action, CatalogsAction::OpenPalette));
    }

    #[test]
    fn n_returns_open_wizard() {
        let (_tmp, cfg, mut reg) = setup();
        let mut state = State::from_registry(&reg);
        let action = handle_key(&mut state, key(KeyCode::Char('n')), &mut reg, &cfg);
        assert!(matches!(action, CatalogsAction::OpenWizard));
    }

    #[test]
    fn marks_current_and_missing_in_rows() {
        let (tmp, _cfg, reg) = setup();
        fs::remove_dir_all(tmp.path().join("lib2")).unwrap();
        let state = State::from_registry(&reg);
        let one = state.rows.iter().find(|r| r.name == "one").unwrap();
        let two = state.rows.iter().find(|r| r.name == "two").unwrap();
        assert!(one.current ^ two.current, "exactly one should be current");
        assert!(two.missing, "two's path was removed");
        assert!(!catalog::is_initialized(&two.path));
    }
}
