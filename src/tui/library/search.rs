use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

use crate::catalog::books::RatingRange;
use crate::tui::library::{ActiveFilter, FilterCriteria, FilterKind};
use crate::tui::widgets::{centered_rect, is_submit_key};

const LABEL_COL_WIDTH: u16 = 12;
const LEFT_PAD: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Query,
    Author,
    Tags,
    Series,
    Rating,
    Apply,
    Cancel,
}

impl Focus {
    const ORDER: [Focus; 7] = [
        Focus::Query,
        Focus::Author,
        Focus::Tags,
        Focus::Series,
        Focus::Rating,
        Focus::Apply,
        Focus::Cancel,
    ];

    pub fn next(self) -> Self {
        let idx = Self::ORDER.iter().position(|f| *f == self).unwrap_or(0);
        Self::ORDER[(idx + 1) % Self::ORDER.len()]
    }

    pub fn prev(self) -> Self {
        let idx = Self::ORDER.iter().position(|f| *f == self).unwrap_or(0);
        let len = Self::ORDER.len();
        Self::ORDER[(idx + len - 1) % len]
    }
}

#[derive(Debug)]
pub struct State {
    pub query: Input,
    pub author: Input,
    pub tags: Input,
    pub series: Input,
    pub rating: Input,
    pub focus: Focus,
    pub error: Option<String>,
}

impl State {
    pub fn from_filter(filter: Option<&ActiveFilter>) -> Self {
        let mut state = Self {
            query: Input::default(),
            author: Input::default(),
            tags: Input::default(),
            series: Input::default(),
            rating: Input::default(),
            focus: Focus::Query,
            error: None,
        };
        let Some(active) = filter else {
            return state;
        };
        let c = &active.criteria;
        // A quick `/` filter only carries free text — bring just that across so
        // the user can flesh it out with structured fields. An advanced filter
        // round-trips every field for editing.
        state.query = Input::default().with_value(c.query.clone().unwrap_or_default());
        if active.kind == FilterKind::Advanced {
            state.author = Input::default().with_value(c.author.clone().unwrap_or_default());
            state.tags = Input::default().with_value(c.tags.join(", "));
            state.series = Input::default().with_value(c.series.clone().unwrap_or_default());
            state.rating =
                Input::default().with_value(c.rating.map(format_rating).unwrap_or_default());
        }
        state
    }
}

pub enum SearchAction {
    None,
    Cancel,
    Apply(FilterCriteria),
}

// While the search wizard is open, swallow `q`/`Ctrl+C` so the user can keep
// typing query terms without the exit keys firing.
pub fn captures_text_input(_state: &State) -> bool {
    true
}

pub fn handle_key(state: &mut State, key: KeyEvent) -> SearchAction {
    // Ctrl+S (or Ctrl+Enter where the terminal supports it) is the project-wide
    // form submit: apply from any field.
    if is_submit_key(&key) {
        return apply(state);
    }

    match key.code {
        KeyCode::Esc => return SearchAction::Cancel,
        KeyCode::Tab | KeyCode::Down => {
            state.focus = state.focus.next();
            return SearchAction::None;
        }
        KeyCode::BackTab | KeyCode::Up => {
            state.focus = state.focus.prev();
            return SearchAction::None;
        }
        KeyCode::Enter => match state.focus {
            Focus::Apply => return apply(state),
            Focus::Cancel => return SearchAction::Cancel,
            _ => {
                state.focus = state.focus.next();
                return SearchAction::None;
            }
        },
        _ => {}
    }

    let event = Event::Key(key);
    match state.focus {
        Focus::Apply | Focus::Cancel => {}
        Focus::Query => {
            state.query.handle_event(&event);
        }
        Focus::Author => {
            state.author.handle_event(&event);
        }
        Focus::Tags => {
            state.tags.handle_event(&event);
        }
        Focus::Series => {
            state.series.handle_event(&event);
        }
        Focus::Rating => {
            state.rating.handle_event(&event);
        }
    }
    SearchAction::None
}

fn apply(state: &mut State) -> SearchAction {
    let rating = match state.rating.value().trim() {
        "" => None,
        s => match s.parse::<RatingRange>() {
            Ok(r) => Some(r),
            Err(msg) => {
                state.error = Some(msg);
                state.focus = Focus::Rating;
                return SearchAction::None;
            }
        },
    };

    let tags: Vec<String> = state
        .tags
        .value()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();

    let criteria = FilterCriteria {
        query: opt(state.query.value()),
        author: opt(state.author.value()),
        tags,
        series: opt(state.series.value()),
        rating,
    };

    if criteria.is_empty() {
        state.error = Some("enter at least one filter".to_string());
        state.focus = Focus::Query;
        return SearchAction::None;
    }
    SearchAction::Apply(criteria)
}

