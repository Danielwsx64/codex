use std::io::{IsTerminal, Write};
use std::path::Path;

use anyhow::{Context, Result};

use crate::catalog::books::{self as catalog_books, Book};
use crate::catalog::{self, devices, render};
use crate::cli::books::{load, resolve_entry};
use crate::cli::DeviceCmd;
use crate::device;
use crate::device::books::DeviceBook;

pub fn dispatch(
    cmd: DeviceCmd,
    data_dir: Option<&Path>,
    catalog_override: Option<&str>,
    json: bool,
) -> Result<()> {
    match cmd {
        DeviceCmd::Ls => dispatch_ls(data_dir, catalog_override, json),
        DeviceCmd::Alias { target, new_alias } => {
            dispatch_alias(&target, &new_alias, data_dir, catalog_override, json)
        }
        DeviceCmd::Books { device } => {
            dispatch_books(device.as_deref(), data_dir, catalog_override, json)
        }
    }
}

fn dispatch_books(
    device: Option<&str>,
    data_dir: Option<&Path>,
    catalog_override: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = load(data_dir)?;
    let entry = resolve_entry(&registry, catalog_override)?.clone();
    let conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;

    let detected = device::detect();
    // Mirror `ls`: persist every detected device so aliases/last_seen stay fresh.
    for found in &detected {
        devices::record_seen(&conn, &found.serial)
            .with_context(|| format!("while recording device `{}`", found.serial))?;
    }

    let target = device::resolve_target(&conn, &detected, device)?;
    let books = device::books::list(&conn, &target.serial, &target.mount_path)
        .with_context(|| format!("while listing books on device `{}`", target.serial))?;

    render::emit(
        json,
        |w| render::render_device_books_human(&books, w),
        |w| render::render_device_books_jsonl(&books, w),
    )?;
    Ok(())
}

pub fn dispatch_push(
    target: Option<&str>,
    device: Option<&str>,
    data_dir: Option<&Path>,
    catalog_override: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = load(data_dir)?;
    let entry = resolve_entry(&registry, catalog_override)?.clone();
    let conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;

    let detected = device::detect();
    // Mirror the other device commands: keep aliases/last_seen fresh.
    for found in &detected {
        devices::record_seen(&conn, &found.serial)
            .with_context(|| format!("while recording device `{}`", found.serial))?;
    }

    let target_device = device::resolve_target(&conn, &detected, device)?;
    let label = device_label(&conn, &target_device.serial);

    let chosen = match target {
        Some(t) => t.to_string(),
        None => {
            let books = catalog_books::handle_ls(&conn).context("while listing catalog books")?;
            match pick_or_bail("Push to device — pick a book", json, &books, catalog_label)? {
                Some(i) => books[i].id.to_string(),
                // Cancelled: nothing to push, and not an error.
                None => return Ok(()),
            }
        }
    };

    let outcome = device::push::push(
        &conn,
        &entry.path,
        &target_device.serial,
        &target_device.mount_path,
        &chosen,
    )
    .with_context(|| format!("while pushing `{chosen}` to device `{label}`"))?;

    render::emit(
        json,
        |w| render::render_push_human(&outcome, &label, w),
        |w| render::render_push_jsonl(&outcome, &target_device.serial, w),
    )?;
    Ok(())
}

pub fn dispatch_pull(
    path: Option<&str>,
    device: Option<&str>,
    force: bool,
    data_dir: Option<&Path>,
    catalog_override: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = load(data_dir)?;
    let entry = resolve_entry(&registry, catalog_override)?.clone();
    let mut conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;

    let detected = device::detect();
    // Mirror the other device commands: keep aliases/last_seen fresh.
    for found in &detected {
        devices::record_seen(&conn, &found.serial)
            .with_context(|| format!("while recording device `{}`", found.serial))?;
    }

    let target_device = device::resolve_target(&conn, &detected, device)?;
    let label = device_label(&conn, &target_device.serial);

    let chosen = match path {
        Some(p) => p.to_string(),
        None => {
            let books =
                device::books::list(&conn, &target_device.serial, &target_device.mount_path)
                    .with_context(|| format!("while listing books on device `{label}`"))?;
            match pick_or_bail(
                "Pull from device — pick a book",
                json,
                &books,
                device_book_label,
            )? {
                Some(i) => books[i].device_path.display().to_string(),
                // Cancelled: nothing to pull, and not an error.
                None => return Ok(()),
            }
        }
    };

    let outcome = device::pull::pull(
        &mut conn,
        &entry.path,
        &target_device.serial,
        &target_device.mount_path,
        &chosen,
        force,
    )
    .with_context(|| format!("while pulling `{chosen}` from device `{label}`"))?;

    render::emit(
        json,
        |w| render::render_pull_human(&outcome, &label, w),
        |w| render::render_pull_jsonl(&outcome, &target_device.serial, w),
    )?;
    Ok(())
}

