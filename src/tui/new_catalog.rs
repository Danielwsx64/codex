use std::path::{Path, PathBuf};

use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

use crate::catalog::handlers;
use crate::config::Registry;
use crate::tui::help::{Binding, Section};
use crate::tui::widgets::{is_submit_key, StatusMessage};

const FIELD_BINDINGS: &[Binding] = &[
    Binding {
        keys: "Tab",
        desc: "next field / complete path",
    },
    Binding {
        keys: "Shift+Tab",
        desc: "previous field",
    },
    Binding {
        keys: "Enter",
        desc: "next field (Save/Cancel on those buttons)",
    },
    Binding {
        keys: "Ctrl+S",
        desc: "submit",
    },
    Binding {
        keys: "Esc",
        desc: "cancel",
    },
];

const KIND_FIELD_BINDINGS: &[Binding] = &[
    Binding {
        keys: "←→ / Space",
        desc: "toggle init / add",
    },
    Binding {
        keys: "Tab / Enter",
        desc: "next field",
    },
    Binding {
        keys: "Shift+Tab",
        desc: "previous field",
    },
    Binding {
        keys: "Ctrl+S",
        desc: "submit",
    },
    Binding {
        keys: "Esc",
        desc: "cancel",
    },
];

pub fn help_sections(state: &State) -> Vec<Section> {
    let bindings = if state.focus == Focus::Kind {
        KIND_FIELD_BINDINGS
    } else {
        FIELD_BINDINGS
    };
    vec![Section {
        title: "New catalog",
        bindings,
    }]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Init,
    Add,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Init => "init (create new)",
            Kind::Add => "add (register existing)",
        }
    }

    fn toggle(self) -> Self {
        match self {
            Kind::Init => Kind::Add,
            Kind::Add => Kind::Init,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Kind,
    Name,
    Path,
    Description,
    Save,
    Cancel,
}

impl Focus {
    const ORDER: [Focus; 6] = [
        Focus::Kind,
        Focus::Name,
        Focus::Path,
        Focus::Description,
        Focus::Save,
        Focus::Cancel,
    ];

    fn next(self) -> Self {
        let idx = Self::ORDER.iter().position(|f| *f == self).unwrap_or(0);
        Self::ORDER[(idx + 1) % Self::ORDER.len()]
    }

    fn prev(self) -> Self {
        let idx = Self::ORDER.iter().position(|f| *f == self).unwrap_or(0);
        let len = Self::ORDER.len();
        Self::ORDER[(idx + len - 1) % len]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Welcome,
    Catalogs,
}

#[derive(Debug)]
pub struct State {
    pub kind: Kind,
    pub name: Input,
    pub path: Input,
    pub description: Input,
    pub focus: Focus,
    pub error: Option<String>,
    pub origin: Origin,
    pub cwd: PathBuf,
}

impl State {
    pub fn new(origin: Origin) -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        Self {
            kind: Kind::Init,
            name: Input::default(),
            path: Input::default(),
            description: Input::default(),
            focus: Focus::Name,
            error: None,
            origin,
            cwd,
        }
    }
}

pub enum WizardAction {
    None,
    Cancel(Origin),
    Submitted(Origin, StatusMessage),
    OpenPalette,
}

pub fn handle_key(
    state: &mut State,
    key: KeyEvent,
    registry: &mut Registry,
    config_dir: &Path,
) -> WizardAction {
    // Ctrl+S (or Ctrl+Enter where the terminal supports it) is the project-wide
    // form submit: create from any field.
    if is_submit_key(&key) {
        return submit(state, registry, config_dir);
    }

    match key.code {
        KeyCode::Esc => return WizardAction::Cancel(state.origin),
        KeyCode::Tab => {
            if state.focus == Focus::Path && try_path_complete(state) {
                return WizardAction::None;
            }
            state.focus = state.focus.next();
            return WizardAction::None;
        }
        KeyCode::BackTab => {
            state.focus = state.focus.prev();
            return WizardAction::None;
        }
        KeyCode::Enter => match state.focus {
            Focus::Save => return submit(state, registry, config_dir),
            Focus::Cancel => return WizardAction::Cancel(state.origin),
            _ => {
                state.focus = state.focus.next();
                return WizardAction::None;
            }
        },
        _ => {}
    }

    if state.focus == Focus::Kind {
        match key.code {
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') => {
                state.kind = state.kind.toggle();
            }
            _ => {}
        }
        return WizardAction::None;
    }

    let event = Event::Key(key);
    match state.focus {
        Focus::Kind | Focus::Save | Focus::Cancel => {}
        Focus::Name => {
            state.name.handle_event(&event);
        }
        Focus::Path => {
            state.path.handle_event(&event);
        }
        Focus::Description => {
            state.description.handle_event(&event);
        }
    }
    WizardAction::None
}

