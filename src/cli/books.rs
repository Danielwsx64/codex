use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

use crate::catalog::columns::LibraryColumn;
use crate::catalog::{self, books, render, tags};
use crate::cli::LibraryColumnArgs;
use crate::config::paths::resolve_config_dir;
use crate::config::{self, CatalogEntry, Registry};

pub fn dispatch_add(
    paths: Vec<PathBuf>,
    force: bool,
    data_dir: Option<&Path>,
    catalog_override: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = load(data_dir)?;
    let entry = resolve_entry(&registry, catalog_override)?.clone();
    let mut conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;
    let outcome = books::handle_add(&mut conn, &entry.path, &paths, force);
    render::emit(
        json,
        |w| render::render_book_add_human(&outcome, w),
        |w| render::render_book_add_jsonl(&outcome, w),
    )?;
    if outcome.any_failed() && !outcome.any_imported() {
        anyhow::bail!("no files were imported");
    }
    Ok(())
}

pub fn dispatch_ls(
    view: LibraryColumnArgs,
    data_dir: Option<&Path>,
    catalog_override: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = load(data_dir)?;
    let entry = resolve_entry(&registry, catalog_override)?.clone();
    let conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;
    let rows = books::handle_ls(&conn).context("while listing books")?;

    let selection = resolve_columns(view.columns.as_deref(), view.all_columns, json)?;
    render::emit(
        json,
        |w| render::render_book_ls_human(&rows, &selection, w),
        |w| render::render_book_ls_jsonl(&rows, &selection, w),
    )?;
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
    render::emit(
        json,
        |w| render::render_book_inspect_human(&book, &absolute_path, w),
        |w| render::render_book_inspect_jsonl(&book, &absolute_path, w),
    )?;
    Ok(())
}

#[derive(Debug, Default)]
pub struct SearchInput {
    pub query: Option<String>,
    pub author: Option<String>,
    pub tag: Vec<String>,
    pub series: Option<String>,
    pub rating: Option<books::RatingRange>,
    pub view: LibraryColumnArgs,
}

pub fn dispatch_search(
    input: SearchInput,
    data_dir: Option<&Path>,
    catalog_override: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = load(data_dir)?;
    let entry = resolve_entry(&registry, catalog_override)?.clone();
    let conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;
    let filters = books::SearchFilters {
        query: input.query.as_deref(),
        author: input.author.as_deref(),
        tags: &input.tag,
        series: input.series.as_deref(),
        rating: input.rating,
    };
    let rows = books::handle_search(&conn, &filters)
        .with_context(|| "while searching the catalog".to_string())?;

    let selection = resolve_columns(input.view.columns.as_deref(), input.view.all_columns, json)?;
    render::emit(
        json,
        |w| render::render_book_ls_human(&rows, &selection, w),
        |w| render::render_book_ls_jsonl(&rows, &selection, w),
    )?;
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
    render::emit(
        json,
        |w| render::render_book_rm_human(&outcome, w),
        |w| render::render_book_rm_jsonl(&outcome, w),
    )?;
    Ok(())
}

pub fn dispatch_tag(
    target: String,
    raw_tags: Vec<String>,
    data_dir: Option<&Path>,
    catalog_override: Option<&str>,
    json: bool,
) -> Result<()> {
    let names = tags::normalize_many(raw_tags);
    if names.is_empty() {
        anyhow::bail!("no valid tag names given");
    }
    let registry = load(data_dir)?;
    let entry = resolve_entry(&registry, catalog_override)?.clone();
    let mut conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;
    let outcome = books::handle_tag_add(&mut conn, &target, &names)
        .with_context(|| format!("while tagging `{target}`"))?;
    render::emit(
        json,
        |w| render::render_tag_human(&outcome, w),
        |w| render::render_tag_jsonl(&outcome, w),
    )?;
    Ok(())
}

pub fn dispatch_untag(
    target: String,
    raw_tags: Vec<String>,
    all: bool,
    data_dir: Option<&Path>,
    catalog_override: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = load(data_dir)?;
    let entry = resolve_entry(&registry, catalog_override)?.clone();
    let mut conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;

    let outcome = if all {
        books::handle_tag_clear(&mut conn, &target)
            .with_context(|| format!("while clearing tags for `{target}`"))?
    } else {
        let names = tags::normalize_many(raw_tags);
        if names.is_empty() {
            anyhow::bail!("no valid tag names given (use --all to clear every tag)");
        }
        books::handle_tag_remove(&mut conn, &target, &names)
            .with_context(|| format!("while untagging `{target}`"))?
    };

    render::emit(
        json,
        |w| render::render_untag_human(&outcome, w),
        |w| render::render_untag_jsonl(&outcome, w),
    )?;
    Ok(())
}

pub fn dispatch_rate(
    target: String,
    rating: u8,
    data_dir: Option<&Path>,
    catalog_override: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = load(data_dir)?;
    let entry = resolve_entry(&registry, catalog_override)?.clone();
    let mut conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;
    let outcome = books::handle_rate(&mut conn, &target, rating)
        .with_context(|| format!("while rating `{target}`"))?;
    render::emit(
        json,
        |w| render::render_rate_human(&outcome, w),
        |w| render::render_rate_jsonl(&outcome, w),
    )?;
    Ok(())
}

pub fn dispatch_series(
    target: String,
    name: Option<String>,
    index: Option<f64>,
    clear: bool,
    data_dir: Option<&Path>,
    catalog_override: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = load(data_dir)?;
    let entry = resolve_entry(&registry, catalog_override)?.clone();
    let mut conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;
    let outcome = books::handle_series(&mut conn, &target, name.as_deref(), index, clear)
        .with_context(|| format!("while setting series for `{target}`"))?;
    render::emit(
        json,
        |w| render::render_series_human(&outcome, w),
        |w| render::render_series_jsonl(&outcome, w),
    )?;
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