pub fn dispatch_sync(
    device: Option<&str>,
    dry_run: bool,
    yes: bool,
    verify: bool,
    data_dir: Option<&Path>,
    catalog_override: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = load(data_dir)?;
    let entry = resolve_entry(&registry, catalog_override)?.clone();
    let mut conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;

    let detected = device::detect();
    // Mirror the other device commands: keep aliases/last_seen fresh.
    for found in &detected {
        devices::record_seen(&conn, &found.serial)
            .with_context(|| format!("while recording device `{}`", found.serial))?;
    }

    let target = device::resolve_target(&conn, &detected, device)?;
    let label = device_label(&conn, &target.serial);

    let plan = device::sync::diff(&conn, &target.serial, &target.mount_path, verify)
        .with_context(|| format!("while computing the sync plan for `{label}`"))?;

    if dry_run {
        render::emit(
            json,
            |w| render::render_sync_plan_human(&plan, w),
            |w| render::render_sync_plan_jsonl(&plan, w),
        )?;
        return Ok(());
    }

    // Applying needs a terminal to confirm each item; `--json` or a piped stdout
    // must opt in with `--yes` (or use `--dry-run` to just see the plan).
    let interactive = !json && std::io::stdout().is_terminal();
    if !yes && !interactive {
        anyhow::bail!(
            "refusing to sync non-interactively; pass --yes to apply or --dry-run to preview"
        );
    }

    if plan.is_empty() {
        if !json {
            println!("Already in sync.");
        }
        return Ok(());
    }

    let mut pushed = 0usize;
    let mut pulled = 0usize;
    let mut skipped = 0usize;
    let mut accept_all = yes;

    for item in &plan.items {
        if !accept_all {
            match confirm(item)? {
                Decision::Yes => {}
                Decision::No => {
                    skipped += 1;
                    continue;
                }
                Decision::All => accept_all = true,
                // Abort: sync never deletes and stops cleanly, leaving the rest
                // of the plan untouched.
                Decision::Quit => break,
            }
        }
        match item.direction {
            device::sync::Direction::Push => {
                let book_id = item
                    .book_id
                    .expect("sync push items always carry the catalog book id");
                let outcome = device::push::push(
                    &conn,
                    &entry.path,
                    &target.serial,
                    &target.mount_path,
                    &book_id.to_string(),
                )
                .with_context(|| format!("while pushing `{}` to `{label}`", item.title))?;
                render::emit(
                    json,
                    |w| render::render_push_human(&outcome, &label, w),
                    |w| render::render_push_jsonl(&outcome, &target.serial, w),
                )?;
                pushed += 1;
            }
            device::sync::Direction::Pull => {
                let arg = item.device_path.display().to_string();
                let outcome = device::pull::pull(
                    &mut conn,
                    &entry.path,
                    &target.serial,
                    &target.mount_path,
                    &arg,
                    false,
                )
                .with_context(|| format!("while pulling `{arg}` from `{label}`"))?;
                render::emit(
                    json,
                    |w| render::render_pull_human(&outcome, &label, w),
                    |w| render::render_pull_jsonl(&outcome, &target.serial, w),
                )?;
                pulled += 1;
            }
        }
    }

    // Conflicts are real ambiguity (a device file matching 2+ catalog books) and
    // are never applied automatically — surface them so the user resolves by hand.
    if !plan.conflicts.is_empty() && !json {
        eprintln!(
            "\n{} conflict(s) skipped; resolve manually (see `cdx sync --dry-run`).",
            plan.conflicts.len()
        );
    }
    if !json {
        println!("\nSynced: {pushed} pushed, {pulled} pulled, {skipped} skipped.");
    }
    Ok(())
}

// One step of the `git add -p`-style confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    Yes,
    No,
    All,
    Quit,
}

fn parse_decision(input: &str) -> Option<Decision> {
    match input.trim() {
        "y" | "Y" => Some(Decision::Yes),
        "n" | "N" => Some(Decision::No),
        "a" | "A" => Some(Decision::All),
        "q" | "Q" => Some(Decision::Quit),
        _ => None,
    }
}

