use std::io::{self, StdoutLock, Write};
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::catalog::{self, books};
use crate::embed::job::{Job, StepOutcome};

pub fn dispatch_sync(
    data_dir: Option<&Path>,
    catalog_override: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = crate::cli::books::load(data_dir)?;
    let entry = crate::cli::books::resolve_entry(&registry, catalog_override)?.clone();
    let conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;

    let pending = books::fetch_pending(&conn).context("while listing pending books")?;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if pending.is_empty() {
        if json {
            write_summary_jsonl(&mut out, 0, 0, 0, 0)?;
        } else {
            writeln!(out, "nothing to sync (0 books pending).")?;
        }
        out.flush()?;
        return Ok(());
    }

    let mut job = Job::from_books(&pending, &entry.path);
    let total = job.total;

    // Drain pre-classified failures (e.g. mobi/azw3) first so the user sees them.
    let mut errors = 0usize;
    for failure in &job.failures.clone() {
        if json {
            write_step_jsonl(
                &mut out,
                "unsupported",
                failure.id,
                &failure.title,
                None,
                Some(&failure.reason),
                job.completed,
                total,
            )?;
        } else {
            writeln!(
                out,
                "[{completed}/{total}] {title} — {reason}",
                completed = job.completed,
                title = failure.title,
                reason = failure.reason,
            )?;
        }
        let _ = books::mark_embed_unsupported(&conn, failure.id);
    }

    while job.is_pending() {
        // Position before advance — completed counts items already done.
        let position = job.completed + 1;
        let Some(step) = job.advance(&conn) else {
            break;
        };
        match step {
            StepOutcome::Synced { id, title } => {
                if json {
                    write_step_jsonl(&mut out, "synced", id, &title, None, None, position, total)?;
                } else {
                    writeln!(out, "[{position}/{total}] {title} — synced")?;
                }
            }
            StepOutcome::Unsupported { id, title, format } => {
                if json {
                    write_step_jsonl(
                        &mut out,
                        "unsupported",
                        id,
                        &title,
                        Some(format.label()),
                        None,
                        position,
                        total,
                    )?;
                } else {
                    writeln!(
                        out,
                        "[{position}/{total}] {title} — unsupported ({})",
                        format.label()
                    )?;
                }
            }
            StepOutcome::Error(failure) => {
                if json {
                    write_step_jsonl(
                        &mut out,
                        "error",
                        failure.id,
                        &failure.title,
                        None,
                        Some(&failure.reason),
                        position,
                        total,
                    )?;
                } else {
                    writeln!(
                        out,
                        "[{position}/{total}] {title} — error: {reason}",
                        title = failure.title,
                        reason = failure.reason,
                    )?;
                }
                errors += 1;
            }
        }
    }

    let unsupported = job
        .failures
        .iter()
        .filter(|f| f.reason.starts_with("embed not supported"))
        .count();
    let synced = job.succeeded;

    if json {
        write_summary_jsonl(&mut out, total, synced, unsupported, errors)?;
    } else {
        writeln!(
            out,
            "Done: {synced} synced, {unsupported} unsupported, {errors} error{plural}.",
            plural = if errors == 1 { "" } else { "s" }
        )?;
    }
    out.flush()?;
    Ok(())
}

#[derive(Serialize)]
struct EmbedStepJson<'a> {
    action: &'static str,
    status: &'a str,
    id: i64,
    title: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
    position: usize,
    total: usize,
}

#[derive(Serialize)]
struct EmbedSummaryJson {
    action: &'static str,
    total: usize,
    synced: usize,
    unsupported: usize,
    errors: usize,
}

#[allow(clippy::too_many_arguments)]
fn write_step_jsonl(
    w: &mut StdoutLock<'_>,
    status: &str,
    id: i64,
    title: &str,
    format: Option<&str>,
    reason: Option<&str>,
    position: usize,
    total: usize,
) -> io::Result<()> {
    let value = EmbedStepJson {
        action: "embed_step",
        status,
        id,
        title,
        format,
        reason,
        position,
        total,
    };
    serde_json::to_writer(&mut *w, &value)?;
    writeln!(w)
}

fn write_summary_jsonl(
    w: &mut StdoutLock<'_>,
    total: usize,
    synced: usize,
    unsupported: usize,
    errors: usize,
) -> io::Result<()> {
    let value = EmbedSummaryJson {
        action: "embed_summary",
        total,
        synced,
        unsupported,
        errors,
    };
    serde_json::to_writer(&mut *w, &value)?;
    writeln!(w)
}
