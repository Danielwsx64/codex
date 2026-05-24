use std::path::{Path, PathBuf};

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::catalog::books::Book;
use crate::embed::{self, EmbedOutcome};
use crate::import::Format;
use crate::tui::widgets::centered_rect;

#[derive(Debug, Clone)]
pub struct Item {
    pub id: i64,
    pub title: String,
    pub abs_path: PathBuf,
    pub format: Format,
    pub book: Book,
}

#[derive(Debug, Clone)]
pub struct Failure {
    pub title: String,
    pub reason: String,
}

#[derive(Debug)]
pub struct State {
    pub total: usize,
    pub completed: usize,
    pub succeeded: usize,
    pub queue: Vec<Item>,
    pub current: Option<Item>,
    pub failures: Vec<Failure>,
    pub done: bool,
}

impl State {
    pub fn from_rows(rows: &[Book], catalog_dir: &Path) -> Self {
        let total = rows.len();
        let mut queue: Vec<Item> = Vec::new();
        let mut failures: Vec<Failure> = Vec::new();
        for b in rows {
            let abs_path = catalog_dir.join(&b.file_path);
            match Format::parse_label(&b.format) {
                Some(format @ (Format::Epub | Format::Pdf)) => {
                    queue.push(Item {
                        id: b.id,
                        title: b.title.clone(),
                        abs_path,
                        format,
                        book: b.clone(),
                    });
                }
                Some(other) => failures.push(Failure {
                    title: b.title.clone(),
                    reason: format!("embed not supported for {}", other.label()),
                }),
                None => failures.push(Failure {
                    title: b.title.clone(),
                    reason: format!("unknown format `{}`", b.format),
                }),
            }
        }
        // pre-counted failures are already "completed" — they contribute to the bar.
        let completed = failures.len();
        // queue is consumed back-to-front; reverse so we visit by display order.
        queue.reverse();
        let done = queue.is_empty();
        let current = queue.last().cloned();
        Self {
            total,
            completed,
            succeeded: 0,
            queue,
            current,
            failures,
            done,
        }
    }

    pub fn is_pending(&self) -> bool {
        !self.done
    }
}

pub fn advance(state: &mut State) {
    if state.done {
        return;
    }
    let Some(item) = state.queue.pop() else {
        state.done = true;
        state.current = None;
        return;
    };
    let result = embed::embed_into_file(&item.abs_path, item.format, &item.book);
    match result {
        Ok(EmbedOutcome::Written) => {
            state.succeeded += 1;
        }
        Ok(EmbedOutcome::Unsupported { format }) => {
            state.failures.push(Failure {
                title: item.title.clone(),
                reason: format!("embed not supported for {}", format.label()),
            });
        }
        Err(err) => {
            state.failures.push(Failure {
                title: item.title.clone(),
                reason: err.to_string(),
            });
        }
    }
    state.completed += 1;
    // The next pending item becomes the "current" hint for the next draw cycle.
    state.current = state.queue.last().cloned();
    if state.queue.is_empty() {
        state.done = true;
    }
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let target_w = area.width.saturating_mul(4) / 5;
    let target_h = area.height.saturating_mul(4) / 5;
    let w = target_w.max(50).min(area.width);
    let h = target_h.max(12).min(area.height);
    let rect = centered_rect(w, h, area);
    frame.render_widget(Clear, rect);

    let title = if state.done {
        " embed: done "
    } else {
        " embed: running "
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

    let status_line = match (&state.current, state.done) {
        (_, true) => Line::from(vec![
            Span::styled(
                "Done — ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                "{succeeded} embedded, {failed} failed",
                succeeded = state.succeeded,
                failed = state.failures.len()
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
        format!("{} / {}", state.completed, state.total),
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(counters, layout[1]);

    let ratio = if state.total == 0 {
        1.0
    } else {
        state.completed as f64 / state.total as f64
    };
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(Color::Cyan).bg(Color::Black))
        .ratio(ratio.clamp(0.0, 1.0));
    frame.render_widget(gauge, layout[2]);

    let failures_label = if state.failures.is_empty() {
        "Failures: none"
    } else {
        "Failures:"
    };
    let label_style = if state.failures.is_empty() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(failures_label, label_style))),
        layout[4],
    );

    if !state.failures.is_empty() {
        let items: Vec<ListItem<'_>> = state
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
        let list = List::new(items);
        frame.render_widget(list, layout[5]);
    }

    let hint = if state.done {
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
