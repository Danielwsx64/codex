#![warn(clippy::all)]
#![deny(unsafe_code)]

pub mod catalog;
pub mod cli;
pub mod config;
pub mod import;
pub mod tui;
pub mod welcome;

use std::io::Write;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Command};

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match cli.command {
        Some(Command::Tui) => tui::run(cli.data_dir.as_deref()),
        Some(Command::Catalog(cmd)) => {
            cli::catalog::dispatch(cmd, cli.data_dir.as_deref(), cli.json)
        }
        Some(Command::Add { paths }) => cli::books::dispatch_add(
            paths,
            cli.data_dir.as_deref(),
            cli.catalog.as_deref(),
            cli.json,
        ),
        Some(Command::Ls) => {
            cli::books::dispatch_ls(cli.data_dir.as_deref(), cli.catalog.as_deref(), cli.json)
        }
        Some(Command::Inspect { target }) => cli::books::dispatch_inspect(
            target,
            cli.data_dir.as_deref(),
            cli.catalog.as_deref(),
            cli.json,
        ),
        Some(Command::Rm { target, keep }) => cli::books::dispatch_rm(
            target,
            keep,
            cli.data_dir.as_deref(),
            cli.catalog.as_deref(),
            cli.json,
        ),
        None => print_welcome(),
    }
}

fn init_tracing(verbose: u8) {
    let default = match verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

fn print_welcome() -> Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    welcome::render_plain(&mut out)?;
    out.flush()?;
    Ok(())
}
