use std::path::Path;

use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

use crate::catalog::books::{self, Book, BookUpdate};
use crate::catalog::tags;
use crate::tui::widgets::{centered_rect, is_submit_key};

const LABEL_COL_WIDTH: u16 = 14;
const LEFT_PAD: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Table,
    Inspect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Title,
    Author,
    Tags,
    SeriesName,
    SeriesIndex,
    Rating,
    PublishedDate,
    Publisher,
    Language,
    Isbn,
    Description,
    Save,
    Cancel,
}

impl Focus {
    const ORDER: [Focus; 13] = [
        Focus::Title,
        Focus::Author,
        Focus::Tags,
        Focus::SeriesName,
        Focus::SeriesIndex,
        Focus::Rating,
        Focus::PublishedDate,
        Focus::Publisher,
        Focus::Language,
        Focus::Isbn,
        Focus::Description,
        Focus::Save,
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
    pub id: i64,
    pub origin: Origin,
    pub title: Input,
    pub author: Input,
    pub tags: Input,
    pub series_name: Input,
    pub series_index: Input,
    pub rating: u8, // 0..=5; 0 means "unrated" (saved as None).
    pub published_date: Input,
    pub publisher: Input,
    pub language: Input,
    pub isbn: Input,
    pub description: Input,
    pub focus: Focus,
    pub error: Option<String>,
}

impl State {
    pub fn from_book(book: &Book, origin: Origin) -> Self {
        Self {
            id: book.id,
            origin,
            title: Input::default().with_value(book.title.clone()),
            author: Input::default().with_value(book.author.clone().unwrap_or_default()),
            tags: Input::default().with_value(book.tags.join(", ")),
            series_name: Input::default().with_value(book.series_name.clone().unwrap_or_default()),
            series_index: Input::default()
                .with_value(book.series_index.map(format_index).unwrap_or_default()),
            rating: book.rating.unwrap_or(0).min(5),
            published_date: Input::default()
                .with_value(book.published_date.clone().unwrap_or_default()),
            publisher: Input::default().with_value(book.publisher.clone().unwrap_or_default()),
            language: Input::default().with_value(book.language.clone().unwrap_or_default()),
            isbn: Input::default().with_value(book.isbn.clone().unwrap_or_default()),
            description: Input::default().with_value(book.description.clone().unwrap_or_default()),
            focus: Focus::Title,
            error: None,
        }
    }
}

pub enum EditAction {
    None,
    Cancel,
    Saved(Box<Book>),
}

// While the edit overlay is open, swallow `q`/`Ctrl+C` so the user can't lose
// their edits by hitting an exit key — even when focus is on Save/Cancel.
pub fn captures_text_input(_state: &State) -> bool {
    true
}

pub fn handle_key(state: &mut State, key: KeyEvent, catalog_dir: &Path) -> EditAction {
    // Ctrl+S (or Ctrl+Enter where the terminal supports it) is the project-wide
    // form submit: save from any field.
    if is_submit_key(&key) {
        return submit(state, catalog_dir);
    }

    // Rating is special: ←/→ and digit keys, not text input.
    if state.focus == Focus::Rating {
        match key.code {
            KeyCode::Esc => return EditAction::Cancel,
            KeyCode::Tab | KeyCode::Down => {
                state.focus = state.focus.next();
                return EditAction::None;
            }
            KeyCode::BackTab | KeyCode::Up => {
                state.focus = state.focus.prev();
                return EditAction::None;
            }
            KeyCode::Enter => {
                state.focus = state.focus.next();
                return EditAction::None;
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Backspace => {
                state.rating = state.rating.saturating_sub(1);
                return EditAction::None;
            }
            KeyCode::Right | KeyCode::Char('l') => {
                state.rating = (state.rating + 1).min(5);
                return EditAction::None;
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let v = c.to_digit(10).unwrap_or(0) as u8;
                if v <= 5 {
                    state.rating = v;
                }
                return EditAction::None;
            }
            _ => return EditAction::None,
        }
    }

    match key.code {
        KeyCode::Esc => return EditAction::Cancel,
        KeyCode::Tab => {
            state.focus = state.focus.next();
            return EditAction::None;
        }
        KeyCode::BackTab => {
            state.focus = state.focus.prev();
            return EditAction::None;
        }
        KeyCode::Up => {
            state.focus = state.focus.prev();
            return EditAction::None;
        }
        KeyCode::Down => {
            state.focus = state.focus.next();
            return EditAction::None;
        }
        KeyCode::Enter => match state.focus {
            Focus::Save => return submit(state, catalog_dir),
            Focus::Cancel => return EditAction::Cancel,
            _ => {
                state.focus = state.focus.next();
                return EditAction::None;
            }
        },
        _ => {}
    }

    let event = Event::Key(key);
    match state.focus {
        Focus::Save | Focus::Cancel | Focus::Rating => {}
        Focus::Title => {
            state.title.handle_event(&event);
        }
        Focus::Author => {
            state.author.handle_event(&event);
        }
        Focus::Tags => {
            state.tags.handle_event(&event);
        }
        Focus::SeriesName => {
            state.series_name.handle_event(&event);
        }
        Focus::SeriesIndex => {
            state.series_index.handle_event(&event);
        }
        Focus::PublishedDate => {
            state.published_date.handle_event(&event);
        }
        Focus::Publisher => {
            state.publisher.handle_event(&event);
        }
        Focus::Language => {
            state.language.handle_event(&event);
        }
        Focus::Isbn => {
            state.isbn.handle_event(&event);
        }
        Focus::Description => {
            state.description.handle_event(&event);
        }
    }
    EditAction::None
}

fn submit(state: &mut State, catalog_dir: &Path) -> EditAction {
    let series_index = match state.series_index.value().trim() {
        "" => None,
        s => match s.parse::<f64>() {
            Ok(v) => Some(v),
            Err(_) => {
                state.error = Some(format!("series_index must be a number (got `{s}`)"));
                state.focus = Focus::SeriesIndex;
                return EditAction::None;
            }
        },
    };
    let rating = if state.rating == 0 {
        None
    } else {
        Some(state.rating)
    };

    let update = BookUpdate {
        title: state.title.value().to_string(),
        author: opt(state.author.value()),
        description: opt(state.description.value()),
        series_name: opt(state.series_name.value()),
        series_index,
        rating,
        isbn: opt(state.isbn.value()),
        publisher: opt(state.publisher.value()),
        language: opt(state.language.value()),
        published_date: opt(state.published_date.value()),
        tags: tags::normalize(state.tags.value()),
    };

    let mut conn = match crate::catalog::open_existing(catalog_dir) {
        Ok(c) => c,
        Err(err) => {
            state.error = Some(err.to_string());
            return EditAction::None;
        }
    };
    match books::handle_update(&mut conn, catalog_dir, state.id, update) {
        Ok(book) => EditAction::Saved(Box::new(book)),
        Err(err) => {
            state.error = Some(err.to_string());
            if let books::Error::Validation { field, .. } = &err {
                state.focus = match *field {
                    "title" => Focus::Title,
                    "rating" => Focus::Rating,
                    _ => state.focus,
                };
            }
            EditAction::None
        }
    }
}

fn opt(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn format_index(idx: f64) -> String {
    if idx.fract() == 0.0 {
        format!("{}", idx as i64)
    } else {
        format!("{idx}")
    }
}

fn stars(value: u8) -> String {
    let v = value.min(5) as usize;
    let mut s = String::new();
    for i in 0..5 {
        s.push(if i < v { '★' } else { '☆' });
    }
    s
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let target_w = area.width.saturating_mul(4) / 5;
    let target_h = area.height.saturating_mul(9) / 10;
    let w = target_w.max(60).min(area.width);
    let h = target_h.max(22).min(area.height);
    let rect = centered_rect(w, h, area);
    frame.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" edit ")
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title
            Constraint::Length(1), // author
            Constraint::Length(1), // tags
            Constraint::Length(1), // spacer
            Constraint::Length(1), // series
            Constraint::Length(1), // index
            Constraint::Length(1), // rating
            Constraint::Length(1), // spacer
            Constraint::Length(1), // publisher
            Constraint::Length(1), // language
            Constraint::Length(1), // published
            Constraint::Length(1), // isbn
            Constraint::Length(1), // spacer
            Constraint::Length(1), // description label
            Constraint::Min(2),    // description body
            Constraint::Length(1), // spacer
            Constraint::Length(1), // buttons
            Constraint::Length(1), // error / status
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
        "Title",
        &state.title,
        state.focus == Focus::Title,
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
        layout[4],
        "Series",
        &state.series_name,
        state.focus == Focus::SeriesName,
    );
    place_input(
        frame,
        layout[5],
        "Index",
        &state.series_index,
        state.focus == Focus::SeriesIndex,
    );
    render_rating_row(frame, layout[6], state.rating, state.focus == Focus::Rating);

    place_input(
        frame,
        layout[8],
        "Publisher",
        &state.publisher,
        state.focus == Focus::Publisher,
    );
    place_input(
        frame,
        layout[9],
        "Language",
        &state.language,
        state.focus == Focus::Language,
    );
    place_input(
        frame,
        layout[10],
        "Published",
        &state.published_date,
        state.focus == Focus::PublishedDate,
    );
    place_input(
        frame,
        layout[11],
        "ISBN",
        &state.isbn,
        state.focus == Focus::Isbn,
    );

    // Description: label row + body block.
    let desc_focused = state.focus == Focus::Description;
    render_field_row(frame, layout[13], "Description", "", desc_focused);
    render_description_body(frame, layout[14], &state.description, desc_focused);
    if desc_focused {
        cursor_pos = Some(description_cursor(layout[14], &state.description));
    }

    // Buttons row.
    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(LEFT_PAD),
            Constraint::Length(12),
            Constraint::Length(2),
            Constraint::Length(12),
            Constraint::Min(0),
        ])
        .split(layout[16]);
    render_button(frame, buttons[1], "[ Save ]", state.focus == Focus::Save);
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
        frame.render_widget(p, layout[17]);
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

