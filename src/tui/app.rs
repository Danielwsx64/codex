use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::Backend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::{Frame, Terminal};

use crate::config::Registry;
use crate::tui::catalogs;
use crate::tui::confirm;
use crate::tui::help;
use crate::tui::library;
use crate::tui::new_catalog;
use crate::tui::palette;
use crate::tui::reader;
use crate::tui::too_small;
use crate::tui::welcome;
use crate::tui::widgets::{outer_block, render_default_footer, render_status, StatusMessage};

pub enum Screen {
    Welcome(welcome::State),
    Catalogs(catalogs::State),
    NewCatalog(new_catalog::State),
    Library(library::State),
    Reader(Box<reader::State>),
}

pub struct App {
    pub config_dir: PathBuf,
    pub registry: Registry,
    pub screen: Screen,
    pub palette: Option<palette::State>,
    pub help: Option<help::State>,
    pub confirm: Option<confirm::State>,
    pub status: Option<StatusMessage>,
    pub should_quit: bool,
    pub terminal_too_small: bool,
}

impl App {
    pub fn new(config_dir: PathBuf) -> Result<Self> {
        let registry = Registry::load(&config_dir).with_context(|| {
            format!(
                "failed to load catalog registry from {}",
                config_dir.display()
            )
        })?;
        let screen = Screen::Welcome(welcome::State::new());
        Ok(Self {
            config_dir,
            registry,
            screen,
            palette: None,
            help: None,
            confirm: None,
            status: None,
            should_quit: false,
            terminal_too_small: false,
        })
    }

