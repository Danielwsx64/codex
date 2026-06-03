use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod paths;

pub const CONFIG_FILENAME: &str = "config.toml";
const CURRENT_VERSION: u32 = 1;
const NAME_MAX_LEN: usize = 64;

#[derive(Debug, Error)]
pub enum Error {
    #[error("config file at {path} is malformed: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("failed to serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("io error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("catalog name `{name}` is invalid (use [A-Za-z0-9_-], 1-{NAME_MAX_LEN} chars)")]
    InvalidName { name: String },
    #[error("catalog `{name}` is already registered")]
    DuplicateName { name: String },
    #[error("catalog path `{}` is already registered as `{name}`", .path.display())]
    DuplicatePath { name: String, path: PathBuf },
    #[error("no catalog named `{name}` is registered")]
    UnknownName { name: String },
    #[error("current catalog `{name}` is no longer registered; run `cdx catalog use <name>`")]
    DanglingCurrent { name: String },
    #[error("no current catalog set; run `cdx catalog use <name>`")]
    NoCurrent,
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogEntry {
    pub name: String,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReaderSettings {
    /// Maximum width (in columns) used for the text body. When the terminal
    /// is wider than this, the content is centered and the surrounding space
    /// becomes the horizontal margin. Set to 0 to use the full width.
    #[serde(default = "ReaderSettings::default_max_content_width")]
    pub max_content_width: u16,
    /// Minimum left/right padding (in columns) added around the text body
    /// even when the terminal is narrower than `max_content_width`.
    #[serde(default = "ReaderSettings::default_horizontal_margin")]
    pub horizontal_margin: u16,
    /// Top/bottom padding (in rows) reserved above the text body and below
    /// the footer. Makes line-of-sight more comfortable.
    #[serde(default = "ReaderSettings::default_vertical_margin")]
    pub vertical_margin: u16,
}

impl ReaderSettings {
    const fn default_max_content_width() -> u16 {
        80
    }
    const fn default_horizontal_margin() -> u16 {
        2
    }
    const fn default_vertical_margin() -> u16 {
        1
    }
}

impl Default for ReaderSettings {
    fn default() -> Self {
        Self {
            max_content_width: Self::default_max_content_width(),
            horizontal_margin: Self::default_horizontal_margin(),
            vertical_margin: Self::default_vertical_margin(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Registry {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    #[serde(default, rename = "catalog")]
    pub catalogs: Vec<CatalogEntry>,
    #[serde(default)]
    pub reader: ReaderSettings,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            current: None,
            catalogs: Vec::new(),
            reader: ReaderSettings::default(),
        }
    }
}

impl Registry {
    pub fn load(config_dir: &Path) -> Result<Self> {
        let path = config_dir.join(CONFIG_FILENAME);
        match fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).map_err(|source| Error::Parse { path, source }),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(Error::Io { path, source }),
        }
    }

    pub fn save(&self, config_dir: &Path) -> Result<()> {
        ensure_config_dir(config_dir)?;
        let final_path = config_dir.join(CONFIG_FILENAME);
        let tmp_path = config_dir.join(format!("{CONFIG_FILENAME}.tmp"));

        let serialized = toml::to_string_pretty(self)?;

        let mut file = fs::File::create(&tmp_path).map_err(|source| Error::Io {
            path: tmp_path.clone(),
            source,
        })?;
        file.write_all(serialized.as_bytes())
            .map_err(|source| Error::Io {
                path: tmp_path.clone(),
                source,
            })?;
        file.sync_all().map_err(|source| Error::Io {
            path: tmp_path.clone(),
            source,
        })?;
        drop(file);

        fs::rename(&tmp_path, &final_path).map_err(|source| Error::Io {
            path: final_path,
            source,
        })?;
        Ok(())
    }

    pub fn find(&self, name: &str) -> Option<&CatalogEntry> {
        self.catalogs.iter().find(|c| c.name == name)
    }

    pub fn resolve_current(&self) -> Result<&CatalogEntry> {
        match &self.current {
            None => Err(Error::NoCurrent),
            Some(name) => self
                .find(name)
                .ok_or_else(|| Error::DanglingCurrent { name: name.clone() }),
        }
    }

    pub fn resolve(&self, override_name: Option<&str>) -> Result<&CatalogEntry> {
        match override_name {
            Some(name) => self.find(name).ok_or_else(|| Error::UnknownName {
                name: name.to_string(),
            }),
            None => self.resolve_current(),
        }
    }

    pub fn insert(&mut self, entry: CatalogEntry) -> Result<()> {
        validate_name(&entry.name)?;
        if self.find(&entry.name).is_some() {
            return Err(Error::DuplicateName { name: entry.name });
        }
        if let Some(existing) = self.catalogs.iter().find(|c| c.path == entry.path) {
            return Err(Error::DuplicatePath {
                name: existing.name.clone(),
                path: entry.path,
            });
        }
        self.catalogs.push(entry);
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> Result<CatalogEntry> {
        let idx = self
            .catalogs
            .iter()
            .position(|c| c.name == name)
            .ok_or_else(|| Error::UnknownName {
                name: name.to_string(),
            })?;
        let removed = self.catalogs.remove(idx);
        if self.current.as_deref() == Some(name) {
            self.current = None;
        }
        Ok(removed)
    }

    pub fn set_current(&mut self, name: &str) -> Result<()> {
        if self.find(name).is_none() {
            return Err(Error::UnknownName {
                name: name.to_string(),
            });
        }
        self.current = Some(name.to_string());
        Ok(())
    }
}

pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > NAME_MAX_LEN {
        return Err(Error::InvalidName {
            name: name.to_string(),
        });
    }
    let valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !valid {
        return Err(Error::InvalidName {
            name: name.to_string(),
        });
    }
    Ok(())
}

fn ensure_config_dir(config_dir: &Path) -> Result<()> {
    fs::create_dir_all(config_dir).map_err(|source| Error::Io {
        path: config_dir.to_path_buf(),
        source,
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(config_dir)
            .map_err(|source| Error::Io {
                path: config_dir.to_path_buf(),
                source,
            })?
            .permissions();
        if perms.mode() & 0o777 != 0o700 {
            perms.set_mode(0o700);
            fs::set_permissions(config_dir, perms).map_err(|source| Error::Io {
                path: config_dir.to_path_buf(),
                source,
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn entry(name: &str, path: &str) -> CatalogEntry {
        CatalogEntry {
            name: name.to_string(),
            path: PathBuf::from(path),
            description: None,
        }
    }

    #[test]
    fn validate_name_accepts_valid_names() {
        for name in ["a", "main", "main-lib", "main_lib", "MAIN123", "a-b_c"] {
            validate_name(name).expect("expected name to be valid");
        }
    }

    #[test]
    fn validate_name_rejects_invalid_names() {
        for name in [
            "",
            "with space",
            "with/slash",
            "ção",
            "x".repeat(65).as_str(),
        ] {
            let err = validate_name(name).expect_err("expected name to be rejected");
            assert!(matches!(err, Error::InvalidName { .. }));
        }
    }

    #[test]
    fn insert_rejects_duplicate_name() {
        let mut reg = Registry::default();
        reg.insert(entry("a", "/p1")).unwrap();
        let err = reg.insert(entry("a", "/p2")).unwrap_err();
        assert!(matches!(err, Error::DuplicateName { .. }));
    }

    #[test]
    fn insert_rejects_duplicate_path() {
        let mut reg = Registry::default();
        reg.insert(entry("a", "/p1")).unwrap();
        let err = reg.insert(entry("b", "/p1")).unwrap_err();
        assert!(matches!(err, Error::DuplicatePath { .. }));
    }

    #[test]
    fn resolve_current_returns_no_current_when_unset() {
        let reg = Registry::default();
        assert!(matches!(reg.resolve_current(), Err(Error::NoCurrent)));
    }

    #[test]
    fn resolve_current_returns_dangling_when_missing() {
        let reg = Registry {
            current: Some("ghost".to_string()),
            ..Registry::default()
        };
        assert!(matches!(
            reg.resolve_current(),
            Err(Error::DanglingCurrent { .. })
        ));
    }

    #[test]
    fn resolve_current_succeeds() {
        let mut reg = Registry::default();
        reg.insert(entry("a", "/p1")).unwrap();
        reg.set_current("a").unwrap();
        assert_eq!(reg.resolve_current().unwrap().name, "a");
    }

    #[test]
    fn remove_clears_current_when_removing_current() {
        let mut reg = Registry::default();
        reg.insert(entry("a", "/p1")).unwrap();
        reg.set_current("a").unwrap();
        reg.remove("a").unwrap();
        assert_eq!(reg.current, None);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempdir().unwrap();
        let mut reg = Registry::default();
        let mut e = entry("personal", "/home/me/books");
        e.description = Some("main".to_string());
        reg.insert(e).unwrap();
        reg.set_current("personal").unwrap();

        reg.save(dir.path()).unwrap();
        let loaded = Registry::load(dir.path()).unwrap();
        assert_eq!(reg, loaded);
    }

    #[test]
    fn reader_settings_default_when_section_absent() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join(CONFIG_FILENAME),
            "version = 1\ncurrent = \"x\"\n[[catalog]]\nname=\"x\"\npath=\"/p\"\n",
        )
        .unwrap();
        let reg = Registry::load(dir.path()).unwrap();
        assert_eq!(reg.reader, ReaderSettings::default());
    }

    #[test]
    fn reader_settings_roundtrip() {
        let dir = tempdir().unwrap();
        let reg = Registry {
            reader: ReaderSettings {
                max_content_width: 100,
                horizontal_margin: 4,
                vertical_margin: 2,
            },
            ..Registry::default()
        };
        reg.save(dir.path()).unwrap();
        let loaded = Registry::load(dir.path()).unwrap();
        assert_eq!(loaded.reader, reg.reader);
    }

    #[test]
    fn reader_settings_partial_section_uses_defaults_for_missing_fields() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join(CONFIG_FILENAME),
            "version = 1\n[reader]\nmax_content_width = 60\n",
        )
        .unwrap();
        let reg = Registry::load(dir.path()).unwrap();
        assert_eq!(reg.reader.max_content_width, 60);
        assert_eq!(
            reg.reader.horizontal_margin,
            ReaderSettings::default().horizontal_margin
        );
        assert_eq!(
            reg.reader.vertical_margin,
            ReaderSettings::default().vertical_margin
        );
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = tempdir().unwrap();
        let reg = Registry::load(dir.path()).unwrap();
        assert_eq!(reg, Registry::default());
    }

    #[test]
    fn save_writes_atomically_without_leaving_tmp() {
        let dir = tempdir().unwrap();
        let reg = Registry::default();
        reg.save(dir.path()).unwrap();
        let tmp = dir.path().join(format!("{CONFIG_FILENAME}.tmp"));
        assert!(!tmp.exists(), "tmp file should be renamed away");
        assert!(dir.path().join(CONFIG_FILENAME).exists());
    }
}
