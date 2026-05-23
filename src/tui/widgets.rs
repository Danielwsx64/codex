use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub text: String,
    pub kind: StatusKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Error,
}

impl StatusMessage {
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: StatusKind::Info,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: StatusKind::Error,
        }
    }
}

pub fn outer_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .title(format!(" {title} "))
        .title_alignment(Alignment::Center)
}

pub fn render_status(frame: &mut Frame<'_>, area: Rect, status: &StatusMessage) {
    let style = match status.kind {
        StatusKind::Info => Style::default().fg(Color::Cyan),
        StatusKind::Error => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    };
    let paragraph = Paragraph::new(Line::from(Span::styled(status.text.clone(), style)))
        .alignment(Alignment::Left);
    frame.render_widget(paragraph, area);
}

pub fn render_default_footer(frame: &mut Frame<'_>, area: Rect, hint: &str) {
    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled(hint, Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled(":", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(" palette  ", Style::default().fg(Color::DarkGray)),
        Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(" quit ", Style::default().fg(Color::DarkGray)),
    ]);
    let paragraph = Paragraph::new(line).alignment(Alignment::Right);
    frame.render_widget(paragraph, area);
}

pub fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(width),
            Constraint::Min(0),
        ])
        .split(vertical[1]);
    horizontal[1]
}

pub fn render_modal(frame: &mut Frame<'_>, area: Rect, title: &str, lines: Vec<Line<'_>>) {
    let height = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2);
    let max_text_width = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.chars().count())
                .sum::<usize>()
        })
        .max()
        .unwrap_or(20);
    let width = u16::try_from(max_text_width.max(title.len() + 4) + 4).unwrap_or(u16::MAX);
    let rect = centered_rect(width.min(area.width), height.min(area.height), area);

    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {title} "))
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(paragraph, inner);
}
