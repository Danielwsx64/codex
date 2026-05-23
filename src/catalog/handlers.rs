use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::catalog;
use crate::config::{CatalogEntry, Registry};

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Config(#[from] crate::config::Error),
    #[error(transparent)]
    Catalog(#[from] crate::catalog::Error),
    #[error("io error on {}: {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct InitOutcome {
    pub name: String,
    pub path: PathBuf,
    pub became_current: bool,
}

#[derive(Debug)]
pub struct AddOutcome {
    pub name: String,
    pub path: PathBuf,
    pub became_current: bool,
}

#[derive(Debug)]
pub struct UseOutcome {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug)]
pub struct RmOutcome {
    pub name: String,
    pub path: PathBuf,
    pub purged: bool,
    pub cleared_current: bool,
}

#[derive(Debug, Clone)]
pub struct CatalogRow {
    pub name: String,
    pub path: PathBuf,
    pub description: Option<String>,
    pub current: bool,
    pub missing: bool,
}

pub fn handle_init(
    registry: &mut Registry,
    config_dir: &Path,
    name: &str,
    path: &Path,
    description: Option<String>,
    no_switch: bool,
) -> Result<InitOutcome> {
    crate::config::validate_name(name)?;

    let conn = catalog::init(path)?;
    drop(conn);
    let canonical = path.canonicalize().map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;

    let entry = CatalogEntry {
        name: name.to_string(),
        path: canonical.clone(),
        description,
    };
    registry.insert(entry)?;

    let became_current = if no_switch {
        if registry.current.is_none() {
            registry.current = Some(name.to_string());
            true
        } else {
            false
        }
    } else {
        registry.current = Some(name.to_string());
        true
    };

    registry.save(config_dir)?;
    Ok(InitOutcome {
        name: name.to_string(),
        path: canonical,
        became_current,
    })
}

pub fn handle_add(
    registry: &mut Registry,
    config_dir: &Path,
    name: &str,
    path: &Path,
    description: Option<String>,
    no_switch: bool,
) -> Result<AddOutcome> {
    crate::config::validate_name(name)?;

    let conn = catalog::open_existing(path)?;
    drop(conn);
    let canonical = path.canonicalize().map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;

    let entry = CatalogEntry {
        name: name.to_string(),
        path: canonical.clone(),
        description,
    };
    registry.insert(entry)?;

    let became_current = if no_switch {
        if registry.current.is_none() {
            registry.current = Some(name.to_string());
            true
        } else {
            false
        }
    } else {
        registry.current = Some(name.to_string());
        true
    };

    registry.save(config_dir)?;
    Ok(AddOutcome {
        name: name.to_string(),
        path: canonical,
        became_current,
    })
}

pub fn handle_ls(registry: &Registry) -> Vec<CatalogRow> {
    let current = registry.current.as_deref();
    registry
        .catalogs
        .iter()
        .map(|c| CatalogRow {
            name: c.name.clone(),
            path: c.path.clone(),
            description: c.description.clone(),
            current: current == Some(c.name.as_str()),
            missing: !catalog::is_initialized(&c.path),
        })
        .collect()
}

pub fn handle_use(registry: &mut Registry, config_dir: &Path, name: &str) -> Result<UseOutcome> {
    registry.set_current(name)?;
    let path = registry
        .find(name)
        .expect("set_current verified the name exists")
        .path
        .clone();
    registry.save(config_dir)?;
    Ok(UseOutcome {
        name: name.to_string(),
        path,
    })
}