fn opt(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn format_rating(r: RatingRange) -> String {
    if r.min == r.max {
        format!("{}", r.min)
    } else {
        format!("{}..{}", r.min, r.max)
    }
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let target_w = area.width.saturating_mul(4) / 5;
    let w = target_w.max(50).min(area.width);
    let h = 13u16.min(area.height);
    let rect = centered_rect(w, h, area);
    frame.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" search ")
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // query
            Constraint::Length(1), // author
            Constraint::Length(1), // tags
            Constraint::Length(1), // series
            Constraint::Length(1), // rating
            Constraint::Length(1), // spacer
            Constraint::Length(1), // buttons
            Constraint::Length(1), // error
        ])
        .split(inner);

    let mut cursor_pos: Option<(u16, u16)> = None;
    let mut place_input =
        |frame: &mut Frame<'_>, area: Rect, label: &str, input: &Input, focused: bool| {
            render_field_row(frame, area, label, input.value(), focused);
            if focused {
                cursor_pos = Some(input_cursor(area, input));
            }
        };

    place_input(
        frame,
        layout[0],
        "Text",
        &state.query,
        state.focus == Focus::Query,
    );
    place_input(
        frame,
        layout[1],
        "Author",
        &state.author,
        state.focus == Focus::Author,
    );
    place_input(
        frame,
        layout[2],
        "Tags",
        &state.tags,
        state.focus == Focus::Tags,
    );
    place_input(
        frame,
        layout[3],
        "Series",
        &state.series,
        state.focus == Focus::Series,
    );
    place_input(
        frame,
        layout[4],
        "Rating",
        &state.rating,
        state.focus == Focus::Rating,
    );

    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(LEFT_PAD),
            Constraint::Length(13),
            Constraint::Length(2),
            Constraint::Length(13),
            Constraint::Min(0),
        ])
        .split(layout[6]);
    render_button(frame, buttons[1], "[ Apply ]", state.focus == Focus::Apply);
    render_button(
        frame,
        buttons[3],
        "[ Cancel ]",
        state.focus == Focus::Cancel,
    );

    if let Some(err) = &state.error {
        let p = Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                err.clone(),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
        ]));
        frame.render_widget(p, layout[7]);
    } else {
        let p = Paragraph::new(Line::from(Span::styled(
            "  whitespace in Text = AND tokens · tags comma-separated · rating N or MIN..MAX",
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(p, layout[7]);
    }

    if let Some((cx, cy)) = cursor_pos {
        frame.set_cursor_position((cx, cy));
    }
}

fn render_field_row(frame: &mut Frame<'_>, area: Rect, label: &str, value: &str, focused: bool) {
    let label_style = if focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let value_style = if focused {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::Gray)
    };
    let line = Line::from(vec![
        Span::raw(" ".repeat(LEFT_PAD as usize)),
        Span::styled(
            format!("{label:<width$}", width = LABEL_COL_WIDTH as usize),
            label_style,
        ),
        Span::styled(value.to_string(), value_style),
    ]);
    let p = if focused {
        Paragraph::new(line).style(Style::default().bg(Color::DarkGray))
    } else {
        Paragraph::new(line)
    };
    frame.render_widget(p, area);
}

fn render_button(frame: &mut Frame<'_>, area: Rect, label: &str, focused: bool) {
    let style = if focused {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let p = Paragraph::new(Span::styled(label, style)).alignment(Alignment::Center);
    frame.render_widget(p, area);
}

fn input_cursor(area: Rect, input: &Input) -> (u16, u16) {
    let visible_width = area.width.saturating_sub(LEFT_PAD + LABEL_COL_WIDTH).max(1) as usize;
    let scroll = input.visual_scroll(visible_width);
    let cursor = input.visual_cursor();
    let x = area.x + LEFT_PAD + LABEL_COL_WIDTH + cursor.saturating_sub(scroll) as u16;
    (x.min(area.x + area.width.saturating_sub(1)), area.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn type_text(state: &mut State, text: &str) {
        for ch in text.chars() {
            handle_key(state, key(KeyCode::Char(ch)));
        }
    }

    #[test]
    fn from_filter_none_is_empty() {
        let s = State::from_filter(None);
        assert_eq!(s.query.value(), "");
        assert_eq!(s.author.value(), "");
        assert_eq!(s.focus, Focus::Query);
    }

    #[test]
    fn from_quick_filter_prefills_only_text() {
        let active = ActiveFilter {
            criteria: FilterCriteria {
                query: Some("dune".to_string()),
                ..FilterCriteria::default()
            },
            kind: FilterKind::Quick,
        };
        let s = State::from_filter(Some(&active));
        assert_eq!(s.query.value(), "dune");
        assert_eq!(s.author.value(), "");
        assert_eq!(s.tags.value(), "");
    }

    #[test]
    fn from_advanced_filter_prefills_all_fields() {
        let active = ActiveFilter {
            criteria: FilterCriteria {
                query: Some("space".to_string()),
                author: Some("Herbert".to_string()),
                tags: vec!["sci-fi".to_string(), "classic".to_string()],
                series: Some("Dune".to_string()),
                rating: Some(RatingRange { min: 3, max: 5 }),
            },
            kind: FilterKind::Advanced,
        };
        let s = State::from_filter(Some(&active));
        assert_eq!(s.query.value(), "space");
        assert_eq!(s.author.value(), "Herbert");
        assert_eq!(s.tags.value(), "sci-fi, classic");
        assert_eq!(s.series.value(), "Dune");
        assert_eq!(s.rating.value(), "3..5");
    }

    #[test]
    fn apply_builds_criteria_with_split_tags() {
        let mut s = State::from_filter(None);
        s.focus = Focus::Tags;
        type_text(&mut s, "horror, gothic ,");
        s.focus = Focus::Apply;
        match apply(&mut s) {
            SearchAction::Apply(c) => {
                assert_eq!(c.tags, vec!["horror".to_string(), "gothic".to_string()]);
                assert!(c.query.is_none());
            }
            _ => panic!("expected Apply"),
        }
    }

    #[test]
    fn apply_parses_rating_range() {
        let mut s = State::from_filter(None);
        s.focus = Focus::Rating;
        type_text(&mut s, "4");
        s.focus = Focus::Apply;
        match apply(&mut s) {
            SearchAction::Apply(c) => {
                assert_eq!(c.rating, Some(RatingRange { min: 4, max: 4 }));
            }
            _ => panic!("expected Apply"),
        }
    }

    #[test]
    fn apply_with_invalid_rating_sets_error() {
        let mut s = State::from_filter(None);
        s.focus = Focus::Rating;
        type_text(&mut s, "nine");
        s.focus = Focus::Apply;
        assert!(matches!(apply(&mut s), SearchAction::None));
        assert!(s.error.is_some());
        assert_eq!(s.focus, Focus::Rating);
    }

    #[test]
    fn apply_with_all_empty_sets_error() {
        let mut s = State::from_filter(None);
        s.focus = Focus::Apply;
        assert!(matches!(apply(&mut s), SearchAction::None));
        assert!(s.error.is_some());
    }

    #[test]
    fn esc_cancels() {
        let mut s = State::from_filter(None);
        assert!(matches!(
            handle_key(&mut s, key(KeyCode::Esc)),
            SearchAction::Cancel
        ));
    }

    #[test]
    fn enter_advances_field_without_applying() {
        let mut s = State::from_filter(None);
        s.focus = Focus::Query;
        type_text(&mut s, "dune");
        let action = handle_key(&mut s, key(KeyCode::Enter));
        assert!(matches!(action, SearchAction::None));
        assert_eq!(s.focus, Focus::Author, "plain Enter advances to next field");
    }

    #[test]
    fn ctrl_s_applies_from_any_field() {
        let mut s = State::from_filter(None);
        s.focus = Focus::Query;
        type_text(&mut s, "dune");
        let action = handle_key(
            &mut s,
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
        );
        match action {
            SearchAction::Apply(c) => assert_eq!(c.query.as_deref(), Some("dune")),
            _ => panic!("expected Apply from Ctrl+S"),
        }
    }

    #[test]
    fn ctrl_enter_also_applies() {
        let mut s = State::from_filter(None);
        s.focus = Focus::Query;
        type_text(&mut s, "dune");
        let action = handle_key(&mut s, KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
        assert!(matches!(action, SearchAction::Apply(_)));
    }
}
