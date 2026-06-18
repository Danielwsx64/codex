use std::path::Path;

use anyhow::{Context, Result};

use crate::catalog::{self, devices, render};
use crate::cli::books::{load, resolve_entry};
use crate::cli::DeviceCmd;
use crate::device;

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
    target: &str,
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
    let outcome = device::push::push(
        &conn,
        &entry.path,
        &target_device.serial,
        &target_device.mount_path,
        target,
    )
    .with_context(|| format!("while pushing `{target}` to device `{label}`"))?;

    render::emit(
        json,
        |w| render::render_push_human(&outcome, &label, w),
        |w| render::render_push_jsonl(&outcome, &target_device.serial, w),
    )?;
    Ok(())
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
