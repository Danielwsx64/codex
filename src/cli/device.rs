use std::io::IsTerminal;
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