    pub fn run<B: Backend>(mut self, terminal: &mut Terminal<B>) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|f| self.render(f))?;
            if self.has_pending_work() {
                if event::poll(Duration::from_millis(0))? {
                    let event = event::read()?;
                    self.dispatch(event)?;
                } else {
                    self.advance_work();
                }
            } else {
                let event = event::read()?;
                self.dispatch(event)?;
            }
        }
        Ok(())
    }

    fn has_pending_work(&self) -> bool {
        matches!(&self.screen, Screen::Library(s) if library::has_pending_embed_job(s))
    }

    fn advance_work(&mut self) {
        if let Screen::Library(s) = &mut self.screen {
            library::advance_embed_job(s);
        }
    }

    pub fn dispatch(&mut self, event: Event) -> Result<()> {
        let Event::Key(key) = event else {
            return Ok(());
        };
        // Ignore key release/repeat to avoid double-firing on terminals that emit them.
        if key.kind != crossterm::event::KeyEventKind::Press {
            return Ok(());
        }

        if self.terminal_too_small {
            if is_exit_key(&key) {
                self.should_quit = true;
            }
            return Ok(());
        }

        // Backspace mirrors Esc as a "back / cancel" key everywhere except
        // inside text input. We translate before dispatch so screens don't
        // each have to special-case it. The palette is excluded so Backspace
        // keeps deleting characters in the `:` input; text-capturing screens
        // (wizard fields, library edit/filter inputs) are excluded for the
        // same reason via `captures_text_input()`.
        let key = self.translate_back_keys(key);

        if self.confirm.is_some() {
            // Ctrl+C is the hard exit and bypasses the guard it would normally
            // open; every other key drives the dialog.
            if is_ctrl_c(&key) {
                self.should_quit = true;
                return Ok(());
            }
            self.handle_confirm(key);
            return Ok(());
        }

        if self.palette.is_some() {
            self.handle_palette(key);
            return Ok(());
        }

        if self.help.is_some() {
            // Ctrl+C always quits, even with help open; help::handle_key only
            // closes on Esc/?/q. Other keys are swallowed.
            if is_ctrl_c(&key) {
                self.should_quit = true;
                return Ok(());
            }
            if matches!(help::handle_key(key), help::HelpAction::Close) {
                self.help = None;
            }
            return Ok(());
        }

        // Ctrl+C is the immediate hard exit; `q` opens a confirmation guard.
        if !self.captures_text_input() {
            if is_ctrl_c(&key) {
                self.should_quit = true;
                return Ok(());
            }
            if is_quit_key(&key) {
                self.confirm = Some(confirm::State::quit());
                return Ok(());
            }
        }

        if !self.captures_text_input() && is_help_key(&key) {
            self.help = Some(help::State);
            return Ok(());
        }

        self.handle_screen(key);
        Ok(())
    }

    fn translate_back_keys(&self, key: KeyEvent) -> KeyEvent {
        if key.code != KeyCode::Backspace {
            return key;
        }
        // Palette has its own text input where Backspace is the editing key.
        if self.palette.is_some() {
            return key;
        }
        // Screen-level text capture (wizard fields, library filter/edit).
        if self.captures_text_input() {
            return key;
        }
        KeyEvent {
            code: KeyCode::Esc,
            ..key
        }
    }

    fn captures_text_input(&self) -> bool {
        match &self.screen {
            Screen::NewCatalog(s) => new_catalog::captures_text_input(s),
            Screen::Library(s) => library::captures_text_input(s),
            Screen::Reader(s) => reader::captures_text_input(s),
            _ => false,
        }
    }

    fn handle_confirm(&mut self, key: KeyEvent) {
        let Some(state) = self.confirm.as_mut() else {
            return;
        };
        match confirm::handle_key(state, key) {
            confirm::ConfirmAction::None => {}
            confirm::ConfirmAction::Confirm => {
                self.confirm = None;
                self.should_quit = true;
            }
            confirm::ConfirmAction::Cancel => {
                self.confirm = None;
            }
        }
    }

    fn handle_palette(&mut self, key: KeyEvent) {
        let Some(state) = self.palette.as_mut() else {
            return;
        };
        match palette::handle_key(state, key) {
            palette::PaletteAction::None => {}
            palette::PaletteAction::Close => {
                self.palette = None;
            }
            palette::PaletteAction::Execute(cmd) => {
                self.palette = None;
                self.apply_palette_command(cmd);
            }
        }
    }

    fn apply_palette_command(&mut self, cmd: palette::Command) {
        match cmd {
            palette::Command::Quit => {
                self.should_quit = true;
            }
            palette::Command::Library => {
                self.screen = Screen::Library(library::State::load(&self.registry));
            }
            palette::Command::Catalogs => {
                self.screen = Screen::Catalogs(catalogs::State::from_registry(&self.registry));
            }
            palette::Command::Search => {
                self.open_search();
            }
            palette::Command::Help => {
                self.help = Some(help::State);
            }
            palette::Command::PageJump(n) => match &mut self.screen {
                Screen::Reader(state) => reader::go_to_page(state, n),
                _ => {
                    self.status = Some(StatusMessage::error(
                        "page jump (`:N`) is only available in the reader",
                    ));
                }
            },
            palette::Command::ChapterJump(n) => match &mut self.screen {
                Screen::Reader(state) => reader::go_to_chapter(state, n),
                _ => {
                    self.status = Some(StatusMessage::error(
                        "chapter jump (`:cN`) is only available in the reader",
                    ));
                }
            },
        }
    }

    // Search lives on the Library screen as "filter mode". `:search` (and the
    // welcome link) open the advanced filter wizard there, preserving any
    // active filter when we're already on Library so it can be edited.
    fn open_search(&mut self) {
        match &mut self.screen {
            Screen::Library(s) => library::open_search_wizard(s),
            _ => {
                let mut s = library::State::load(&self.registry);
                library::open_search_wizard(&mut s);
                self.screen = Screen::Library(s);
            }
        }
    }

    fn handle_screen(&mut self, key: KeyEvent) {
        self.status = None;
        match &mut self.screen {
            Screen::Welcome(state) => {
                let action = welcome::handle_key(state, key);
                self.apply_welcome_action(action);
            }
            Screen::Catalogs(state) => {
                let action = catalogs::handle_key(state, key, &mut self.registry, &self.config_dir);
                self.apply_catalogs_action(action);
            }
            Screen::NewCatalog(state) => {
                let action =
                    new_catalog::handle_key(state, key, &mut self.registry, &self.config_dir);
                self.apply_wizard_action(action);
            }
            Screen::Library(state) => {
                let action = library::handle_key(state, key);
                self.apply_library_action(action);
            }
            Screen::Reader(state) => {
                let action = reader::handle_key(state, key);
                self.apply_reader_action(action);
            }
        }
    }

    fn apply_welcome_action(&mut self, action: welcome::WelcomeAction) {
        match action {
            welcome::WelcomeAction::None => {}
            welcome::WelcomeAction::OpenPalette => {
                self.palette = Some(palette::State::new());
            }
            welcome::WelcomeAction::Enter(welcome::Section::Library) => {
                self.screen = Screen::Library(library::State::load(&self.registry));
            }
            welcome::WelcomeAction::Enter(welcome::Section::Catalogs) => {
                self.screen = Screen::Catalogs(catalogs::State::from_registry(&self.registry));
            }
            welcome::WelcomeAction::Enter(welcome::Section::Search) => {
                self.open_search();
            }
            welcome::WelcomeAction::Enter(_) => {}
        }
    }

    fn apply_catalogs_action(&mut self, action: catalogs::CatalogsAction) {
        match action {
            catalogs::CatalogsAction::None => {}
            catalogs::CatalogsAction::Back => {
                self.screen = Screen::Welcome(welcome::State::new());
            }
            catalogs::CatalogsAction::OpenWizard => {
                self.screen =
                    Screen::NewCatalog(new_catalog::State::new(new_catalog::Origin::Catalogs));
            }
            catalogs::CatalogsAction::OpenPalette => {
                self.palette = Some(palette::State::new());
            }
            catalogs::CatalogsAction::Status(s) => {
                self.status = Some(s);
            }
        }
    }

    fn apply_library_action(&mut self, action: library::LibraryAction) {
        match action {
            library::LibraryAction::None => {}
            library::LibraryAction::Back => {
                self.screen = Screen::Welcome(welcome::State::new());
            }
            library::LibraryAction::OpenPalette => {
                self.palette = Some(palette::State::new());
            }
            library::LibraryAction::Status(s) => {
                self.status = Some(s);
            }
            library::LibraryAction::OpenReader { catalog_dir, book } => {
                self.open_reader(catalog_dir, *book);
            }
        }
    }

    fn apply_reader_action(&mut self, action: reader::ReaderAction) {
        match action {
            reader::ReaderAction::None => {}
            reader::ReaderAction::Back => {
                self.screen = Screen::Library(library::State::load(&self.registry));
            }
            reader::ReaderAction::OpenPalette => {
                self.palette = Some(palette::State::new());
            }
            reader::ReaderAction::Status(s) => {
                self.status = Some(s);
            }
        }
    }

    fn open_reader(&mut self, catalog_dir: PathBuf, book: crate::catalog::books::Book) {
        match reader::open_book(catalog_dir, &book, self.registry.reader) {
            Ok(state) => {
                self.screen = Screen::Reader(Box::new(state));
            }
            Err(err) => {
                self.status = Some(StatusMessage::error(format!(
                    "could not open `{}`: {err}",
                    book.title
                )));
            }
        }
    }

    fn apply_wizard_action(&mut self, action: new_catalog::WizardAction) {
        match action {
            new_catalog::WizardAction::None => {}
            new_catalog::WizardAction::Cancel(origin) => {
                self.screen = match origin {
                    new_catalog::Origin::Welcome => Screen::Welcome(welcome::State::new()),
                    new_catalog::Origin::Catalogs => {
                        Screen::Catalogs(catalogs::State::from_registry(&self.registry))
                    }
                };
            }
            new_catalog::WizardAction::Submitted(_, status) => {
                self.screen = Screen::Catalogs(catalogs::State::from_registry(&self.registry));
                self.status = Some(status);
            }
            new_catalog::WizardAction::OpenPalette => {
                self.palette = Some(palette::State::new());
            }
        }
    }

    pub fn render(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        self.terminal_too_small = too_small::is_too_small(area);
        if self.terminal_too_small {
            too_small::render(frame, area);
            return;
        }
        let block = outer_block("codex");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(inner);

        match &mut self.screen {
            Screen::Welcome(state) => welcome::render(frame, chunks[0], state),
            Screen::Catalogs(state) => catalogs::render(frame, chunks[0], state),
            Screen::NewCatalog(state) => new_catalog::render(frame, chunks[0], state),
            Screen::Library(state) => library::render(frame, chunks[0], state),
            Screen::Reader(state) => reader::render(frame, chunks[0], state),
        }

        if self.help.is_some() {
            let mut sections = vec![help::GLOBAL];
            sections.extend(self.screen_help_sections());
            help::render(frame, chunks[0], &sections);
        }

        if let Some(palette) = &self.palette {
            palette::render(frame, chunks[1], palette);
        } else if let Some(status) = &self.status {
            render_status(frame, chunks[1], status);
        } else {
            render_default_footer(frame, chunks[1]);
        }

        // The quit guard sits above every other layer, including help/palette.
        if let Some(confirm) = &self.confirm {
            confirm::render(frame, chunks[0], confirm);
        }
    }

    fn screen_help_sections(&self) -> Vec<help::Section> {
        match &self.screen {
            Screen::Welcome(state) => welcome::help_sections(state),
            Screen::Catalogs(state) => catalogs::help_sections(state),
            Screen::NewCatalog(state) => new_catalog::help_sections(state),
            Screen::Library(state) => library::help_sections(state),
            Screen::Reader(state) => reader::help_sections(state),
        }
    }
}