// Prompt for a single plan item, reprompting on unrecognized input. EOF on stdin
// is treated as quit so a closed pipe can't spin forever.
fn confirm(item: &device::sync::SyncItem) -> Result<Decision> {
    let verb = match item.direction {
        device::sync::Direction::Pull => "pull",
        device::sync::Direction::Push => "push",
    };
    let mut line = String::new();
    loop {
        print!("{verb} \"{}\"? [y]es/[n]o/[a]ll/[q]uit ", item.title);
        std::io::stdout().flush().ok();
        line.clear();
        if std::io::stdin().read_line(&mut line)? == 0 {
            return Ok(Decision::Quit);
        }
        if let Some(decision) = parse_decision(&line) {
            return Ok(decision);
        }
        println!("Please answer y, n, a, or q.");
    }
}

// Shared interactive selection for `push`/`pull` when the positional argument is
// omitted: render each row to a label and let the user pick one. The picker
// needs a terminal and is meaningless with machine output, so `--json` or a
// non-tty stdout is a hard error instead. An empty list is also an error (there
// is nothing to pick); a cancelled picker returns `Ok(None)`.
fn pick_or_bail<T>(
    title: &str,
    json: bool,
    rows: &[T],
    label: impl Fn(&T) -> String,
) -> Result<Option<usize>> {
    if json || !std::io::stdout().is_terminal() {
        anyhow::bail!(
            "no selection given; pass the argument or run in a terminal to pick interactively"
        );
    }
    if rows.is_empty() {
        anyhow::bail!("nothing to pick");
    }
    let labels: Vec<String> = rows.iter().map(label).collect();
    crate::tui::pick::pick(title, &labels)
}

fn catalog_label(book: &Book) -> String {
    let author = book.author.as_deref().unwrap_or("—");
    format!("#{} {} — {} ({})", book.id, book.title, author, book.format)
}

fn device_book_label(book: &DeviceBook) -> String {
    let title = book.title.as_deref().unwrap_or("(unknown title)");
    let author = book.author.as_deref().unwrap_or("—");
    format!(
        "[{}] {title} — {author} ({})  {}",
        book.presence.as_str(),
        book.format,
        book.device_path.display(),
    )
}

// Friendly name for messages: the alias when set, else the bare serial.
fn device_label(conn: &rusqlite::Connection, serial: &str) -> String {
    devices::list(conn)
        .unwrap_or_default()
        .into_iter()
        .find(|k| k.serial == serial)
        .and_then(|k| k.alias)
        .unwrap_or_else(|| serial.to_string())
}

fn dispatch_alias(
    target: &str,
    new_alias: &str,
    data_dir: Option<&Path>,
    catalog_override: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = load(data_dir)?;
    let entry = resolve_entry(&registry, catalog_override)?.clone();
    let conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;

    let outcome = devices::handle_alias(&conn, target, new_alias)
        .with_context(|| format!("while setting alias for `{target}`"))?;

    render::emit(
        json,
        |w| render::render_alias_human(&outcome, w),
        |w| render::render_alias_jsonl(&outcome, w),
    )?;
    Ok(())
}

fn dispatch_ls(data_dir: Option<&Path>, catalog_override: Option<&str>, json: bool) -> Result<()> {
    let registry = load(data_dir)?;
    let entry = resolve_entry(&registry, catalog_override)?.clone();
    let conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;

    let detected = device::detect();
    // Persist every detected device so first-seen ones appear in the listing
    // and `last_seen_at` refreshes; the upsert keeps any existing alias.
    for found in &detected {
        devices::record_seen(&conn, &found.serial)
            .with_context(|| format!("while recording device `{}`", found.serial))?;
    }

    let known = devices::list(&conn).context("while listing known devices")?;
    let mut rows = device::build_device_rows(&detected, &known);
    device::enrich(&mut rows);

    render::emit(
        json,
        |w| render::render_device_ls_human(&rows, w),
        |w| render::render_device_ls_jsonl(&rows, w),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_decision, Decision};

    #[test]
    fn parse_decision_accepts_each_key_either_case() {
        assert_eq!(parse_decision("y"), Some(Decision::Yes));
        assert_eq!(parse_decision("Y\n"), Some(Decision::Yes));
        assert_eq!(parse_decision(" n "), Some(Decision::No));
        assert_eq!(parse_decision("a"), Some(Decision::All));
        assert_eq!(parse_decision("Q\n"), Some(Decision::Quit));
    }

    #[test]
    fn parse_decision_rejects_unknown_input() {
        assert_eq!(parse_decision(""), None);
        assert_eq!(parse_decision("yes please"), None);
        assert_eq!(parse_decision("x"), None);
    }
}
