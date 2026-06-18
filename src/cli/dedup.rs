use std::io::IsTerminal;
use std::path::Path;

use anyhow::{Context, Result};

use crate::catalog::books::Book;
use crate::catalog::render::{DedupGroupView, DedupMemberView};
use crate::catalog::{self, books, render};
use crate::cli::books::{load, resolve_entry};
use crate::dedup::{self, DetectBy, DuplicateGroup};

pub fn dispatch(
    by: DetectBy,
    rm: bool,
    yes: bool,
    keep: bool,
    data_dir: Option<&Path>,
    catalog_override: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = load(data_dir)?;
    let entry = resolve_entry(&registry, catalog_override)?.clone();
    let mut conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;

    // Backfill fingerprints for any old book missing them so the hash signal
    // works catalog-wide. Best-effort: a failure here just narrows detection.
    if let Err(err) = books::ensure_fingerprints(&conn, &entry.path) {
        tracing::warn!(error = %err, "fingerprint backfill failed; hash detection may be incomplete");
    }

    let everything = books::handle_ls(&conn).context("while loading books")?;
    let hashes = books::load_all_hashes(&conn).context("while loading content hashes")?;
    let groups = dedup::find_duplicate_groups(&everything, &hashes, by);

    // Removal is an interactive/scripted action, never a machine-output one.
    if rm || yes {
        if json {
            anyhow::bail!("--rm/--yes remove books and cannot be combined with --json");
        }
        return remove(&mut conn, &entry.path, &everything, &groups, yes, keep);
    }

    let views = build_views(&everything, &groups);
    render::emit(
        json,
        |w| render::render_dedup_human(&views, w),
        |w| render::render_dedup_jsonl(&views, w),
    )?;
    Ok(())
}

// Builds the renderer view from the domain groups, looking up book data by id.
fn build_views<'a>(books: &'a [Book], groups: &[DuplicateGroup]) -> Vec<DedupGroupView<'a>> {
    let by_id: std::collections::HashMap<i64, &Book> = books.iter().map(|b| (b.id, b)).collect();
    groups
        .iter()
        .map(|g| DedupGroupView {
            reason: g.reason,
            linked_by_hash: g.linked_by_hash,
            linked_by_meta: g.linked_by_meta,
            members: g
                .members
                .iter()
                .filter_map(|id| by_id.get(id).copied())
                .map(|book| DedupMemberView {
                    book,
                    score: dedup::completeness_score(book),
                    suggested: book.id == g.suggested,
                })
                .collect(),
        })
        .collect()
}

fn remove(
    conn: &mut rusqlite::Connection,
    catalog_dir: &Path,
    books_list: &[Book],
    groups: &[DuplicateGroup],
    yes: bool,
    keep: bool,
) -> Result<()> {
    if groups.is_empty() {
        println!("No duplicate books found.");
        return Ok(());
    }

    let ids: Vec<i64> = if yes {
        // Script path: accept every suggestion without prompting.
        groups.iter().map(|g| g.suggested).collect()
    } else {
        // `--rm`: pick interactively. Needs a terminal.
        if !std::io::stdout().is_terminal() {
            anyhow::bail!(
                "--rm needs a terminal to pick interactively; pass --yes to accept all suggestions"
            );
        }
        let by_id: std::collections::HashMap<i64, &Book> =
            books_list.iter().map(|b| (b.id, b)).collect();
        let mut entries: Vec<i64> = Vec::new();
        let mut labels: Vec<String> = Vec::new();
        for (n, group) in groups.iter().enumerate() {
            for &id in &group.members {
                let Some(book) = by_id.get(&id) else { continue };
                let marker = if id == group.suggested {
                    "* suggested"
                } else {
                    "  keep     "
                };
                labels.push(format!(
                    "[group {}] {marker}  id {id}  {title} ({format})",
                    n + 1,
                    title = book.title,
                    format = book.format,
                ));
                entries.push(id);
            }
        }
        match crate::tui::pick::pick_multi("Duplicates — mark copies to remove", &labels)? {
            Some(indices) if !indices.is_empty() => {
                indices.into_iter().map(|i| entries[i]).collect()
            }
            // Cancelled or nothing marked: not an error, just nothing to do.
            _ => return Ok(()),
        }
    };

    for id in ids {
        let outcome = books::handle_rm(conn, catalog_dir, &id.to_string(), keep)
            .with_context(|| format!("while removing book id {id}"))?;
        render::render_book_rm_human(&outcome, &mut std::io::stdout())
            .context("while reporting removal")?;
    }
    Ok(())
}
