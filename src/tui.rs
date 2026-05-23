use std::io::{self, Stdout};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use ratatui::Terminal;

use crate::welcome;

pub fn run() -> Result<()> {
    let mut terminal = TerminalGuard::enter()?;
    event_loop(&mut terminal.terminal)?;
    Ok(())
}

fn event_loop<B: Backend>(terminal: &mut Terminal<B>) -> Result<()> {
    loop {
        terminal.draw(render_welcome)?;
        if let Event::Key(key) = event::read()? {
            if is_exit_key(&key) {
                return Ok(());
            }
        }
    }
}

fn is_exit_key(key: &KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('q') => true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => true,
        _ => false,
    }
}

pub fn render_welcome(frame: &mut Frame<'_>) {
    let area = frame.area();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" codex ")
        .title_alignment(Alignment::Center);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = welcome_lines();
    let height = u16::try_from(lines.len()).unwrap_or(u16::MAX);
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(inner);

    let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(paragraph, vertical[1]);

    render_footer(frame, inner);
}

fn render_footer(frame: &mut Frame<'_>, inner: Rect) {
    if inner.height < 2 {
        return;
    }
    let footer_area = Rect {
        x: inner.x,
        y: inner.y + inner.height - 1,
        width: inner.width,
        height: 1,
    };
    let footer = Paragraph::new(Line::from(vec![
        Span::raw(" press "),
        Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" or "),
        Span::styled("Ctrl+C", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" to quit "),
    ]))
    .alignment(Alignment::Right);
    frame.render_widget(footer, footer_area);
}

fn welcome_lines() -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = welcome::ART
        .lines()
        .map(|l| Line::from(Span::raw(l)))
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("codex v{}", welcome::version()),
        Style::default().add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::raw(welcome::TAGLINE)));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::raw(welcome::HINT)));
    lines
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    fn exit_keys_are_q_and_ctrl_c_only() {
        let q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let plain_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);

        assert!(is_exit_key(&q));
        assert!(is_exit_key(&ctrl_c));
        assert!(!is_exit_key(&esc));
        assert!(!is_exit_key(&enter));
        assert!(!is_exit_key(&plain_c));
    }

    #[test]
    fn render_welcome_shows_version_tagline_and_hint() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("TestBackend never fails to initialize");
        terminal
            .draw(render_welcome)
            .expect("TestBackend draw is infallible");

        let buffer = terminal.backend().buffer().clone();
        let rendered = buffer_to_string(&buffer);

        assert!(
            rendered.contains(&format!("codex v{}", welcome::version())),
            "version line missing in TUI frame; got:\n{rendered}"
        );
        assert!(
            rendered.contains("Terminal-first ebook library"),
            "tagline missing in TUI frame; got:\n{rendered}"
        );
        assert!(
            rendered.contains("cdx --help"),
            "help hint missing in TUI frame; got:\n{rendered}"
        );
        assert!(
            rendered.contains("press q or Ctrl+C to quit"),
            "exit-key footer missing; got:\n{rendered}"
        );
    }

    fn buffer_to_string(buffer: &ratatui::buffer::Buffer) -> String {
        let area = buffer.area();
        let mut out = String::with_capacity(usize::from(area.width + 1) * usize::from(area.height));
        for y in 0..area.height {
            for x in 0..area.width {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }
}
