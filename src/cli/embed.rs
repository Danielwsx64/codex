use std::io::{self, Write};
use std::path::Path;

use anyhow::{Context, Result};

use crate::catalog::{self, books};
use crate::embed::job::{Job, StepOutcome};

pub fn dispatch_sync(data_dir: Option<&Path>, catalog_override: Option<&str>) -> Result<()> {
    let registry = crate::cli::books::load(data_dir)?;
    let entry = crate::cli::books::resolve_entry(&registry, catalog_override)?.clone();
    let conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;

    let pending = books::fetch_pending(&conn).context("while listing pending books")?;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if pending.is_empty() {
        writeln!(out, "nothing to sync (0 books pending).")?;
        return Ok(());
    }

    let mut job = Job::from_books(&pending, &entry.path);
    let total = job.total;

    // Drain pre-classified failures (e.g. mobi/azw3) first so the user sees them.
    let mut errors = 0usize;
    for failure in &job.failures.clone() {
        writeln!(
            out,
            "[{completed}/{total}] {title} — {reason}",
            completed = job.completed,
            title = failure.title,
            reason = failure.reason,
        )?;
        // mirror to the DB so we don't re-attempt next run
        let _ = books::mark_embed_unsupported(&conn, failure.id);
    }

    while job.is_pending() {
        // Position before advance — completed counts items already done.
        let position = job.completed + 1;
        let Some(step) = job.advance(&conn) else {
            break;
        };
        match step {
            StepOutcome::Synced { title, .. } => {
                writeln!(out, "[{position}/{total}] {title} — synced")?;
            }
            StepOutcome::Unsupported { title, format, .. } => {
                writeln!(
                    out,
                    "[{position}/{total}] {title} — unsupported ({})",
                    format.label()
                )?;
            }
            StepOutcome::Error(failure) => {
                writeln!(
                    out,
                    "[{position}/{total}] {title} — error: {reason}",
                    title = failure.title,
                    reason = failure.reason,
                )?;
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
    writeln!(
        out,
        "Done: {synced} synced, {unsupported} unsupported, {errors} error{plural}.",
        plural = if errors == 1 { "" } else { "s" }
    )?;
    out.flush()?;
    Ok(())
}
