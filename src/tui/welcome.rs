use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::help::{Binding, Section as HelpSection};
use crate::welcome;

const WELCOME_BINDINGS: &[Binding] = &[
    Binding {
        keys: "↑↓ / j k",
        desc: "navigate sections",
    },
    Binding {
        keys: "Enter",
        desc: "open selected section",
    },
];

pub fn help_sections(_state: &State) -> Vec<HelpSection> {
    vec![HelpSection {
        title: "Welcome",
        bindings: WELCOME_BINDINGS,
    }]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Library,
    Catalogs,
    Search,
    Devices,
}

impl Section {
    pub const ALL: [Section; 4] = [
        Section::Library,
        Section::Catalogs,
        Section::Search,
        Section::Devices,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Section::Library => "Library",
            Section::Catalogs => "Catalogs",
            Section::Search => "Search",
            Section::Devices => "Devices",
        }
    }

    pub fn milestone(self) -> Option<&'static str> {
        match self {
            Section::Library | Section::Catalogs => None,
            Section::Search => Some("v0.3"),
            Section::Devices => Some("v0.4"),
        }
    }

    pub fn enabled(self) -> bool {
        self.milestone().is_none()
    }
}

#[derive(Debug, Clone)]
pub struct State {
    pub cursor: usize,
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

impl State {
    pub fn new() -> Self {
        Self {
            cursor: Self::first_enabled(),
        }
    }

    fn first_enabled() -> usize {
        Section::ALL
            .iter()
            .position(|s| s.enabled())
            .expect("at least one welcome section is always enabled")
    }

    pub fn selected(&self) -> Section {
        Section::ALL[self.cursor]
    }

    fn move_cursor(&mut self, delta: isize) {
        let len = Section::ALL.len() as isize;
        let mut idx = self.cursor as isize;
        for _ in 0..len {
            idx = (idx + delta).rem_euclid(len);
            if Section::ALL[idx as usize].enabled() {
                self.cursor = idx as usize;
                return;
            }
        }
    }
}

pub enum WelcomeAction {
    None,
    Enter(Section),
    OpenPalette,
}

pub fn handle_key(state: &mut State, key: KeyEvent) -> WelcomeAction {
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            state.move_cursor(1);
            WelcomeAction::None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.move_cursor(-1);
            WelcomeAction::None
        }
        KeyCode::Enter => {
            let sec = state.selected();
            if sec.enabled() {
                WelcomeAction::Enter(sec)
            } else {
                WelcomeAction::None
            }
        }
        KeyCode::Char(':') => WelcomeAction::OpenPalette,
        _ => WelcomeAction::None,
    }
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let mut header_lines: Vec<Line<'static>> = welcome::ART
        .lines()
        .map(|l| Line::from(Span::raw(l)))
        .collect();
    header_lines.push(Line::from(""));
    header_lines.push(Line::from(Span::styled(
        format!("codex v{}", welcome::version()),
        Style::default().add_modifier(Modifier::BOLD),
    )));
    header_lines.push(Line::from(Span::raw(welcome::TAGLINE)));
    header_lines.push(Line::from(""));

    let menu_lines: Vec<Line<'static>> = Section::ALL
        .iter()
        .enumerate()
        .map(|(idx, sec)| menu_line(*sec, idx == state.cursor))
        .collect();

    let header_height = u16::try_from(header_lines.len()).unwrap_or(u16::MAX);
    let menu_height = u16::try_from(menu_lines.len()).unwrap_or(u16::MAX);
    let total = header_height.saturating_add(menu_height).saturating_add(1);

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(total),
            Constraint::Min(0),
        ])
        .split(area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height),
            Constraint::Length(1),
            Constraint::Length(menu_height),
        ])
        .split(vertical[1]);

    let header = Paragraph::new(header_lines).alignment(Alignment::Center);
    frame.render_widget(header, inner[0]);
    let menu = Paragraph::new(menu_lines).alignment(Alignment::Center);
    frame.render_widget(menu, inner[2]);
}

fn menu_line(section: Section, selected: bool) -> Line<'static> {
    let arrow = if selected { "▶ " } else { "  " };
    let label = section.label();
    let suffix = match section.milestone() {
        Some(m) => format!(" ({m})"),
        None => String::new(),
    };
    let text = format!("{arrow}{label}{suffix}");
    let mut style = Style::default();
    if !section.enabled() {
        style = style.fg(Color::DarkGray);
    }
    if selected {
        style = style.add_modifier(Modifier::BOLD).fg(Color::Cyan);
    }
    Line::from(Span::styled(text, style))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn new_starts_at_first_enabled_section() {
        let s = State::new();
        assert_eq!(s.selected(), Section::Library);
    }

    #[test]
    fn down_skips_disabled_sections() {
        let mut s = State::new();
        handle_key(&mut s, key(KeyCode::Down));
        assert_eq!(s.selected(), Section::Catalogs);
        handle_key(&mut s, key(KeyCode::Down));
        // Search is disabled, must skip to next enabled. With Search+Devices disabled,
        // the next enabled wrapping forward is Library.
        assert_eq!(s.selected(), Section::Library);
    }

    #[test]
    fn up_wraps_to_last_enabled() {
        let mut s = State::new();
        handle_key(&mut s, key(KeyCode::Up));
        assert_eq!(s.selected(), Section::Catalogs);
    }

    #[test]
    fn enter_on_enabled_returns_navigate() {
        let mut s = State::new();
        let action = handle_key(&mut s, key(KeyCode::Enter));
        assert!(matches!(action, WelcomeAction::Enter(Section::Library)));
    }

    #[test]
    fn colon_opens_palette() {
        let mut s = State::new();
        let action = handle_key(&mut s, key(KeyCode::Char(':')));
        assert!(matches!(action, WelcomeAction::OpenPalette));
    }
}
