#![warn(clippy::all)]
#![deny(unsafe_code)]

pub mod tui;
pub mod welcome;

use std::io::Write;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "cdx",
    version,
    about = "Terminal-first ebook library and ereader manager"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Tui,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Tui) => tui::run(),
        None => print_welcome(),
    }
}

fn print_welcome() -> Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    welcome::render_plain(&mut out)?;
    out.flush()?;
    Ok(())
}