fn submit(state: &mut State, registry: &mut Registry, config_dir: &Path) -> WizardAction {
    let name = state.name.value().trim().to_string();
    let path_text = state.path.value().trim().to_string();
    let description = {
        let d = state.description.value().trim().to_string();
        if d.is_empty() {
            None
        } else {
            Some(d)
        }
    };

    if name.is_empty() {
        state.error = Some("name is required".to_string());
        return WizardAction::None;
    }
    if path_text.is_empty() {
        state.error = Some("path is required".to_string());
        return WizardAction::None;
    }
    let path = PathBuf::from(&path_text);

    let result = match state.kind {
        Kind::Init => handlers::handle_init(registry, config_dir, &name, &path, description, false)
            .map(|o| (o.name, o.path, "created")),
        Kind::Add => handlers::handle_add(registry, config_dir, &name, &path, description, false)
            .map(|o| (o.name, o.path, "registered")),
    };

    match result {
        Ok((reg_name, reg_path, verb)) => {
            state.error = None;
            WizardAction::Submitted(
                state.origin,
                StatusMessage::info(format!("{verb} `{reg_name}` at {}", reg_path.display())),
            )
        }
        Err(err) => {
            state.error = Some(err.to_string());
            WizardAction::None
        }
    }
}

pub fn captures_text_input(state: &State) -> bool {
    matches!(state.focus, Focus::Name | Focus::Path | Focus::Description)
}

fn try_path_complete(state: &mut State) -> bool {
    let current = state.path.value().to_string();
    if current.is_empty() {
        let mut cwd_str = state.cwd.to_string_lossy().into_owned();
        if !cwd_str.ends_with('/') {
            cwd_str.push('/');
        }
        state.path = Input::default().with_value(cwd_str);
        return true;
    }

    let (dir_in_input, prefix) = split_input(&current);
    let resolved = resolve_dir(dir_in_input, &state.cwd);
    let candidates = list_dir_candidates(&resolved, prefix);
    if candidates.is_empty() {
        return false;
    }
    let new_prefix = if candidates.len() == 1 {
        format!("{}/", candidates[0])
    } else {
        longest_common_prefix(candidates.iter().map(|s| s.as_str()))
    };
    if new_prefix.len() <= prefix.len() {
        return false;
    }
    let new_value = format!("{dir_in_input}{new_prefix}");
    state.path = Input::default().with_value(new_value);
    true
}

fn split_input(input: &str) -> (&str, &str) {
    match input.rfind('/') {
        Some(idx) => (&input[..=idx], &input[idx + 1..]),
        None => ("", input),
    }
}

fn resolve_dir(dir_in_input: &str, cwd: &Path) -> PathBuf {
    if dir_in_input.is_empty() {
        return cwd.to_path_buf();
    }
    let p = Path::new(dir_in_input);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    }
}

