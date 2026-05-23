use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("could not determine the cdx config directory; set --data-dir or $HOME")]
    Unresolvable,
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn resolve_config_dir(data_dir_override: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = data_dir_override {
        return Ok(path.to_path_buf());
    }
    ProjectDirs::from("", "", "cdx")
        .map(|p| p.config_dir().to_path_buf())
        .ok_or(Error::Unresolvable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_wins_over_project_dirs() {
        let override_path = PathBuf::from("/tmp/cdx-test");
        let resolved = resolve_config_dir(Some(&override_path)).unwrap();
        assert_eq!(resolved, override_path);
    }

    #[test]
    fn no_override_resolves_via_project_dirs() {
        let resolved = resolve_config_dir(None);
        assert!(resolved.is_ok(), "ProjectDirs should resolve on this host");
        let p = resolved.unwrap();
        assert!(
            p.ends_with("cdx") || p.to_string_lossy().contains("cdx"),
            "resolved path {:?} should mention cdx",
            p
        );
    }
}
