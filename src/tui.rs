use std::io::{self, Stdout};
use std::path::Path;

use anyhow::{Context, Result};
use crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

pub mod app;
pub mod catalogs;
pub mod confirm;
pub mod devices;
pub mod duplicates;
pub mod help;
pub mod library;
pub mod loading;
pub mod new_catalog;
pub mod palette;
pub mod pick;
pub mod reader;
pub mod too_small;
pub mod welcome;
pub mod widgets;

use crate::config::paths::resolve_config_dir;
use crate::tui::app::App;

pub fn run(data_dir: Option<&Path>) -> Result<()> {
    let config_dir =
        resolve_config_dir(data_dir).context("failed to resolve the cdx config directory")?;
    let app = App::new(config_dir)?;
    let mut terminal = TerminalGuard::enter()?;
    app.run(&mut terminal.terminal)?;
    Ok(())
}

pub(crate) struct TerminalGuard {
    pub(crate) terminal: Terminal<CrosstermBackend<Stdout>>,
    // Whether we pushed keyboard enhancement flags and must pop them on exit.
    // Only set on terminals that report support, so the pop stays balanced.
    keyboard_enhanced: bool,
}

impl TerminalGuard {
    pub(crate) fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        // Best-effort: on terminals that negotiate it, DISAMBIGUATE_ESCAPE_CODES
        // lets Ctrl+Enter be reported as a distinct chord. Most terminals (and
        // any tmux/screen session) can't, so the portable form-submit chord is
        // Ctrl+S (see widgets::is_submit_key); this just enables Ctrl+Enter as a
        // bonus where supported.
        let keyboard_enhanced = matches!(supports_keyboard_enhancement(), Ok(true));
        if keyboard_enhanced {
            execute!(
                stdout,
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
            )?;
        }
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        // Last step, so a failed setup never leaves logs silenced: while the
        // TUI owns the terminal, tty-bound log lines are dropped instead of
        // smearing over the alternate screen (see `crate::log_writer`).
        crate::set_tui_owns_terminal(true);
        Ok(Self {
            terminal,
            keyboard_enhanced,
        })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        crate::set_tui_owns_terminal(false);
        if self.keyboard_enhanced {
            let _ = execute!(self.terminal.backend_mut(), PopKeyboardEnhancementFlags);
        }
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}