fn list_dir_candidates(dir: &Path, prefix: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with(prefix))
        .collect();
    names.sort();
    names
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
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Length(3), // kind
            Constraint::Length(3), // name
            Constraint::Length(3), // path
            Constraint::Length(3), // description
            Constraint::Length(1), // buttons
            Constraint::Length(2), // error / hint
            Constraint::Min(0),
        ])
        .split(area);

    let header = Paragraph::new(Line::from(Span::styled(
        "New catalog",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    frame.render_widget(header, layout[0]);

    render_kind(frame, layout[1], state);
    render_input(
        frame,
        layout[2],
        "Name",
        &state.name,
        state.focus == Focus::Name,
    );
    render_input(
        frame,
        layout[3],
        "Path",
        &state.path,
        state.focus == Focus::Path,
    );
    render_input(
        frame,
        layout[4],
        "Description (optional)",
        &state.description,
        state.focus == Focus::Description,
    );

    render_buttons(frame, layout[5], state);

    if let Some(err) = &state.error {
        let p = Paragraph::new(Line::from(Span::styled(
            err.clone(),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
        frame.render_widget(p, layout[6]);
    } else {
        let p = Paragraph::new(Line::from(Span::styled(
            "init creates db + books/; add registers an existing catalog directory",
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(p, layout[6]);
    }

    place_cursor(frame, &layout, state);
}

fn render_buttons(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(12),
            Constraint::Length(2),
            Constraint::Length(12),
            Constraint::Min(0),
        ])
        .split(area);
    render_button(frame, buttons[0], "[ Save ]", state.focus == Focus::Save);
    render_button(
        frame,
        buttons[2],
        "[ Cancel ]",
        state.focus == Focus::Cancel,
    );
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

fn render_kind(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let block = field_block("Kind", state.focus == Focus::Kind);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let p = Paragraph::new(Line::from(vec![
        Span::raw(" ◀ "),
        Span::styled(
            state.kind.label(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ▶  "),
        Span::styled(
            "(←/→ or space to toggle)",
            Style::default().fg(Color::DarkGray),
        ),
    ]));
    frame.render_widget(p, inner);
}

fn render_input(frame: &mut Frame<'_>, area: Rect, label: &str, input: &Input, focused: bool) {
    let block = field_block(label, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = inner.width.max(1) as usize;
    let scroll = input.visual_scroll(width);
    let value = input.value().to_string();
    let p = Paragraph::new(value).scroll((0, scroll as u16));
    frame.render_widget(p, inner);
}

fn field_block(title: &str, focused: bool) -> Block<'_> {
    let style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Block::default()
        .borders(Borders::ALL)
        .title(format!(" {title} "))
        .border_style(style)
}

fn place_cursor(frame: &mut Frame<'_>, layout: &[Rect], state: &State) {
    let (input, area) = match state.focus {
        Focus::Kind | Focus::Save | Focus::Cancel => return,
        Focus::Name => (&state.name, layout[2]),
        Focus::Path => (&state.path, layout[3]),
        Focus::Description => (&state.description, layout[4]),
    };
    // account for borders (1 char on each side)
    let inner_x = area.x.saturating_add(1);
    let inner_y = area.y.saturating_add(1);
    let inner_width = area.width.saturating_sub(2).max(1) as usize;
    let scroll = input.visual_scroll(inner_width);
    let cursor = input.visual_cursor();
    let cx = inner_x.saturating_add((cursor.saturating_sub(scroll)) as u16);
    frame.set_cursor_position((cx, inner_y));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use std::fs;
    use tempfile::tempdir;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl_enter() -> KeyEvent {
        KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL)
    }

    fn ctrl_s() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)
    }

    fn type_text(state: &mut State, text: &str, registry: &mut Registry, cfg: &Path) {
        for ch in text.chars() {
            handle_key(
                state,
                KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
                registry,
                cfg,
            );
        }
    }

    #[test]
    fn tab_cycles_focus_through_non_path_fields() {
        let mut s = State::new(Origin::Welcome);
        let cfg = tempdir().unwrap();
        // Use an empty tempdir as cwd so path completion has no candidates.
        s.cwd = cfg.path().to_path_buf();
        let mut reg = Registry::default();

        assert_eq!(s.focus, Focus::Name);
        handle_key(&mut s, key(KeyCode::Tab), &mut reg, cfg.path());
        assert_eq!(s.focus, Focus::Path);
        // Empty Path + Tab → fills with cwd, focus stays.
        handle_key(&mut s, key(KeyCode::Tab), &mut reg, cfg.path());
        assert_eq!(s.focus, Focus::Path);
        // cwd is empty → no further completion → advance.
        handle_key(&mut s, key(KeyCode::Tab), &mut reg, cfg.path());
        assert_eq!(s.focus, Focus::Description);
        handle_key(&mut s, key(KeyCode::Tab), &mut reg, cfg.path());
        assert_eq!(s.focus, Focus::Save);
        handle_key(&mut s, key(KeyCode::Tab), &mut reg, cfg.path());
        assert_eq!(s.focus, Focus::Cancel);
        handle_key(&mut s, key(KeyCode::Tab), &mut reg, cfg.path());
        assert_eq!(s.focus, Focus::Kind);
    }

    #[test]
    fn tab_on_empty_path_fills_with_cwd() {
        let dir = tempdir().unwrap();
        let mut s = State::new(Origin::Welcome);
        s.cwd = dir.path().to_path_buf();
        s.focus = Focus::Path;
        let mut reg = Registry::default();
        let cfg = tempdir().unwrap();
        handle_key(&mut s, key(KeyCode::Tab), &mut reg, cfg.path());
        assert_eq!(s.path.value(), format!("{}/", dir.path().display()));
        assert_eq!(s.focus, Focus::Path);
    }

    #[test]
    fn tab_completes_unique_relative_prefix() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("books-archive")).unwrap();
        fs::create_dir(dir.path().join("downloads")).unwrap();
        let mut s = State::new(Origin::Welcome);
        s.cwd = dir.path().to_path_buf();
        s.focus = Focus::Path;
        s.path = Input::default().with_value("bo".to_string());
        let mut reg = Registry::default();
        let cfg = tempdir().unwrap();
        handle_key(&mut s, key(KeyCode::Tab), &mut reg, cfg.path());
        assert_eq!(s.path.value(), "books-archive/");
        assert_eq!(s.focus, Focus::Path);
    }

    #[test]
    fn tab_extends_to_longest_common_prefix_when_ambiguous() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("library1")).unwrap();
        fs::create_dir(dir.path().join("library2")).unwrap();
        let mut s = State::new(Origin::Welcome);
        s.cwd = dir.path().to_path_buf();
        s.focus = Focus::Path;
        s.path = Input::default().with_value("li".to_string());
        let mut reg = Registry::default();
        let cfg = tempdir().unwrap();
        handle_key(&mut s, key(KeyCode::Tab), &mut reg, cfg.path());
        assert_eq!(s.path.value(), "library");
        assert_eq!(s.focus, Focus::Path);
    }

    #[test]
    fn tab_advances_focus_when_no_path_candidates() {
        let dir = tempdir().unwrap();
        let mut s = State::new(Origin::Welcome);
        s.cwd = dir.path().to_path_buf();
        s.focus = Focus::Path;
        s.path = Input::default().with_value("nonexistent".to_string());
        let mut reg = Registry::default();
        let cfg = tempdir().unwrap();
        handle_key(&mut s, key(KeyCode::Tab), &mut reg, cfg.path());
        assert_eq!(s.focus, Focus::Description);
    }

    #[test]
    fn tab_completes_absolute_path_prefix() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();
        let abs_prefix = format!("{}/", dir.path().display());
        let mut s = State::new(Origin::Welcome);
        s.focus = Focus::Path;
        s.path = Input::default().with_value(format!("{abs_prefix}su"));
        let mut reg = Registry::default();
        let cfg = tempdir().unwrap();
        handle_key(&mut s, key(KeyCode::Tab), &mut reg, cfg.path());
        assert_eq!(s.path.value(), format!("{abs_prefix}subdir/"));
    }

    #[test]
    fn tab_completion_ignores_files() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("readme.txt"), "x").unwrap();
        fs::create_dir(dir.path().join("readme-dir")).unwrap();
        let mut s = State::new(Origin::Welcome);
        s.cwd = dir.path().to_path_buf();
        s.focus = Focus::Path;
        s.path = Input::default().with_value("re".to_string());
        let mut reg = Registry::default();
        let cfg = tempdir().unwrap();
        handle_key(&mut s, key(KeyCode::Tab), &mut reg, cfg.path());
        assert_eq!(s.path.value(), "readme-dir/");
    }

    #[test]
    fn kind_toggles_with_arrows() {
        let mut s = State::new(Origin::Welcome);
        s.focus = Focus::Kind;
        let mut reg = Registry::default();
        let cfg = tempdir().unwrap();
        assert_eq!(s.kind, Kind::Init);
        handle_key(&mut s, key(KeyCode::Right), &mut reg, cfg.path());
        assert_eq!(s.kind, Kind::Add);
        handle_key(&mut s, key(KeyCode::Left), &mut reg, cfg.path());
        assert_eq!(s.kind, Kind::Init);
    }

    #[test]
    fn typing_into_name_field() {
        let mut s = State::new(Origin::Welcome);
        let mut reg = Registry::default();
        let cfg = tempdir().unwrap();
        type_text(&mut s, "main", &mut reg, cfg.path());
        assert_eq!(s.name.value(), "main");
    }

    #[test]
    fn submit_init_creates_entry() {
        let dir = tempdir().unwrap();
        let cfg = dir.path().join("cfg");
        fs::create_dir_all(&cfg).unwrap();
        let cat = dir.path().join("lib");
        let mut s = State::new(Origin::Welcome);
        let mut reg = Registry::default();

        type_text(&mut s, "main", &mut reg, &cfg);
        handle_key(&mut s, key(KeyCode::Tab), &mut reg, &cfg);
        type_text(&mut s, cat.to_str().unwrap(), &mut reg, &cfg);
        // Ctrl+S is the portable submit chord; it works from any field.
        let action = handle_key(&mut s, ctrl_s(), &mut reg, &cfg);
        assert!(matches!(
            action,
            WizardAction::Submitted(Origin::Welcome, _)
        ));
        assert_eq!(reg.catalogs.len(), 1);
        assert_eq!(reg.catalogs[0].name, "main");
    }

    #[test]
    fn ctrl_enter_also_submits_where_supported() {
        let dir = tempdir().unwrap();
        let cfg = dir.path().join("cfg");
        fs::create_dir_all(&cfg).unwrap();
        let cat = dir.path().join("lib");
        let mut s = State::new(Origin::Welcome);
        let mut reg = Registry::default();
        type_text(&mut s, "main", &mut reg, &cfg);
        s.path = Input::default().with_value(cat.to_string_lossy().into_owned());
        let action = handle_key(&mut s, ctrl_enter(), &mut reg, &cfg);
        assert!(matches!(
            action,
            WizardAction::Submitted(Origin::Welcome, _)
        ));
    }

    #[test]
    fn enter_advances_field_without_submitting() {
        let mut s = State::new(Origin::Welcome);
        let mut reg = Registry::default();
        let cfg = tempdir().unwrap();
        assert_eq!(s.focus, Focus::Name);
        let action = handle_key(&mut s, key(KeyCode::Enter), &mut reg, cfg.path());
        assert!(matches!(action, WizardAction::None));
        assert_eq!(s.focus, Focus::Path);
        assert!(reg.catalogs.is_empty());
    }

    #[test]
    fn enter_on_save_button_submits() {
        let dir = tempdir().unwrap();
        let cfg = dir.path().join("cfg");
        fs::create_dir_all(&cfg).unwrap();
        let cat = dir.path().join("lib");
        let mut s = State::new(Origin::Welcome);
        let mut reg = Registry::default();
        type_text(&mut s, "main", &mut reg, &cfg);
        s.path = Input::default().with_value(cat.to_string_lossy().into_owned());
        s.focus = Focus::Save;
        let action = handle_key(&mut s, key(KeyCode::Enter), &mut reg, &cfg);
        assert!(matches!(
            action,
            WizardAction::Submitted(Origin::Welcome, _)
        ));
        assert_eq!(reg.catalogs.len(), 1);
    }

    #[test]
    fn enter_on_cancel_button_cancels() {
        let mut s = State::new(Origin::Catalogs);
        let mut reg = Registry::default();
        let cfg = tempdir().unwrap();
        s.focus = Focus::Cancel;
        let action = handle_key(&mut s, key(KeyCode::Enter), &mut reg, cfg.path());
        assert!(matches!(action, WizardAction::Cancel(Origin::Catalogs)));
    }

    #[test]
    fn submit_invalid_name_sets_error_keeps_open() {
        let dir = tempdir().unwrap();
        let cfg = dir.path().join("cfg");
        fs::create_dir_all(&cfg).unwrap();
        let mut s = State::new(Origin::Welcome);
        let mut reg = Registry::default();
        // empty name
        // skip name, fill path
        handle_key(&mut s, key(KeyCode::Tab), &mut reg, &cfg);
        type_text(&mut s, "/tmp/foo", &mut reg, &cfg);
        let action = handle_key(&mut s, ctrl_enter(), &mut reg, &cfg);
        assert!(matches!(action, WizardAction::None));
        assert!(s.error.is_some());
        assert!(reg.catalogs.is_empty());
    }

    #[test]
    fn esc_returns_cancel() {
        let mut s = State::new(Origin::Catalogs);
        let mut reg = Registry::default();
        let cfg = tempdir().unwrap();
        let action = handle_key(&mut s, key(KeyCode::Esc), &mut reg, cfg.path());
        assert!(matches!(action, WizardAction::Cancel(Origin::Catalogs)));
    }

    #[test]
    fn captures_text_input_only_on_text_fields() {
        let mut s = State::new(Origin::Welcome);
        assert!(captures_text_input(&s)); // Name
        for f in [Focus::Kind, Focus::Save, Focus::Cancel] {
            s.focus = f;
            assert!(!captures_text_input(&s), "{f:?} must not capture text");
        }
        for f in [Focus::Name, Focus::Path, Focus::Description] {
            s.focus = f;
            assert!(captures_text_input(&s), "{f:?} must capture text");
        }
    }
}
