use std::io::{self, Stdout};
use std::path::Path;

use anyhow::{Context, Result};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

pub mod app;
pub mod catalogs;
pub mod library;
pub mod new_catalog;
pub mod palette;
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
