use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::widgets::centered_rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Ok,
    Cancel,
}

#[derive(Debug, Clone)]
pub struct State {
    pub title: String,
    pub message: String,
    pub ok_label: String,
    pub cancel_label: String,
    pub focus: Button,
}

impl State {
    // The quit guard: OK is focused by default so a bare Enter confirms.
    pub fn quit() -> Self {
        Self {
            title: "quit".to_string(),
            message: "Quit codex?".to_string(),
            ok_label: "[ OK ]".to_string(),
            cancel_label: "[ Cancel ]".to_string(),
            focus: Button::Ok,
        }
    }
}

pub enum ConfirmAction {
    None,
    Confirm,
    Cancel,
}

pub fn handle_key(state: &mut State, key: KeyEvent) -> ConfirmAction {
    match key.code {
        KeyCode::Esc => ConfirmAction::Cancel,
        KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
            state.focus = match state.focus {
                Button::Ok => Button::Cancel,
                Button::Cancel => Button::Ok,
            };
            ConfirmAction::None
        }
        KeyCode::Enter => match state.focus {
            Button::Ok => ConfirmAction::Confirm,
            Button::Cancel => ConfirmAction::Cancel,
        },
        _ => ConfirmAction::None,
    }
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let width = (state.message.chars().count() + 6)
        .max(state.title.len() + 4)
        .max(state.ok_label.chars().count() + state.cancel_label.chars().count() + 8);
    let width = u16::try_from(width).unwrap_or(u16::MAX).min(area.width);
    let height = 5u16.min(area.height);
    let rect = centered_rect(width, height, area);

    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", state.title))
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // message
            Constraint::Length(1), // spacer
            Constraint::Length(1), // buttons
        ])
        .split(inner);

    let message =
        Paragraph::new(Line::from(Span::raw(state.message.clone()))).alignment(Alignment::Center);
    frame.render_widget(message, layout[0]);

    let buttons = Line::from(vec![
        button_span(&state.ok_label, state.focus == Button::Ok),
        Span::raw("   "),
        button_span(&state.cancel_label, state.focus == Button::Cancel),
    ]);
    let p = Paragraph::new(buttons).alignment(Alignment::Center);
    frame.render_widget(p, layout[2]);
}

fn button_span(label: &str, focused: bool) -> Span<'_> {
    let style = if focused {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    Span::styled(label.to_string(), style)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn quit_focuses_ok_by_default() {
        let s = State::quit();
        assert_eq!(s.focus, Button::Ok);
    }

    #[test]
    fn enter_on_ok_confirms() {
        let mut s = State::quit();
        assert!(matches!(
            handle_key(&mut s, key(KeyCode::Enter)),
            ConfirmAction::Confirm
        ));
    }

    #[test]
    fn enter_on_cancel_cancels() {
        let mut s = State::quit();
        s.focus = Button::Cancel;
        assert!(matches!(
            handle_key(&mut s, key(KeyCode::Enter)),
            ConfirmAction::Cancel
        ));
    }

    #[test]
    fn esc_cancels() {
        let mut s = State::quit();
        assert!(matches!(
            handle_key(&mut s, key(KeyCode::Esc)),
            ConfirmAction::Cancel
        ));
    }

    #[test]
    fn arrows_and_tab_toggle_focus() {
        let mut s = State::quit();
        handle_key(&mut s, key(KeyCode::Right));
        assert_eq!(s.focus, Button::Cancel);
        handle_key(&mut s, key(KeyCode::Left));
        assert_eq!(s.focus, Button::Ok);
        handle_key(&mut s, key(KeyCode::Tab));
        assert_eq!(s.focus, Button::Cancel);
        handle_key(&mut s, key(KeyCode::BackTab));
        assert_eq!(s.focus, Button::Ok);
    }
}