fn render_rating_row(frame: &mut Frame<'_>, area: Rect, value: u8, focused: bool) {
    let label_style = if focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let star_style = Style::default().fg(Color::Yellow).add_modifier(if focused {
        Modifier::BOLD
    } else {
        Modifier::empty()
    });
    let suffix = if value == 0 {
        " (unrated)".to_string()
    } else {
        format!(" ({value}/5)")
    };
    let suffix_style = if focused {
        Style::default().fg(Color::Gray)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let hint = if focused {
        "   ←/→ adjust · 0-5 set"
    } else {
        ""
    };
    let hint_style = Style::default().fg(Color::DarkGray);
    let line = Line::from(vec![
        Span::raw(" ".repeat(LEFT_PAD as usize)),
        Span::styled(
            format!(
                "{label:<width$}",
                label = "Rating",
                width = LABEL_COL_WIDTH as usize
            ),
            label_style,
        ),
        Span::styled(stars(value), star_style),
        Span::styled(suffix, suffix_style),
        Span::styled(hint, hint_style),
    ]);
    let p = if focused {
        Paragraph::new(line).style(Style::default().bg(Color::DarkGray))
    } else {
        Paragraph::new(line)
    };
    frame.render_widget(p, area);
}

fn render_description_body(frame: &mut Frame<'_>, area: Rect, input: &Input, focused: bool) {
    let inner_x_offset = LEFT_PAD + 2; // indent body slightly past label column
    let body_area = Rect {
        x: area.x.saturating_add(inner_x_offset),
        y: area.y,
        width: area.width.saturating_sub(inner_x_offset + 2),
        height: area.height,
    };

    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);
    let inside = block.inner(body_area);
    frame.render_widget(block, body_area);

    let text_style = if focused {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::Gray)
    };
    let p = Paragraph::new(Span::styled(input.value().to_string(), text_style))
        .wrap(Wrap { trim: false });
    frame.render_widget(p, inside);
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
    // Cursor sits at: left_pad + label_col_width + visual_cursor_offset.
    let visible_width = area.width.saturating_sub(LEFT_PAD + LABEL_COL_WIDTH).max(1) as usize;
    let scroll = input.visual_scroll(visible_width);
    let cursor = input.visual_cursor();
    let x = area.x + LEFT_PAD + LABEL_COL_WIDTH + cursor.saturating_sub(scroll) as u16;
    (x.min(area.x + area.width.saturating_sub(1)), area.y)
}

