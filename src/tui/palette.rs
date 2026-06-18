use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

pub const COMMANDS: &[&str] = &[
    ":library",
    ":catalogs",
    ":search",
    ":devices",
    ":duplicates",
    ":help",
    ":h",
    ":quit",
    ":q",
];

#[derive(Debug)]
pub struct State {
    pub input: Input,
    pub error: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

impl State {
    pub fn new() -> Self {
        Self {
            input: Input::default().with_value(":".to_string()),
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Library,
    Catalogs,
    Search,
    Devices,
    Duplicates,
    Help,
    Quit,
    /// Absolute page jump (1-indexed). Active only in the reader.
    PageJump(usize),
    /// Absolute chapter jump (1-indexed). Active only in the reader.
    ChapterJump(usize),
}

pub enum PaletteAction {
    None,
    Close,
    Execute(Command),
}

pub fn handle_key(state: &mut State, key: KeyEvent) -> PaletteAction {
    match key.code {
        KeyCode::Esc => PaletteAction::Close,
        KeyCode::Enter => {
            let value = state.input.value().trim().to_string();
            match parse(&value) {
                Some(cmd) => PaletteAction::Execute(cmd),
                None => {
                    state.error = Some(format!("unknown command `{value}`"));
                    PaletteAction::None
                }
            }
        }
        KeyCode::Tab => {
            if let Some(completed) = complete(state.input.value()) {
                state.input = Input::default().with_value(completed);
            }
            PaletteAction::None
        }
        _ => {
            state.error = None;
            let event = Event::Key(key);
            state.input.handle_event(&event);
            // Reset to ":" if user erased everything (keeps the prefix visible).
            if state.input.value().is_empty() {
                state.input = Input::default().with_value(":".to_string());
            }
            PaletteAction::None
        }
    }
}

pub fn parse(input: &str) -> Option<Command> {
    match input {
        ":library" => Some(Command::Library),
        ":catalogs" => Some(Command::Catalogs),
        ":search" => Some(Command::Search),
        ":devices" => Some(Command::Devices),
        ":duplicates" => Some(Command::Duplicates),
        ":help" | ":h" => Some(Command::Help),
        ":quit" | ":q" => Some(Command::Quit),
        _ => parse_jump(input),
    }
}

fn parse_jump(input: &str) -> Option<Command> {
    let rest = input.strip_prefix(':')?;
    if let Some(chapter) = rest.strip_prefix('c') {
        let n: usize = chapter.parse().ok()?;
        return Some(Command::ChapterJump(n));
    }
    let n: usize = rest.parse().ok()?;
    Some(Command::PageJump(n))
}

fn complete(current: &str) -> Option<String> {
    let candidates: Vec<&&str> = COMMANDS.iter().filter(|c| c.starts_with(current)).collect();
    if candidates.is_empty() {
        return None;
    }
    if candidates.len() == 1 {
        return Some(candidates[0].to_string());
    }
    let prefix = longest_common_prefix(candidates.iter().map(|s| **s));
    if prefix.len() > current.len() {
        Some(prefix)
    } else {
        None
    }
}

fn longest_common_prefix<'a, I: IntoIterator<Item = &'a str>>(strings: I) -> String {
    let mut iter = strings.into_iter();
    let Some(first) = iter.next() else {
        return String::new();
    };
    let mut prefix: String = first.to_string();
    for s in iter {
        while !s.starts_with(&prefix) {
            prefix.pop();
            if prefix.is_empty() {
                return prefix;
            }
        }
    }
    prefix
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let mut spans = vec![Span::styled(
        state.input.value().to_string(),
        Style::default().add_modifier(Modifier::BOLD),
    )];
    if let Some(err) = &state.error {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(err.clone(), Style::default().fg(Color::Red)));
    }
    let paragraph = Paragraph::new(Line::from(spans));
    frame.render_widget(paragraph, area);
    // place cursor at end of input
    let cx = area
        .x
        .saturating_add(state.input.visual_cursor() as u16)
        .min(area.x + area.width.saturating_sub(1));
    frame.set_cursor_position((cx, area.y));
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
            handle_key(state, KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
    }

    #[test]
    fn starts_with_colon_prefix() {
        let s = State::new();
        assert_eq!(s.input.value(), ":");
    }

    #[test]
    fn tab_completes_unique_prefix() {
        let mut s = State::new();
        type_text(&mut s, "c");
        handle_key(&mut s, key(KeyCode::Tab));
        assert_eq!(s.input.value(), ":catalogs");
    }

    #[test]
    fn tab_completes_l_to_library() {
        let mut s = State::new();
        type_text(&mut s, "l");
        handle_key(&mut s, key(KeyCode::Tab));
        assert_eq!(s.input.value(), ":library");
    }

    #[test]
    fn tab_at_ambiguous_uses_common_prefix() {
        let mut s = State::new();
        type_text(&mut s, "q");
        handle_key(&mut s, key(KeyCode::Tab));
        // both :q and :quit start with :q — common prefix is :q (unchanged)
        assert_eq!(s.input.value(), ":q");
    }

    #[test]
    fn enter_quit_returns_quit_command() {
        let mut s = State::new();
        type_text(&mut s, "quit");
        let action = handle_key(&mut s, key(KeyCode::Enter));
        assert!(matches!(action, PaletteAction::Execute(Command::Quit)));
    }

    #[test]
    fn enter_q_returns_quit_command() {
        let mut s = State::new();
        type_text(&mut s, "q");
        let action = handle_key(&mut s, key(KeyCode::Enter));
        assert!(matches!(action, PaletteAction::Execute(Command::Quit)));
    }

    #[test]
    fn enter_search_returns_search_command() {
        let mut s = State::new();
        type_text(&mut s, "search");
        let action = handle_key(&mut s, key(KeyCode::Enter));
        assert!(matches!(action, PaletteAction::Execute(Command::Search)));
    }

    #[test]
    fn tab_completes_s_to_search() {
        let mut s = State::new();
        type_text(&mut s, "s");
        handle_key(&mut s, key(KeyCode::Tab));
        assert_eq!(s.input.value(), ":search");
    }

    #[test]
    fn tab_at_d_is_ambiguous_devices_duplicates() {
        let mut s = State::new();
        type_text(&mut s, "d");
        handle_key(&mut s, key(KeyCode::Tab));
        // `:devices` and `:duplicates` share only `:d`, so completion stalls there.
        assert_eq!(s.input.value(), ":d");
    }

    #[test]
    fn tab_completes_de_to_devices() {
        let mut s = State::new();
        type_text(&mut s, "de");
        handle_key(&mut s, key(KeyCode::Tab));
        assert_eq!(s.input.value(), ":devices");
    }

    #[test]
    fn enter_duplicates_returns_duplicates_command() {
        let mut s = State::new();
        type_text(&mut s, "duplicates");
        let action = handle_key(&mut s, key(KeyCode::Enter));
        assert!(matches!(
            action,
            PaletteAction::Execute(Command::Duplicates)
        ));
    }

    #[test]
    fn enter_devices_returns_devices_command() {
        let mut s = State::new();
        type_text(&mut s, "devices");
        let action = handle_key(&mut s, key(KeyCode::Enter));
        assert!(matches!(action, PaletteAction::Execute(Command::Devices)));
    }

    #[test]
    fn enter_catalogs_returns_catalogs_command() {
        let mut s = State::new();
        type_text(&mut s, "catalogs");
        let action = handle_key(&mut s, key(KeyCode::Enter));
        assert!(matches!(action, PaletteAction::Execute(Command::Catalogs)));
    }

    #[test]
    fn enter_help_returns_help_command() {
        let mut s = State::new();
        type_text(&mut s, "help");
        let action = handle_key(&mut s, key(KeyCode::Enter));
        assert!(matches!(action, PaletteAction::Execute(Command::Help)));
    }

    #[test]
    fn enter_h_returns_help_command() {
        let mut s = State::new();
        type_text(&mut s, "h");
        let action = handle_key(&mut s, key(KeyCode::Enter));
        assert!(matches!(action, PaletteAction::Execute(Command::Help)));
    }

    #[test]
    fn unknown_command_sets_error() {
        let mut s = State::new();
        type_text(&mut s, "nope");
        let action = handle_key(&mut s, key(KeyCode::Enter));
        assert!(matches!(action, PaletteAction::None));
        assert!(s.error.is_some());
    }

    #[test]
    fn esc_returns_close() {
        let mut s = State::new();
        let action = handle_key(&mut s, key(KeyCode::Esc));
        assert!(matches!(action, PaletteAction::Close));
    }

    #[test]
    fn parse_page_jump_numeric() {
        assert_eq!(parse(":42"), Some(Command::PageJump(42)));
        assert_eq!(parse(":1"), Some(Command::PageJump(1)));
    }

    #[test]
    fn parse_chapter_jump_with_c_prefix() {
        assert_eq!(parse(":c3"), Some(Command::ChapterJump(3)));
        assert_eq!(parse(":c1"), Some(Command::ChapterJump(1)));
    }

    #[test]
    fn parse_rejects_malformed_jumps() {
        assert_eq!(parse(":c"), None);
        assert_eq!(parse(":cabc"), None);
        assert_eq!(parse(":42abc"), None);
    }

    #[test]
    fn backspace_below_prefix_restores_prefix() {
        let mut s = State::new();
        // erase ":"
        handle_key(&mut s, key(KeyCode::Backspace));
        assert_eq!(s.input.value(), ":");
    }
}
