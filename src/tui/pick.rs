use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::tui::TerminalGuard;

#[derive(Debug, PartialEq, Eq)]
enum PickAction {
    None,
    Selected(usize),
    Cancel,
}

struct State<'a> {
    title: &'a str,
    labels: &'a [String],
    cursor: usize,
}

// Open a full-screen list and let the user pick one row with the arrow keys (or
// j/k) + Enter. Returns the chosen index, or None if the user cancelled (Esc / q
// / Ctrl+C). Rows are pre-rendered labels so the picker stays decoupled from any
// particular domain type. The caller guarantees a TTY; the TerminalGuard
// restores the terminal on every exit path, panics included.
pub(crate) fn pick(title: &str, labels: &[String]) -> Result<Option<usize>> {
    let mut state = State {
        title,
        labels,
        cursor: 0,
    };
    let mut guard = TerminalGuard::enter()?;
    loop {
        guard.terminal.draw(|f| render(f, f.area(), &state))?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        // Ignore key release/repeat to avoid double-firing on terminals that emit them.
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match handle_key(&mut state, key) {
            PickAction::None => {}
            PickAction::Selected(i) => return Ok(Some(i)),
            PickAction::Cancel => return Ok(None),
        }
    }
}

fn handle_key(state: &mut State, key: KeyEvent) -> PickAction {
    // Ctrl+C is a reserved exit key everywhere in the TUI.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return PickAction::Cancel;
    }
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            if !state.labels.is_empty() {
                state.cursor = (state.cursor + 1) % state.labels.len();
            }
            PickAction::None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if !state.labels.is_empty() {
                state.cursor = (state.cursor + state.labels.len() - 1) % state.labels.len();
            }
            PickAction::None
        }
        KeyCode::Enter if !state.labels.is_empty() => {
            PickAction::Selected(state.cursor.min(state.labels.len() - 1))
        }
        KeyCode::Esc | KeyCode::Char('q') => PickAction::Cancel,
        _ => PickAction::None,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum MultiAction {
    None,
    Confirm,
    Cancel,
}

struct MultiState<'a> {
    title: &'a str,
    labels: &'a [String],
    cursor: usize,
    selected: Vec<bool>,
}

// Open a full-screen list and let the user mark any number of rows with Space,
// then confirm with Enter. Returns the marked indices (possibly empty) on confirm,
// or None if the user cancelled (Esc / q / Ctrl+C). Like `pick`, rows are
// pre-rendered labels and the caller guarantees a TTY; the TerminalGuard restores
// the terminal on every exit path.
pub(crate) fn pick_multi(title: &str, labels: &[String]) -> Result<Option<Vec<usize>>> {
    let mut state = MultiState {
        title,
        labels,
        cursor: 0,
        selected: vec![false; labels.len()],
    };
    let mut guard = TerminalGuard::enter()?;
    loop {
        guard.terminal.draw(|f| render_multi(f, f.area(), &state))?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match handle_multi_key(&mut state, key) {
            MultiAction::None => {}
            MultiAction::Confirm => {
                let chosen = state
                    .selected
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &on)| on.then_some(i))
                    .collect();
                return Ok(Some(chosen));
            }
            MultiAction::Cancel => return Ok(None),
        }
    }
}

fn handle_multi_key(state: &mut MultiState, key: KeyEvent) -> MultiAction {
    // Ctrl+C is a reserved exit key everywhere in the TUI.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return MultiAction::Cancel;
    }
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            if !state.labels.is_empty() {
                state.cursor = (state.cursor + 1) % state.labels.len();
            }
            MultiAction::None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if !state.labels.is_empty() {
                state.cursor = (state.cursor + state.labels.len() - 1) % state.labels.len();
            }
            MultiAction::None
        }
        KeyCode::Char(' ') => {
            if let Some(slot) = state.selected.get_mut(state.cursor) {
                *slot = !*slot;
            }
            MultiAction::None
        }
        KeyCode::Enter => MultiAction::Confirm,
        KeyCode::Esc | KeyCode::Char('q') => MultiAction::Cancel,
        _ => MultiAction::None,
    }
}