pub fn handle_rm(
    registry: &mut Registry,
    config_dir: &Path,
    name: &str,
    purge: bool,
) -> Result<RmOutcome> {
    let was_current = registry.current.as_deref() == Some(name);
    let removed = registry.remove(name)?;
    if purge && removed.path.exists() {
        fs::remove_dir_all(&removed.path).map_err(|source| Error::Io {
            path: removed.path.clone(),
            source,
        })?;
    }
    registry.save(config_dir)?;
    Ok(RmOutcome {
        name: removed.name,
        path: removed.path,
        purged: purge,
        cleared_current: was_current,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup() -> (tempfile::TempDir, PathBuf) {
        let dir = tempdir().unwrap();
        let cfg = dir.path().join("cfg");
        fs::create_dir_all(&cfg).unwrap();
        (dir, cfg)
    }

    #[test]
    fn init_happy_path_becomes_current() {
        let (tmp, cfg) = setup();
        let cat = tmp.path().join("lib");
        let mut reg = Registry::default();
        let out = handle_init(&mut reg, &cfg, "main", &cat, None, false).unwrap();
        assert_eq!(out.name, "main");
        assert!(out.became_current);
        assert_eq!(reg.current.as_deref(), Some("main"));
        assert_eq!(reg.catalogs.len(), 1);
    }

    #[test]
    fn init_no_switch_keeps_existing_current() {
        let (tmp, cfg) = setup();
        let cat1 = tmp.path().join("lib1");
        let cat2 = tmp.path().join("lib2");
        let mut reg = Registry::default();
        handle_init(&mut reg, &cfg, "first", &cat1, None, false).unwrap();
        let out = handle_init(&mut reg, &cfg, "second", &cat2, None, true).unwrap();
        assert!(!out.became_current);
        assert_eq!(reg.current.as_deref(), Some("first"));
    }

    #[test]
    fn init_no_switch_sets_current_when_none_yet() {
        let (tmp, cfg) = setup();
        let cat = tmp.path().join("lib");
        let mut reg = Registry::default();
        let out = handle_init(&mut reg, &cfg, "main", &cat, None, true).unwrap();
        assert!(
            out.became_current,
            "first catalog should become current even with --no-switch"
        );
        assert_eq!(reg.current.as_deref(), Some("main"));
    }

    #[test]
    fn init_refuses_invalid_name() {
        let (tmp, cfg) = setup();
        let cat = tmp.path().join("lib");
        let mut reg = Registry::default();
        let err = handle_init(&mut reg, &cfg, "with space", &cat, None, false).unwrap_err();
        assert!(matches!(
            err,
            Error::Config(crate::config::Error::InvalidName { .. })
        ));
        assert!(
            !cat.exists() || !catalog::is_initialized(&cat),
            "catalog dir should not be initialized when name fails validation"
        );
    }

    #[test]
    fn add_requires_existing_catalog() {
        let (tmp, cfg) = setup();
        let empty = tmp.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        let mut reg = Registry::default();
        let err = handle_add(&mut reg, &cfg, "main", &empty, None, false).unwrap_err();
        assert!(matches!(
            err,
            Error::Catalog(catalog::Error::NotACatalog { .. })
        ));
    }

    #[test]
    fn add_registers_existing_catalog() {
        let (tmp, cfg) = setup();
        let cat = tmp.path().join("lib");
        catalog::init(&cat).unwrap();

        let mut reg = Registry::default();
        let out = handle_add(&mut reg, &cfg, "main", &cat, None, false).unwrap();
        assert_eq!(out.name, "main");
        assert!(reg.find("main").is_some());
    }

    #[test]
    fn use_unknown_name_errors() {
        let (_tmp, cfg) = setup();
        let mut reg = Registry::default();
        let err = handle_use(&mut reg, &cfg, "ghost").unwrap_err();
        assert!(matches!(
            err,
            Error::Config(crate::config::Error::UnknownName { .. })
        ));
    }

    #[test]
    fn rm_clears_current_when_removing_current() {
        let (tmp, cfg) = setup();
        let cat = tmp.path().join("lib");
        let mut reg = Registry::default();
        handle_init(&mut reg, &cfg, "main", &cat, None, false).unwrap();
        let out = handle_rm(&mut reg, &cfg, "main", false).unwrap();
        assert!(out.cleared_current);
        assert_eq!(reg.current, None);
    }

    #[test]
    fn rm_purge_deletes_files() {
        let (tmp, cfg) = setup();
        let cat = tmp.path().join("lib");
        let mut reg = Registry::default();
        handle_init(&mut reg, &cfg, "main", &cat, None, false).unwrap();
        assert!(cat.is_dir());
        let out = handle_rm(&mut reg, &cfg, "main", true).unwrap();
        assert!(out.purged);
        assert!(!cat.exists(), "purge should remove the catalog directory");
    }

    #[test]
    fn ls_marks_current_and_missing() {
        let (tmp, cfg) = setup();
        let cat1 = tmp.path().join("lib1");
        let cat2 = tmp.path().join("lib2");
        let mut reg = Registry::default();
        handle_init(&mut reg, &cfg, "alive", &cat1, None, false).unwrap();
        handle_init(&mut reg, &cfg, "gone", &cat2, None, true).unwrap();
        fs::remove_dir_all(&cat2).unwrap();

        let rows = handle_ls(&reg);
        assert_eq!(rows.len(), 2);
        let alive = rows.iter().find(|r| r.name == "alive").unwrap();
        assert!(alive.current);
        assert!(!alive.missing);
        let gone = rows.iter().find(|r| r.name == "gone").unwrap();
        assert!(!gone.current);
        assert!(gone.missing);
    }
}
