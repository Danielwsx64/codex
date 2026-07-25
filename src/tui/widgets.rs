use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

// Project-wide form convention: plain Enter moves between fields, the submit
// chord fires the form's primary action (Save / Apply / create). The canonical
// chord is Ctrl+S — it survives raw mode and terminal multiplexers (tmux,
// screen) everywhere. Ctrl+Enter is accepted too, but only terminals that
// negotiate keyboard enhancement (see TerminalGuard) can distinguish it from a
// bare Enter; most terminals and any tmux session cannot, which is why Ctrl+S
// is the one we document. The on-screen action button + Enter is the last-ditch
// fallback.
pub fn is_submit_key(key: &KeyEvent) -> bool {
    if !key.modifiers.contains(KeyModifiers::CONTROL) {
        return false;
    }
    matches!(
        key.code,
        KeyCode::Enter | KeyCode::Char('s') | KeyCode::Char('S')
    )
}

// The cursor row in every list and table. ratatui applies a highlight by
// patching each cell, so a style that sets no foreground leaves each span's own
// colour in place — and `DarkGray` is also our muted-text colour, which would
// then render invisible against this bar. Forcing the foreground is what keeps
// the selected row readable; the trade-off is that the semantic accents (green
// connected, cyan current, yellow modified) flatten to white on that one row.
pub fn selection_style() -> Style {
    Style::default()
        .fg(Color::White)
        .bg(Color::DarkGray)
        .add_modifier(Modifier::BOLD)
}

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

pub fn render_default_footer(frame: &mut Frame<'_>, area: Rect) {
    let dim = Style::default().fg(Color::DarkGray);
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let line = Line::from(vec![
        Span::styled("?", bold),
        Span::styled(" help  ", dim),
        Span::styled(":", bold),
        Span::styled(" command  ", dim),
        Span::styled("q", bold),
        Span::styled(" quit ", dim),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn k(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn submit_key_accepts_ctrl_s_and_ctrl_enter() {
        assert!(is_submit_key(&k(KeyCode::Char('s'), KeyModifiers::CONTROL)));
        assert!(is_submit_key(&k(KeyCode::Char('S'), KeyModifiers::CONTROL)));
        assert!(is_submit_key(&k(KeyCode::Enter, KeyModifiers::CONTROL)));
    }

    #[test]
    fn submit_key_rejects_plain_keys() {
        assert!(!is_submit_key(&k(KeyCode::Enter, KeyModifiers::NONE)));
        assert!(!is_submit_key(&k(KeyCode::Char('s'), KeyModifiers::NONE)));
        // Ctrl on an unrelated key is not a submit.
        assert!(!is_submit_key(&k(
            KeyCode::Char('a'),
            KeyModifiers::CONTROL
        )));
    }

    // The bug this guards: the highlight used to set only a background, so a
    // span already painted in the muted colour (which is the same DarkGray)
    // vanished into the bar. An explicit foreground is what prevents that, so a
    // future edit must not drop it or make the two colours meet again.
    #[test]
    fn selection_style_forces_a_foreground_that_contrasts_with_its_bar() {
        let style = selection_style();
        assert!(
            style.fg.is_some(),
            "without a foreground each span keeps its own and can match the bar"
        );
        assert_ne!(style.fg, style.bg);
        assert_ne!(style.fg, Some(Color::DarkGray), "DarkGray is muted text");
    }
}