fn render_multi(frame: &mut Frame<'_>, area: Rect, state: &MultiState) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    let header = Paragraph::new(Line::from(Span::styled(
        state.title,
        Style::default().add_modifier(Modifier::BOLD),
    )));
    frame.render_widget(header, layout[0]);

    let items: Vec<ListItem> = state
        .labels
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let mark = if state.selected.get(i).copied().unwrap_or(false) {
                "[x] "
            } else {
                "[ ] "
            };
            ListItem::new(Line::from(format!("{mark}{label}")))
        })
        .collect();
    let list = List::new(items).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    let mut list_state = ListState::default();
    if !state.labels.is_empty() {
        list_state.select(Some(state.cursor.min(state.labels.len() - 1)));
    }
    frame.render_stateful_widget(list, layout[1], &mut list_state);

    let footer = Paragraph::new(Line::from(Span::styled(
        "  ↑↓ / j k  move · Space  mark · Enter  confirm · Esc / q  cancel",
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(footer, layout[2]);
}

fn render(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    let header = Paragraph::new(Line::from(Span::styled(
        state.title,
        Style::default().add_modifier(Modifier::BOLD),
    )));
    frame.render_widget(header, layout[0]);

    let items: Vec<ListItem> = state
        .labels
        .iter()
        .map(|label| ListItem::new(Line::from(label.clone())))
        .collect();
    let list = List::new(items).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    let mut list_state = ListState::default();
    if !state.labels.is_empty() {
        list_state.select(Some(state.cursor.min(state.labels.len() - 1)));
    }
    frame.render_stateful_widget(list, layout[1], &mut list_state);

    let footer = Paragraph::new(Line::from(Span::styled(
        "  ↑↓ / j k  move · Enter  select · Esc / q  cancel",
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(footer, layout[2]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("row {i}")).collect()
    }

    fn state(labels: &[String], cursor: usize) -> State<'_> {
        State {
            title: "pick",
            labels,
            cursor,
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn down_and_up_wrap_around() {
        let rows = labels(3);
        let mut state = state(&rows, 0);

        assert_eq!(handle_key(&mut state, key(KeyCode::Down)), PickAction::None);
        assert_eq!(state.cursor, 1);
        assert_eq!(
            handle_key(&mut state, key(KeyCode::Char('j'))),
            PickAction::None
        );
        assert_eq!(state.cursor, 2);
        // wraps past the end
        handle_key(&mut state, key(KeyCode::Down));
        assert_eq!(state.cursor, 0);
        // wraps below the start
        assert_eq!(handle_key(&mut state, key(KeyCode::Up)), PickAction::None);
        assert_eq!(state.cursor, 2);
        handle_key(&mut state, key(KeyCode::Char('k')));
        assert_eq!(state.cursor, 1);
    }

    #[test]
    fn movement_on_empty_list_is_a_no_op() {
        let rows: Vec<String> = Vec::new();
        let mut state = state(&rows, 0);
        handle_key(&mut state, key(KeyCode::Down));
        handle_key(&mut state, key(KeyCode::Up));
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn enter_selects_the_cursor() {
        let rows = labels(2);
        let mut state = state(&rows, 1);
        assert_eq!(
            handle_key(&mut state, key(KeyCode::Enter)),
            PickAction::Selected(1)
        );
    }

    #[test]
    fn enter_on_empty_list_does_nothing() {
        let rows: Vec<String> = Vec::new();
        let mut state = state(&rows, 0);
        assert_eq!(
            handle_key(&mut state, key(KeyCode::Enter)),
            PickAction::None
        );
    }

    #[test]
    fn esc_q_and_ctrl_c_cancel() {
        let rows = labels(1);
        let mut state = state(&rows, 0);
        assert_eq!(
            handle_key(&mut state, key(KeyCode::Esc)),
            PickAction::Cancel
        );
        assert_eq!(
            handle_key(&mut state, key(KeyCode::Char('q'))),
            PickAction::Cancel
        );
        assert_eq!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
            ),
            PickAction::Cancel
        );
    }

    fn multi_state(labels: &[String]) -> MultiState<'_> {
        MultiState {
            title: "pick",
            labels,
            cursor: 0,
            selected: vec![false; labels.len()],
        }
    }

    #[test]
    fn space_toggles_the_row_under_the_cursor() {
        let rows = labels(3);
        let mut state = multi_state(&rows);
        handle_multi_key(&mut state, key(KeyCode::Down));
        assert_eq!(
            handle_multi_key(&mut state, key(KeyCode::Char(' '))),
            MultiAction::None
        );
        assert_eq!(state.selected, vec![false, true, false]);
        // Toggling again clears it.
        handle_multi_key(&mut state, key(KeyCode::Char(' ')));
        assert_eq!(state.selected, vec![false, false, false]);
    }

    #[test]
    fn enter_confirms_with_the_marked_indices() {
        let rows = labels(3);
        let mut state = multi_state(&rows);
        // mark row 0, move to row 2, mark it
        handle_multi_key(&mut state, key(KeyCode::Char(' ')));
        handle_multi_key(&mut state, key(KeyCode::Up)); // wraps to row 2
        handle_multi_key(&mut state, key(KeyCode::Char(' ')));
        assert_eq!(
            handle_multi_key(&mut state, key(KeyCode::Enter)),
            MultiAction::Confirm
        );
        assert_eq!(state.selected, vec![true, false, true]);
    }

    #[test]
    fn multi_movement_wraps() {
        let rows = labels(3);
        let mut state = multi_state(&rows);
        handle_multi_key(&mut state, key(KeyCode::Up));
        assert_eq!(state.cursor, 2);
        handle_multi_key(&mut state, key(KeyCode::Char('j')));
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn multi_esc_q_and_ctrl_c_cancel() {
        let rows = labels(1);
        let mut state = multi_state(&rows);
        assert_eq!(
            handle_multi_key(&mut state, key(KeyCode::Esc)),
            MultiAction::Cancel
        );
        assert_eq!(
            handle_multi_key(&mut state, key(KeyCode::Char('q'))),
            MultiAction::Cancel
        );
        assert_eq!(
            handle_multi_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
            ),
            MultiAction::Cancel
        );
    }
}
