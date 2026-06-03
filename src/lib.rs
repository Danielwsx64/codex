#![warn(clippy::all)]
#![deny(unsafe_code)]

pub mod catalog;
pub mod cli;
pub mod config;
pub mod embed;
pub mod epub;
pub mod fingerprint;
pub mod import;
pub mod reader;
pub mod tui;
pub mod welcome;

use std::io::Write;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Command, EmbedCmd};

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match cli.command {
        Some(Command::Tui) => tui::run(cli.data_dir.as_deref()),
        Some(Command::Catalog(cmd)) => {
            cli::catalog::dispatch(cmd, cli.data_dir.as_deref(), cli.json)
        }
        Some(Command::Add { paths, force }) => cli::books::dispatch_add(
            paths,
            force,
            cli.data_dir.as_deref(),
            cli.catalog.as_deref(),
            cli.json,
        ),
        Some(Command::Ls { view }) => cli::books::dispatch_ls(
            view,
            cli.data_dir.as_deref(),
            cli.catalog.as_deref(),
            cli.json,
        ),
        Some(Command::Inspect { target }) => cli::books::dispatch_inspect(
            target,
            cli.data_dir.as_deref(),
            cli.catalog.as_deref(),
            cli.json,
        ),
        Some(Command::Search {
            query,
            author,
            tag,
            series,
            rating,
            view,
        }) => cli::books::dispatch_search(
            cli::books::SearchInput {
                query,
                author,
                tag,
                series,
                rating,
                view,
            },
            cli.data_dir.as_deref(),
            cli.catalog.as_deref(),
            cli.json,
        ),
        Some(Command::Edit { target }) => cli::edit::dispatch(
            target,
            cli.data_dir.as_deref(),
            cli.catalog.as_deref(),
            cli.json,
        ),
        Some(Command::Tag { target, tags }) => cli::books::dispatch_tag(
            target,
            tags,
            cli.data_dir.as_deref(),
            cli.catalog.as_deref(),
            cli.json,
        ),
        Some(Command::Rate { target, rating }) => cli::books::dispatch_rate(
            target,
            rating,
            cli.data_dir.as_deref(),
            cli.catalog.as_deref(),
            cli.json,
        ),
        Some(Command::Series {
            target,
            name,
            index,
            clear,
        }) => cli::books::dispatch_series(
            target,
            name,
            index,
            clear,
            cli.data_dir.as_deref(),
            cli.catalog.as_deref(),
            cli.json,
        ),
        Some(Command::Untag { target, tags, all }) => cli::books::dispatch_untag(
            target,
            tags,
            all,
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
        Some(Command::Embed(EmbedCmd::Sync)) => {
            cli::embed::dispatch_sync(cli.data_dir.as_deref(), cli.catalog.as_deref(), cli.json)
        }
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
