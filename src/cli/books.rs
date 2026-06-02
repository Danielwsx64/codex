use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

use crate::catalog::columns::LibraryColumn;
use crate::catalog::{self, books, render};
use crate::config::paths::resolve_config_dir;
use crate::config::{self, CatalogEntry, Registry};

pub fn dispatch_add(
    paths: Vec<PathBuf>,
    data_dir: Option<&Path>,
    catalog_override: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = load(data_dir)?;
    let entry = resolve_entry(&registry, catalog_override)?.clone();
    let mut conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;
    let outcome = books::handle_add(&mut conn, &entry.path, &paths);
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if json {
        render::render_book_add_jsonl(&outcome, &mut out)?;
    } else {
        render::render_book_add_human(&outcome, &mut out)?;
    }
    out.flush()?;
    if outcome.any_failed() && !outcome.any_imported() {
        anyhow::bail!("no files were imported");
    }
    Ok(())
}

pub fn dispatch_ls(
    columns: Option<String>,
    all_columns: bool,
    data_dir: Option<&Path>,
    catalog_override: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = load(data_dir)?;
    let entry = resolve_entry(&registry, catalog_override)?.clone();
    let conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;
    let rows = books::handle_ls(&conn).context("while listing books")?;

    let selection = resolve_columns(columns.as_deref(), all_columns, json)?;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if json {
        render::render_book_ls_jsonl(&rows, &selection, &mut out)?;
    } else {
        render::render_book_ls_human(&rows, &selection, &mut out)?;
    }
    out.flush()?;
    Ok(())
}

fn resolve_columns(
    columns: Option<&str>,
    all_columns: bool,
    json: bool,
) -> Result<Vec<LibraryColumn>> {
    if let Some(list) = columns {
        let mut out: Vec<LibraryColumn> = Vec::new();
        for slug in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let col = LibraryColumn::from_slug(slug).ok_or_else(|| {
                anyhow!(
                    "unknown column `{slug}`; available: {}",
                    LibraryColumn::ALL
                        .iter()
                        .map(|c| c.slug())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?;
            out.push(col);
        }
        if out.is_empty() {
            return Err(anyhow!("--columns requires at least one slug"));
        }
        return Ok(out);
    }
    if all_columns {
        return Ok(LibraryColumn::ALL.to_vec());
    }
    // No explicit selection: JSON keeps wide coverage so existing consumers
    // see every catalog field; the human table sticks to the compact default.
    if json {
        Ok(LibraryColumn::ALL.to_vec())
    } else {
        Ok(LibraryColumn::DEFAULT.to_vec())
    }
}

pub fn dispatch_inspect(
    target: String,
    data_dir: Option<&Path>,
    catalog_override: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = load(data_dir)?;
    let entry = resolve_entry(&registry, catalog_override)?.clone();
    let conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;
    let book = books::handle_inspect(&conn, &target)
        .with_context(|| format!("while inspecting `{target}`"))?;
    let absolute_path = entry.path.join(&book.file_path);
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if json {
        render::render_book_inspect_jsonl(&book, &absolute_path, &mut out)?;
    } else {
        render::render_book_inspect_human(&book, &absolute_path, &mut out)?;
    }
    out.flush()?;
    Ok(())
}

pub fn dispatch_rm(
    target: String,
    keep: bool,
    data_dir: Option<&Path>,
    catalog_override: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = load(data_dir)?;
    let entry = resolve_entry(&registry, catalog_override)?.clone();
    let mut conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;
    let outcome = books::handle_rm(&mut conn, &entry.path, &target, keep)
        .with_context(|| format!("while removing `{target}`"))?;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if json {
        render::render_book_rm_jsonl(&outcome, &mut out)?;
    } else {
        render::render_book_rm_human(&outcome, &mut out)?;
    }
    out.flush()?;
    Ok(())
}

pub(crate) fn load(data_dir: Option<&Path>) -> Result<Registry> {
    let config_dir =
        resolve_config_dir(data_dir).context("failed to resolve the cdx config directory")?;
    Registry::load(&config_dir).with_context(|| {
        format!(
            "failed to load catalog registry from {}",
            config_dir.join(config::CONFIG_FILENAME).display()
        )
    })
}

pub(crate) fn resolve_entry<'a>(
    registry: &'a Registry,
    catalog_override: Option<&str>,
) -> Result<&'a CatalogEntry> {
    registry
        .resolve(catalog_override)
        .map_err(|e| anyhow::Error::new(e).context("no catalog selected"))
}