pub fn is_exit_key(key: &KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('q') => true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => true,
        _ => false,
    }
}

fn is_quit_key(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('q'))
}

fn is_ctrl_c(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('c')) && key.modifiers.contains(KeyModifiers::CONTROL)
}

fn is_help_key(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('?'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventKind};
    use std::fs;
    use tempfile::tempdir;

    use crate::catalog::handlers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    #[test]
    fn exit_keys_are_q_and_ctrl_c_only() {
        assert!(is_exit_key(&key(KeyCode::Char('q'))));
        assert!(is_exit_key(&ctrl(KeyCode::Char('c'))));
        assert!(!is_exit_key(&key(KeyCode::Esc)));
        assert!(!is_exit_key(&key(KeyCode::Enter)));
        assert!(!is_exit_key(&key(KeyCode::Char('c'))));
    }

    #[test]
    fn always_starts_on_welcome() {
        let dir = tempdir().unwrap();
        let cfg = dir.path().join("cfg");
        fs::create_dir_all(&cfg).unwrap();

        let app = App::new(cfg.clone()).unwrap();
        assert!(matches!(app.screen, Screen::Welcome(_)));

        let mut reg = Registry::default();
        handlers::handle_init(&mut reg, &cfg, "one", &dir.path().join("a"), None, false).unwrap();
        handlers::handle_init(&mut reg, &cfg, "two", &dir.path().join("b"), None, true).unwrap();

        let app = App::new(cfg).unwrap();
        assert!(
            matches!(app.screen, Screen::Welcome(_)),
            "welcome must remain the home screen even with multiple catalogs registered"
        );
    }

    #[test]
    fn q_opens_quit_confirmation_without_quitting() {
        let dir = tempdir().unwrap();
        let cfg = dir.path().join("cfg");
        fs::create_dir_all(&cfg).unwrap();
        let mut app = App::new(cfg).unwrap();
        app.dispatch(Event::Key(key(KeyCode::Char('q')))).unwrap();
        assert!(!app.should_quit, "q must not quit immediately");
        assert!(app.confirm.is_some(), "q must open the quit confirmation");
    }

    #[test]
    fn confirm_enter_quits() {
        let mut app = fresh_app();
        app.dispatch(Event::Key(key(KeyCode::Char('q')))).unwrap();
        // OK is focused by default; Enter confirms.
        app.dispatch(Event::Key(key(KeyCode::Enter))).unwrap();
        assert!(app.should_quit);
        assert!(app.confirm.is_none());
    }

    #[test]
    fn confirm_esc_cancels_and_keeps_running() {
        let mut app = fresh_app();
        app.dispatch(Event::Key(key(KeyCode::Char('q')))).unwrap();
        app.dispatch(Event::Key(key(KeyCode::Esc))).unwrap();
        assert!(!app.should_quit);
        assert!(app.confirm.is_none(), "Esc must dismiss the dialog");
    }

    #[test]
    fn confirm_enter_on_cancel_button_keeps_running() {
        let mut app = fresh_app();
        app.dispatch(Event::Key(key(KeyCode::Char('q')))).unwrap();
        // Move focus to Cancel, then Enter.
        app.dispatch(Event::Key(key(KeyCode::Right))).unwrap();
        app.dispatch(Event::Key(key(KeyCode::Enter))).unwrap();
        assert!(!app.should_quit);
        assert!(app.confirm.is_none());
    }

    #[test]
    fn ctrl_c_quits_immediately_without_confirmation() {
        let mut app = fresh_app();
        app.dispatch(Event::Key(ctrl(KeyCode::Char('c')))).unwrap();
        assert!(app.should_quit);
        assert!(app.confirm.is_none());
    }

    #[test]
    fn ctrl_c_quits_even_with_confirmation_open() {
        let mut app = fresh_app();
        app.dispatch(Event::Key(key(KeyCode::Char('q')))).unwrap();
        assert!(app.confirm.is_some());
        app.dispatch(Event::Key(ctrl(KeyCode::Char('c')))).unwrap();
        assert!(app.should_quit);
    }

    #[test]
    fn q_does_not_quit_when_palette_open() {
        let dir = tempdir().unwrap();
        let cfg = dir.path().join("cfg");
        fs::create_dir_all(&cfg).unwrap();
        let mut app = App::new(cfg).unwrap();
        app.dispatch(Event::Key(key(KeyCode::Char(':')))).unwrap();
        assert!(app.palette.is_some());
        app.dispatch(Event::Key(key(KeyCode::Char('q')))).unwrap();
        assert!(!app.should_quit, "q should be typed into palette, not quit");
        assert!(app.palette.is_some(), "palette should remain open");
    }

    #[test]
    fn q_does_not_quit_when_wizard_text_field_focused() {
        let dir = tempdir().unwrap();
        let cfg = dir.path().join("cfg");
        fs::create_dir_all(&cfg).unwrap();
        let mut app = App::new(cfg).unwrap();
        // navigate to wizard via welcome → catalogs → n
        // simpler: directly install wizard via apply_welcome_action path
        app.screen = Screen::NewCatalog(new_catalog::State::new(new_catalog::Origin::Welcome));
        app.dispatch(Event::Key(key(KeyCode::Char('q')))).unwrap();
        assert!(!app.should_quit);
        // q should have been typed into the name field
        match &app.screen {
            Screen::NewCatalog(s) => assert_eq!(s.name.value(), "q"),
            _ => panic!("expected wizard"),
        }
    }

    #[test]
    fn palette_quit_command_sets_should_quit() {
        let dir = tempdir().unwrap();
        let cfg = dir.path().join("cfg");
        fs::create_dir_all(&cfg).unwrap();
        let mut app = App::new(cfg).unwrap();
        app.dispatch(Event::Key(key(KeyCode::Char(':')))).unwrap();
        for ch in "quit".chars() {
            app.dispatch(Event::Key(key(KeyCode::Char(ch)))).unwrap();
        }
        app.dispatch(Event::Key(key(KeyCode::Enter))).unwrap();
        assert!(app.should_quit);
    }

    #[test]
    fn palette_catalogs_command_navigates() {
        let dir = tempdir().unwrap();
        let cfg = dir.path().join("cfg");
        fs::create_dir_all(&cfg).unwrap();
        let mut app = App::new(cfg).unwrap();
        app.dispatch(Event::Key(key(KeyCode::Char(':')))).unwrap();
        for ch in "catalogs".chars() {
            app.dispatch(Event::Key(key(KeyCode::Char(ch)))).unwrap();
        }
        app.dispatch(Event::Key(key(KeyCode::Enter))).unwrap();
        assert!(app.palette.is_none());
        assert!(matches!(app.screen, Screen::Catalogs(_)));
    }

    fn fresh_app() -> App {
        let dir = tempdir().unwrap();
        let cfg = dir.path().join("cfg");
        fs::create_dir_all(&cfg).unwrap();
        // Leak the tempdir so the cfg path stays valid for the App's lifetime in
        // the test. Tests are short-lived; the OS reclaims on process exit.
        let _ = Box::leak(Box::new(dir));
        App::new(cfg).unwrap()
    }

    #[test]
    fn question_mark_opens_help_on_welcome() {
        let mut app = fresh_app();
        app.dispatch(Event::Key(key(KeyCode::Char('?')))).unwrap();
        assert!(app.help.is_some());
    }

    #[test]
    fn question_mark_in_wizard_text_field_does_not_open_help() {
        let mut app = fresh_app();
        app.screen = Screen::NewCatalog(new_catalog::State::new(new_catalog::Origin::Welcome));
        app.dispatch(Event::Key(key(KeyCode::Char('?')))).unwrap();
        assert!(app.help.is_none());
        match &app.screen {
            Screen::NewCatalog(s) => assert_eq!(s.name.value(), "?"),
            _ => panic!("expected wizard"),
        }
    }

    #[test]
    fn help_open_swallows_unrelated_keys() {
        let mut app = fresh_app();
        app.help = Some(help::State);
        // Pressing `e` should NOT trigger any screen action while help is open.
        app.dispatch(Event::Key(key(KeyCode::Char('e')))).unwrap();
        assert!(app.help.is_some(), "help must stay open on unrelated key");
        assert!(matches!(app.screen, Screen::Welcome(_)));
    }

    #[test]
    fn help_open_q_closes_help_without_quitting() {
        let mut app = fresh_app();
        app.help = Some(help::State);
        app.dispatch(Event::Key(key(KeyCode::Char('q')))).unwrap();
        assert!(app.help.is_none());
        assert!(!app.should_quit);
    }

    #[test]
    fn help_open_esc_closes_help() {
        let mut app = fresh_app();
        app.help = Some(help::State);
        app.dispatch(Event::Key(key(KeyCode::Esc))).unwrap();
        assert!(app.help.is_none());
    }

    #[test]
    fn backspace_mirrors_esc_in_help_overlay() {
        let mut app = fresh_app();
        app.help = Some(help::State);
        app.dispatch(Event::Key(key(KeyCode::Backspace))).unwrap();
        assert!(app.help.is_none(), "Backspace should close help like Esc");
    }

    #[test]
    fn backspace_mirrors_esc_in_quit_confirmation() {
        let mut app = fresh_app();
        app.dispatch(Event::Key(key(KeyCode::Char('q')))).unwrap();
        assert!(app.confirm.is_some());
        app.dispatch(Event::Key(key(KeyCode::Backspace))).unwrap();
        assert!(
            app.confirm.is_none(),
            "Backspace should cancel the confirm dialog like Esc"
        );
        assert!(!app.should_quit);
    }

    #[test]
    fn backspace_in_palette_does_not_close_palette() {
        let mut app = fresh_app();
        app.dispatch(Event::Key(key(KeyCode::Char(':')))).unwrap();
        assert!(app.palette.is_some());
        // Backspace inside the palette is the editing key; it must not be
        // translated to Esc (which would close the palette).
        app.dispatch(Event::Key(key(KeyCode::Backspace))).unwrap();
        assert!(
            app.palette.is_some(),
            "Backspace inside palette stays as text-edit, palette remains open"
        );
    }

    #[test]
    fn backspace_in_wizard_text_field_does_not_translate() {
        let mut app = fresh_app();
        app.screen = Screen::NewCatalog(new_catalog::State::new(new_catalog::Origin::Welcome));
        // Type two characters then Backspace; expect the wizard to delete a
        // char, not abort the screen.
        app.dispatch(Event::Key(key(KeyCode::Char('a')))).unwrap();
        app.dispatch(Event::Key(key(KeyCode::Char('b')))).unwrap();
        app.dispatch(Event::Key(key(KeyCode::Backspace))).unwrap();
        match &app.screen {
            Screen::NewCatalog(s) => assert_eq!(s.name.value(), "a"),
            _ => panic!("expected wizard still active"),
        }
    }

    #[test]
    fn help_open_ctrl_c_quits() {
        let mut app = fresh_app();
        app.help = Some(help::State);
        app.dispatch(Event::Key(ctrl(KeyCode::Char('c')))).unwrap();
        assert!(app.should_quit);
    }

    #[test]
    fn help_open_colon_does_not_open_palette() {
        let mut app = fresh_app();
        app.help = Some(help::State);
        app.dispatch(Event::Key(key(KeyCode::Char(':')))).unwrap();
        assert!(app.palette.is_none());
        assert!(app.help.is_some());
    }

    #[test]
    fn too_small_blocks_non_exit_keys() {
        let mut app = fresh_app();
        app.terminal_too_small = true;
        // `?` would normally open help; while too-small it must be swallowed.
        app.dispatch(Event::Key(key(KeyCode::Char('?')))).unwrap();
        assert!(app.help.is_none());
        assert!(!app.should_quit);
        // `:` would normally open palette; same swallowing.
        app.dispatch(Event::Key(key(KeyCode::Char(':')))).unwrap();
        assert!(app.palette.is_none());
        assert!(!app.should_quit);
    }

    #[test]
    fn too_small_allows_quit_keys() {
        let mut app = fresh_app();
        app.terminal_too_small = true;
        app.dispatch(Event::Key(key(KeyCode::Char('q')))).unwrap();
        assert!(app.should_quit);

        let mut app = fresh_app();
        app.terminal_too_small = true;
        app.dispatch(Event::Key(ctrl(KeyCode::Char('c')))).unwrap();
        assert!(app.should_quit);
    }

    #[test]
    fn palette_search_from_welcome_opens_library_with_wizard() {
        let mut app = fresh_app();
        app.dispatch(Event::Key(key(KeyCode::Char(':')))).unwrap();
        for ch in "search".chars() {
            app.dispatch(Event::Key(key(KeyCode::Char(ch)))).unwrap();
        }
        app.dispatch(Event::Key(key(KeyCode::Enter))).unwrap();
        assert!(app.palette.is_none());
        match &app.screen {
            Screen::Library(s) => {
                assert!(matches!(s.overlay, Some(library::Overlay::Search(_))));
            }
            _ => panic!("expected Library screen"),
        }
    }

    #[test]
    fn welcome_search_link_opens_library_with_wizard() {
        let mut app = fresh_app();
        // Welcome menu: Library -> Catalogs -> Search.
        app.dispatch(Event::Key(key(KeyCode::Down))).unwrap();
        app.dispatch(Event::Key(key(KeyCode::Down))).unwrap();
        app.dispatch(Event::Key(key(KeyCode::Enter))).unwrap();
        match &app.screen {
            Screen::Library(s) => {
                assert!(matches!(s.overlay, Some(library::Overlay::Search(_))));
            }
            _ => panic!("expected Library screen"),
        }
    }

    #[test]
    fn palette_search_from_library_preserves_filter() {
        let mut app = fresh_app();
        app.screen = Screen::Library(library::State::load(&app.registry));
        if let Screen::Library(s) = &mut app.screen {
            s.filter = Some(library::ActiveFilter {
                criteria: library::FilterCriteria {
                    query: Some("dune".to_string()),
                    ..library::FilterCriteria::default()
                },
                kind: library::FilterKind::Quick,
            });
        }
        app.dispatch(Event::Key(key(KeyCode::Char(':')))).unwrap();
        for ch in "search".chars() {
            app.dispatch(Event::Key(key(KeyCode::Char(ch)))).unwrap();
        }
        app.dispatch(Event::Key(key(KeyCode::Enter))).unwrap();
        match &app.screen {
            Screen::Library(s) => {
                match &s.overlay {
                    Some(library::Overlay::Search(w)) => assert_eq!(w.query.value(), "dune"),
                    _ => panic!("expected search overlay"),
                }
                assert!(s.filter.is_some(), "existing filter must survive :search");
            }
            _ => panic!("expected Library screen"),
        }
    }

    #[test]
    fn palette_help_command_opens_help_overlay() {
        let mut app = fresh_app();
        app.dispatch(Event::Key(key(KeyCode::Char(':')))).unwrap();
        for ch in "help".chars() {
            app.dispatch(Event::Key(key(KeyCode::Char(ch)))).unwrap();
        }
        app.dispatch(Event::Key(key(KeyCode::Enter))).unwrap();
        assert!(app.palette.is_none());
        assert!(app.help.is_some());
    }
}
