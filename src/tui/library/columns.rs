use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::catalog;
use crate::catalog::columns::LibraryColumn;
use crate::catalog::settings;
use crate::tui::widgets::centered_rect;

#[derive(Debug, Clone)]
pub struct State {
    pub selected: Vec<bool>,
    pub cursor: usize,
    pub error: Option<String>,
}

impl State {
    pub fn from_active(active: &[LibraryColumn]) -> Self {
        let selected: Vec<bool> = LibraryColumn::ALL
            .iter()
            .map(|c| active.contains(c))
            .collect();
        Self {
            selected,
            cursor: 0,
            error: None,
        }
    }
}

pub enum ColumnsAction {
    None,
    Cancel,
    Saved(Vec<LibraryColumn>),
}

pub fn handle_key(state: &mut State, key: KeyEvent, catalog_dir: &Path) -> ColumnsAction {
    let total = LibraryColumn::ALL.len();
    match key.code {
        KeyCode::Esc => ColumnsAction::Cancel,
        KeyCode::Up | KeyCode::Char('k') => {
            if total > 0 {
                state.cursor = (state.cursor + total - 1) % total;
            }
            ColumnsAction::None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if total > 0 {
                state.cursor = (state.cursor + 1) % total;
            }
            ColumnsAction::None
        }
        KeyCode::Char(' ') => {
            if let Some(b) = state.selected.get_mut(state.cursor) {
                *b = !*b;
            }
            state.error = None;
            ColumnsAction::None
        }
        KeyCode::Enter => submit(state, catalog_dir),
        _ => ColumnsAction::None,
    }
}

fn submit(state: &mut State, catalog_dir: &Path) -> ColumnsAction {
    let chosen: Vec<LibraryColumn> = LibraryColumn::ALL
        .iter()
        .zip(state.selected.iter())
        .filter_map(|(col, on)| if *on { Some(*col) } else { None })
        .collect();
    if chosen.is_empty() {
        state.error = Some("select at least one column".to_string());
        return ColumnsAction::None;
    }
    let conn = match catalog::open_existing(catalog_dir) {
        Ok(c) => c,
        Err(err) => {
            state.error = Some(err.to_string());
            return ColumnsAction::None;
        }
    };
    if let Err(err) = settings::save_library_columns(&conn, &chosen) {
        state.error = Some(err.to_string());
        return ColumnsAction::None;
    }
    ColumnsAction::Saved(chosen)
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let target_w = 36u16.min(area.width);
    let target_h = (LibraryColumn::ALL.len() as u16 + 6).min(area.height);
    let rect = centered_rect(target_w, target_h, area);
    frame.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" columns ")
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header hint
            Constraint::Min(1),    // list
            Constraint::Length(1), // footer hint
            Constraint::Length(1), // error
        ])
        .split(inner);

    let header = Paragraph::new(Line::from(Span::styled(
        "  pick the columns shown in the library table",
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(header, layout[0]);

    let items: Vec<ListItem<'_>> = LibraryColumn::ALL
        .iter()
        .zip(state.selected.iter())
        .map(|(col, on)| {
            let mark = if *on { "[x]" } else { "[ ]" };
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(mark.to_string(), Style::default().fg(Color::Cyan)),
                Span::raw("  "),
                Span::raw(col.header().to_string()),
            ]))
        })
        .collect();
    let list = List::new(items).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    let mut list_state = ListState::default();
    list_state.select(Some(state.cursor.min(LibraryColumn::ALL.len() - 1)));
    frame.render_stateful_widget(list, layout[1], &mut list_state);

    let footer = Paragraph::new(Line::from(Span::styled(
        "  ↑↓ move · Space toggle · Enter save · Esc cancel",
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(footer, layout[2]);

    if let Some(err) = &state.error {
        let p = Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                err.clone(),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
        ]));
        frame.render_widget(p, layout[3]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use tempfile::tempdir;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn from_active_marks_selected_columns() {
        let active = vec![
            LibraryColumn::Id,
            LibraryColumn::Title,
            LibraryColumn::Rating,
        ];
        let s = State::from_active(&active);
        for (i, col) in LibraryColumn::ALL.iter().enumerate() {
            assert_eq!(s.selected[i], active.contains(col), "{col:?}");
        }
    }

    #[test]
    fn space_toggles_current_cursor() {
        let mut s = State::from_active(LibraryColumn::DEFAULT);
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        catalog::init(&cat).unwrap();
        // Move cursor to a column not in default (Series at index 4).
        for _ in 0..4 {
            handle_key(&mut s, key(KeyCode::Down), &cat);
        }
        assert!(!s.selected[4]);
        handle_key(&mut s, key(KeyCode::Char(' ')), &cat);
        assert!(s.selected[4]);
    }

    #[test]
    fn enter_saves_when_at_least_one_selected() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        catalog::init(&cat).unwrap();
        let mut s = State::from_active(LibraryColumn::DEFAULT);
        let action = handle_key(&mut s, key(KeyCode::Enter), &cat);
        match action {
            ColumnsAction::Saved(cols) => {
                assert_eq!(cols, LibraryColumn::DEFAULT.to_vec());
            }
            _ => panic!("expected Saved"),
        }
        let conn = catalog::open_existing(&cat).unwrap();
        let loaded = settings::load_library_columns(&conn).unwrap();
        assert_eq!(loaded, LibraryColumn::DEFAULT.to_vec());
    }

    #[test]
    fn enter_with_no_selection_keeps_overlay_open_with_error() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        catalog::init(&cat).unwrap();
        let mut s = State::from_active(&[]);
        let action = handle_key(&mut s, key(KeyCode::Enter), &cat);
        assert!(matches!(action, ColumnsAction::None));
        assert!(s.error.is_some());
    }

    #[test]
    fn esc_cancels() {
        let dir = tempdir().unwrap();
        let cat = dir.path().join("c");
        catalog::init(&cat).unwrap();
        let mut s = State::from_active(LibraryColumn::DEFAULT);
        let action = handle_key(&mut s, key(KeyCode::Esc), &cat);
        assert!(matches!(action, ColumnsAction::Cancel));
    }
}
