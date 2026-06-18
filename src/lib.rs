#![warn(clippy::all)]
#![deny(unsafe_code)]

pub mod catalog;
pub mod cli;
pub mod config;
pub mod device;
pub mod embed;
pub mod epub;
pub mod fingerprint;
pub mod import;
pub mod matching;
pub mod reader;
pub mod tui;
pub mod welcome;

use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};

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
        Some(Command::Push { target, device }) => cli::device::dispatch_push(
            target.as_deref(),
            device.as_deref(),
            cli.data_dir.as_deref(),
            cli.catalog.as_deref(),
            cli.json,
        ),
        Some(Command::Pull {
            path,
            device,
            force,
        }) => cli::device::dispatch_pull(
            path.as_deref(),
            device.as_deref(),
            force,
            cli.data_dir.as_deref(),
            cli.catalog.as_deref(),
            cli.json,
        ),
        Some(Command::Device(cmd)) => cli::device::dispatch(
            cmd,
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
        .with_writer(log_writer)
        .try_init();
}

static TUI_OWNS_TERMINAL: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_tui_owns_terminal(owned: bool) {
    TUI_OWNS_TERMINAL.store(owned, Ordering::Relaxed);
}

// While the TUI holds the terminal (raw mode + alternate screen), a log line
// written to a tty stderr smears over the rendered UI. Drop those instead; a
// redirected stderr (`cdx tui 2>cdx.log`) still receives everything.
fn silence_logs(tui_owns_terminal: bool, stderr_is_tty: bool) -> bool {
    tui_owns_terminal && stderr_is_tty
}

enum LogWriter {
    Stderr(std::io::Stderr),
    Sink,
}

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            LogWriter::Stderr(out) => out.write(buf),
            LogWriter::Sink => Ok(buf.len()),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            LogWriter::Stderr(out) => out.flush(),
            LogWriter::Sink => Ok(()),
        }
    }
}

fn log_writer() -> LogWriter {
    let owned = TUI_OWNS_TERMINAL.load(Ordering::Relaxed);
    if silence_logs(owned, std::io::stderr().is_terminal()) {
        LogWriter::Sink
    } else {
        LogWriter::Stderr(std::io::stderr())
    }
}

fn print_welcome() -> Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    welcome::render_plain(&mut out)?;
    out.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_silenced_only_when_tui_owns_a_tty_stderr() {
        assert!(silence_logs(true, true));
        assert!(!silence_logs(true, false), "redirected stderr keeps logs");
        assert!(!silence_logs(false, true), "CLI mode keeps logs");
        assert!(!silence_logs(false, false));
    }

    #[test]
    fn sink_writer_swallows_bytes_without_error() {
        let mut sink = LogWriter::Sink;
        assert_eq!(sink.write(b"warn line").unwrap(), 9);
        sink.flush().unwrap();
    }
}
