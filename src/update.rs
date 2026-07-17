use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

const RELEASES_LATEST_URL: &str = "https://api.github.com/repos/Danielwsx64/codex/releases/latest";
const USER_AGENT: &str = concat!("cdx/", env!("CARGO_PKG_VERSION"));
// GitHub asset download can be large and USB-slow networks vary; keep generous.
const READ_TIMEOUT: Duration = Duration::from_secs(120);
const CALL_TIMEOUT: Duration = Duration::from_secs(30);
// Hard cap on a downloaded binary so a malformed/huge response can't OOM us.
const MAX_ASSET_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum Error {
    #[error("network error talking to GitHub: {0}")]
    Http(Box<ureq::Error>),
    #[error("could not parse the GitHub release response: {0}")]
    Decode(#[from] std::io::Error),
    #[error("this platform ({os}/{arch}) has no prebuilt cdx release; build from source")]
    UnsupportedPlatform {
        os: &'static str,
        arch: &'static str,
    },
    #[error("release `{tag}` has no asset named `{asset}`")]
    MissingAsset { tag: String, asset: String },
    #[error("checksum mismatch for `{asset}`: expected {expected}, got {actual}")]
    ChecksumMismatch {
        asset: String,
        expected: String,
        actual: String,
    },
    #[error("could not locate the running cdx executable: {0}")]
    CurrentExe(#[source] std::io::Error),
    #[error("downloaded asset is larger than the {MAX_ASSET_BYTES}-byte limit")]
    AssetTooLarge,
    #[error("malformed checksum file for `{asset}`")]
    MalformedChecksum { asset: String },
}

impl From<ureq::Error> for Error {
    fn from(e: ureq::Error) -> Self {
        Error::Http(Box::new(e))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
}

#[derive(Debug, Clone)]
pub struct CheckOutcome {
    pub current: String,
    pub latest: String,
    pub newer_available: bool,
    pub release: Release,
}

pub fn current_version() -> &'static str {
    crate::welcome::version()
}

// The release-asset target triple for the running platform. We ship a single
// static musl binary for Linux (runs on gnu and musl hosts alike), so Linux
// x86_64 always maps to the musl asset regardless of the host libc.
pub fn asset_target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("x86_64-unknown-linux-musl"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        _ => None,
    }
}

pub fn asset_name() -> Result<String> {
    asset_target()
        .map(|t| format!("cdx-{t}"))
        .ok_or(Error::UnsupportedPlatform {
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
        })
}

pub fn build_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_read(READ_TIMEOUT)
        .timeout_connect(CALL_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
}

pub fn fetch_latest(agent: &ureq::Agent) -> Result<Release> {
    let release: Release = agent
        .get(RELEASES_LATEST_URL)
        .set("Accept", "application/vnd.github+json")
        .call()?
        .into_json()?;
    Ok(release)
}

pub fn check(agent: &ureq::Agent) -> Result<CheckOutcome> {
    let release = fetch_latest(agent)?;
    let current = current_version().to_string();
    let latest = release.tag_name.trim_start_matches('v').to_string();
    let newer_available = is_newer(&latest, &current);
    Ok(CheckOutcome {
        current,
        latest,
        newer_available,
        release,
    })
}

// Downloads the platform asset, verifies its SHA-256 sidecar, and atomically
// replaces the running executable. Returns the path that was replaced.
pub fn install(agent: &ureq::Agent, release: &Release) -> Result<PathBuf> {
    let name = asset_name()?;
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == name)
        .ok_or_else(|| Error::MissingAsset {
            tag: release.tag_name.clone(),
            asset: name.clone(),
        })?;

    let bytes = download(agent, &asset.browser_download_url)?;
    let expected = fetch_checksum(agent, &asset.browser_download_url, &name)?;
    let actual = sha256_hex(&bytes);
    if !actual.eq_ignore_ascii_case(&expected) {
        return Err(Error::ChecksumMismatch {
            asset: name,
            expected,
            actual,
        });
    }

    let exe = std::env::current_exe().map_err(Error::CurrentExe)?;
    replace_executable(&exe, &bytes)?;
    Ok(exe)
}

fn download(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>> {
    let resp = agent
        .get(url)
        .set("Accept", "application/octet-stream")
        .call()?;
    let mut buf = Vec::new();
    resp.into_reader()
        .take(MAX_ASSET_BYTES + 1)
        .read_to_end(&mut buf)?;
    if buf.len() as u64 > MAX_ASSET_BYTES {
        return Err(Error::AssetTooLarge);
    }
    Ok(buf)
}

fn fetch_checksum(agent: &ureq::Agent, asset_url: &str, asset_name: &str) -> Result<String> {
    let url = format!("{asset_url}.sha256");
    let body = agent.get(&url).call()?.into_string()?;
    parse_checksum(&body).ok_or_else(|| Error::MalformedChecksum {
        asset: asset_name.to_string(),
    })
}

// `sha256sum` emits "<hex>  <filename>"; accept either that or a bare hex digest.
fn parse_checksum(body: &str) -> Option<String> {
    let token = body.split_whitespace().next()?;
    let is_hex = token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit());
    is_hex.then(|| token.to_string())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        // writing to a String can't fail; discard the Result to satisfy clippy.
        let _ = write!(out, "{b:02x}");
    }
    out
}

// Write the new binary next to the current exe (same filesystem → atomic rename)
// and swap it in. On Unix, replacing a running executable via rename is allowed.
fn replace_executable(exe: &Path, bytes: &[u8]) -> Result<()> {
    let dir = exe.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.flush()?;
    make_executable(tmp.path())?;
    tmp.persist(exe).map_err(|e| Error::Decode(e.error))?;
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

// Parse a dotted "major.minor.patch" (leading `v` already stripped), ignoring
// any pre-release/build suffix. Missing components default to 0.
fn parse_version(s: &str) -> (u64, u64, u64) {
    let core = s
        .trim_start_matches('v')
        .split(['-', '+'])
        .next()
        .unwrap_or("");
    let mut parts = core.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    let major = parts.next().unwrap_or(0);
    let minor = parts.next().unwrap_or(0);
    let patch = parts.next().unwrap_or(0);
    (major, minor, patch)
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    parse_version(latest) > parse_version(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_ordering() {
        assert!(is_newer("1.0.1", "1.0.0"));
        assert!(is_newer("1.1.0", "1.0.9"));
        assert!(is_newer("2.0.0", "1.9.9"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("1.0.0", "1.0.1"));
    }

    #[test]
    fn version_ignores_v_prefix_and_suffix() {
        assert_eq!(parse_version("v1.2.3"), (1, 2, 3));
        assert_eq!(parse_version("1.2.3-rc1"), (1, 2, 3));
        assert_eq!(parse_version("1.2"), (1, 2, 0));
        assert!(is_newer("v1.2.0", "1.1.9"));
    }

    #[test]
    fn asset_name_matches_target() {
        // On the supported dev/CI hosts this resolves; on others it errors.
        match asset_target() {
            Some(t) => assert_eq!(asset_name().unwrap(), format!("cdx-{t}")),
            None => assert!(asset_name().is_err()),
        }
    }

    #[test]
    fn checksum_parsing() {
        let hex = "a".repeat(64);
        assert_eq!(
            parse_checksum(&format!("{hex}  cdx-linux")),
            Some(hex.clone())
        );
        assert_eq!(parse_checksum(&format!("{hex}\n")), Some(hex.clone()));
        assert_eq!(parse_checksum("not-a-hash"), None);
        assert_eq!(parse_checksum(""), None);
    }

    #[test]
    fn sha256_of_known_input() {
        // echo -n "abc" | sha256sum
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
