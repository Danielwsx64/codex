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
    }
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
