use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::embed::job::Job;
use crate::tui::widgets::centered_rect;

pub use crate::embed::job::{Failure, Item};

pub fn render(frame: &mut Frame<'_>, area: Rect, job: &Job) {
    let target_w = area.width.saturating_mul(4) / 5;
    let target_h = area.height.saturating_mul(4) / 5;
    let w = target_w.max(50).min(area.width);
    let h = target_h.max(12).min(area.height);
    let rect = centered_rect(w, h, area);
    frame.render_widget(Clear, rect);

    let title = if job.done {
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

    let status_line = match (&job.current, job.done) {
        (_, true) => Line::from(vec![
            Span::styled(
                "Done — ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                "{succeeded} embedded, {failed} failed",
                succeeded = job.succeeded,
                failed = job.failures.len()
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
        let list = List::new(items);
        frame.render_widget(list, layout[5]);
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
