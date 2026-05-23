use std::io::{self, Write};
use std::path::Path;

use anyhow::{Context, Result};

use crate::catalog::handlers;
use crate::catalog::render;
use crate::cli::CatalogCmd;
use crate::config::paths::resolve_config_dir;
use crate::config::{self, Registry};

pub fn dispatch(cmd: CatalogCmd, data_dir: Option<&Path>, json: bool) -> Result<()> {
    let config_dir =
        resolve_config_dir(data_dir).context("failed to resolve the cdx config directory")?;

    match cmd {
        CatalogCmd::Init {
            name,
            path,
            description,
            no_switch,
        } => {
            let mut registry = load_registry(&config_dir)?;
            let outcome = handlers::handle_init(
                &mut registry,
                &config_dir,
                &name,
                &path,
                description,
                no_switch,
            )
            .with_context(|| {
                format!("while initializing catalog `{name}` at {}", path.display())
            })?;
            let stdout = io::stdout();
            let mut out = stdout.lock();
            if json {
                render::render_init_jsonl(&outcome, &mut out)?;
            } else {
                render::render_init_human(&outcome, &mut out)?;
            }
            out.flush()?;
            Ok(())
        }
        CatalogCmd::Add {
            name,
            path,
            description,
            no_switch,
        } => {
            let mut registry = load_registry(&config_dir)?;
            let outcome = handlers::handle_add(
                &mut registry,
                &config_dir,
                &name,
                &path,
                description,
                no_switch,
            )
            .with_context(|| format!("while registering catalog `{name}` at {}", path.display()))?;
            let stdout = io::stdout();
            let mut out = stdout.lock();
            if json {
                render::render_add_jsonl(&outcome, &mut out)?;
            } else {
                render::render_add_human(&outcome, &mut out)?;
            }
            out.flush()?;
            Ok(())
        }
        CatalogCmd::Ls => {
            let registry = load_registry(&config_dir)?;
            let rows = handlers::handle_ls(&registry);
            let stdout = io::stdout();
            let mut out = stdout.lock();
            if json {
                render::render_ls_jsonl(&rows, &mut out)?;
            } else {
                render::render_ls_human(&rows, &mut out)?;
            }
            out.flush()?;
            Ok(())
        }
        CatalogCmd::Use { name } => {
            let mut registry = load_registry(&config_dir)?;
            let outcome = handlers::handle_use(&mut registry, &config_dir, &name)
                .with_context(|| format!("while switching to catalog `{name}`"))?;
            let stdout = io::stdout();
            let mut out = stdout.lock();
            if json {
                render::render_use_jsonl(&outcome, &mut out)?;
            } else {
                render::render_use_human(&outcome, &mut out)?;
            }
            out.flush()?;
            Ok(())
        }
        CatalogCmd::Rm { name, purge } => {
            let mut registry = load_registry(&config_dir)?;
            let outcome = handlers::handle_rm(&mut registry, &config_dir, &name, purge)
                .with_context(|| format!("while removing catalog `{name}`"))?;
            let stdout = io::stdout();
            let mut out = stdout.lock();
            if json {
                render::render_rm_jsonl(&outcome, &mut out)?;
            } else {
                render::render_rm_human(&outcome, &mut out)?;
            }
            out.flush()?;
            Ok(())
        }
    }
}

fn load_registry(config_dir: &Path) -> Result<Registry> {
    Registry::load(config_dir).with_context(|| {
        format!(
            "failed to load catalog registry from {}",
            config_dir.join(config::CONFIG_FILENAME).display()
        )
    })
}
