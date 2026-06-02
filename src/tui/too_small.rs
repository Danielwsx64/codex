use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use crate::tui::widgets::outer_block;
use crate::welcome;

pub const MIN_COLS: u16 = 90;
pub const MIN_ROWS: u16 = 25;

pub fn is_too_small(area: Rect) -> bool {
    area.width < MIN_COLS || area.height < MIN_ROWS
}

pub fn render(frame: &mut Frame<'_>, area: Rect) {
    // Emergency render: terminal is too narrow/short even for the outer block.
    // Skip the block, just show a minimal message.
    if area.width < 20 || area.height < 3 {
        frame.render_widget(Clear, area);
        let line = Line::from(Span::styled(
            "Too small",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
        let paragraph = Paragraph::new(line).alignment(Alignment::Center);
        frame.render_widget(paragraph, area);
        return;
    }

    let block = outer_block("codex");
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line<'static>> = Vec::new();

    let logo_lines: Vec<&str> = welcome::ART.lines().collect();
    let logo_w = logo_lines
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0) as u16;
    let logo_h = logo_lines.len() as u16;
    let body_min_h: u16 = 8;
    let show_logo = logo_w <= inner.width && logo_h.saturating_add(body_min_h) <= inner.height;

    if show_logo {
        for l in &logo_lines {
            lines.push(Line::from(Span::raw((*l).to_string())));
        }
        lines.push(Line::from(""));
    }

    lines.push(Line::from(Span::styled(
        "Terminal too small",
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::raw(format!(
        "Minimum size: {MIN_COLS} cols x {MIN_ROWS} rows"
    ))));
    lines.push(Line::from(Span::styled(
        format!("Current size: {} cols x {} rows", area.width, area.height),
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Resize the terminal, or use the cdx CLI.",
        Style::default().fg(Color::Gray),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Press q or Ctrl+C to quit.",
        Style::default().fg(Color::DarkGray),
    )));

    let total = u16::try_from(lines.len()).unwrap_or(u16::MAX);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(total.min(inner.height)),
            Constraint::Min(0),
        ])
        .split(inner);

    let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(paragraph, chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(w: u16, h: u16) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: w,
            height: h,
        }
    }

    #[test]
    fn detects_too_narrow() {
        assert!(is_too_small(rect(MIN_COLS - 1, MIN_ROWS)));
    }

    #[test]
    fn detects_too_short() {
        assert!(is_too_small(rect(MIN_COLS, MIN_ROWS - 1)));
    }

    #[test]
    fn minimum_is_acceptable() {
        assert!(!is_too_small(rect(MIN_COLS, MIN_ROWS)));
    }

    #[test]
    fn larger_is_acceptable() {
        assert!(!is_too_small(rect(MIN_COLS + 10, MIN_ROWS + 10)));
    }
}