fn description_cursor(parent_area: Rect, input: &Input) -> (u16, u16) {
    // The body is drawn as a bordered box at offset (LEFT_PAD + 2, 0) inside
    // `parent_area`, with width = parent_area.width - (LEFT_PAD + 2) - 2. The
    // text inside the border starts at x+1, y+1 and has width box_width - 2.
    let outer_x_offset: u16 = LEFT_PAD + 2;
    let outer_w = parent_area.width.saturating_sub(outer_x_offset + 2);
    let inner_x = parent_area.x.saturating_add(outer_x_offset + 1);
    let inner_y = parent_area.y.saturating_add(1);
    let inner_w = outer_w.saturating_sub(2).max(1) as usize;
    let value = input.value();
    let cursor = input.visual_cursor();
    let before: String = value.chars().take(cursor).collect();
    let mut col = 0usize;
    let mut row = 0usize;
    for ch in before.chars() {
        if ch == '\n' || col >= inner_w {
            row += 1;
            col = 0;
        }
        if ch != '\n' {
            col += 1;
        }
    }
    let cx = inner_x.saturating_add(col as u16);
    let cy = inner_y.saturating_add(row as u16);
    (cx, cy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::books::EmbedStatus;
    use crossterm::event::KeyModifiers;
    use tempfile::tempdir;

    fn sample_book() -> Book {
        Book {
            id: 1,
            title: "Dune".to_string(),
            author: Some("Herbert".to_string()),
            format: "epub".to_string(),
            file_path: "books/1/dune.epub".to_string(),
            added_at: "2024-01-01".to_string(),
            description: None,
            series_name: None,
            series_index: None,
            rating: None,
            isbn: None,
            publisher: None,
            language: None,
            published_date: None,
            tags: vec![],
            embed_status: EmbedStatus::Pending,
            embed_synced_at: None,
        }
    }

    #[test]
    fn enter_advances_focus_on_text_field() {
        let mut s = State::from_book(&sample_book(), Origin::Table);
        s.focus = Focus::Title;
        let dir = tempdir().unwrap();
        let action = handle_key(
            &mut s,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            dir.path(),
        );
        assert!(matches!(action, EditAction::None));
        assert_eq!(s.focus, Focus::Author, "plain Enter advances to next field");
    }

    #[test]
    fn ctrl_s_routes_to_save_not_focus_advance() {
        let mut s = State::from_book(&sample_book(), Origin::Table);
        s.focus = Focus::Title;
        // An empty dir has no catalog, so the save attempt surfaces an error
        // instead of advancing focus — proof that Ctrl+S routed to submit.
        let dir = tempdir().unwrap();
        let action = handle_key(
            &mut s,
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
            dir.path(),
        );
        assert!(matches!(action, EditAction::None));
        assert!(s.error.is_some(), "Ctrl+S must attempt to save");
        assert_eq!(s.focus, Focus::Title, "focus must not advance on Ctrl+S");
    }

    #[test]
    fn ctrl_enter_also_routes_to_save() {
        let mut s = State::from_book(&sample_book(), Origin::Table);
        s.focus = Focus::Title;
        let dir = tempdir().unwrap();
        let action = handle_key(
            &mut s,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            dir.path(),
        );
        assert!(matches!(action, EditAction::None));
        assert!(s.error.is_some(), "Ctrl+Enter must attempt to save");
        assert_eq!(s.focus, Focus::Title);
    }

    #[test]
    fn ctrl_enter_submits_even_from_rating_field() {
        let mut s = State::from_book(&sample_book(), Origin::Table);
        s.focus = Focus::Rating;
        let dir = tempdir().unwrap();
        let action = handle_key(
            &mut s,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            dir.path(),
        );
        assert!(matches!(action, EditAction::None));
        assert!(s.error.is_some());
        assert_eq!(s.focus, Focus::Rating);
    }
}
