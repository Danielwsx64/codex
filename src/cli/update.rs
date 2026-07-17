use std::io::{self, Write};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::update::{self, CheckOutcome};

#[derive(Serialize)]
struct CheckJson<'a> {
    current: &'a str,
    latest: &'a str,
    newer_available: bool,
    html_url: &'a str,
}

pub fn dispatch(check: bool, yes: bool, json: bool) -> Result<()> {
    let agent = update::build_agent();
    let outcome = update::check(&agent).context("failed to check for updates on GitHub")?;

    if json {
        emit_json(&outcome)?;
    }

    if !outcome.newer_available {
        if !json {
            println!("cdx is up to date (v{}).", outcome.current);
        }
        return Ok(());
    }

    if check {
        if !json {
            report_available(&outcome);
        }
        return Ok(());
    }

    // Installing: the human path always narrates; --json already emitted the plan.
    if !json {
        report_available(&outcome);
    }

    if !yes && !confirm(&outcome.latest)? {
        // Confirmation is a human-only gate; log to stderr, leave stdout clean.
        eprintln!("Update cancelled.");
        return Ok(());
    }

    let path = update::install(&agent, &outcome.release)
        .with_context(|| format!("failed to install cdx v{}", outcome.latest))?;

    if !json {
        println!("Updated cdx to v{} at {}.", outcome.latest, path.display());
    }
    Ok(())
}

fn emit_json(outcome: &CheckOutcome) -> Result<()> {
    let record = CheckJson {
        current: &outcome.current,
        latest: &outcome.latest,
        newer_available: outcome.newer_available,
        html_url: &outcome.release.html_url,
    };
    let stdout = io::stdout();
    let mut out = stdout.lock();
    serde_json::to_writer(&mut out, &record).context("failed to serialize update status")?;
    writeln!(out)?;
    out.flush()?;
    Ok(())
}

fn report_available(outcome: &CheckOutcome) {
    println!(
        "A new cdx release is available: v{} (current v{}).",
        outcome.latest, outcome.current
    );
    if !outcome.release.html_url.is_empty() {
        println!("Release notes: {}", outcome.release.html_url);
    }
}

// One-shot confirmation before self-replacing the binary. Default is No: a bare
// Enter or EOF (closed pipe) cancels rather than installs.
fn confirm(latest: &str) -> Result<bool> {
    print!("Install cdx v{latest} now? [y/N] ");
    io::stdout().flush().ok();
    let mut line = String::new();
    if io::stdin().read_line(&mut line)? == 0 {
        return Ok(false);
    }
    Ok(matches!(line.trim(), "y" | "Y"))
}
