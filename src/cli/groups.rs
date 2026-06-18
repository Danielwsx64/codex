use std::path::Path;

use anyhow::{Context, Result};

use crate::catalog::groups::{self, GroupBy};
use crate::catalog::{self, render};
use crate::cli::books::{load, resolve_entry};

pub fn dispatch(
    by: GroupBy,
    data_dir: Option<&Path>,
    catalog_override: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = load(data_dir)?;
    let entry = resolve_entry(&registry, catalog_override)?.clone();
    let conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;

    let rows = groups::list_groups(&conn, by)
        .with_context(|| format!("while grouping books by {}", by.as_str()))?;

    render::emit(
        json,
        |w| render::render_groups_human(by, &rows, w),
        |w| render::render_groups_jsonl(&rows, w),
    )?;
    Ok(())
}
