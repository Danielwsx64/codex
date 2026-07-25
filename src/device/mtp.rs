use std::path::{Path, PathBuf};

use super::sysfs::UsbDevice;

// gvfs exposes its backends through a single FUSE mount (one per user session);
// each mounted MTP device is a directory inside it.
pub const GVFS_FSTYPE: &str = "fuse.gvfsd-fuse";

const HOST_PREFIX: &str = "mtp:host=";

// A Kindle reachable over MTP. `root` is the *storage* directory (the one that
// holds `documents/`), not the gvfs host directory, so the rest of the device
// code keeps treating it as an ordinary mount point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtpMount {
    pub serial: String,
    pub root: PathBuf,
}

// Newer Kindles (Colorsoft, Scribe, Paperwhite 12) speak MTP only — they never
// appear as a block device, so the usual sysfs-walk-up-from-/dev/sdX path finds
// nothing. Instead we look at what gvfs has already mounted and tie each host
// directory back to an Amazon USB device by serial. cdx only *discovers* these
// mounts; mounting is left to the desktop session or an explicit `gio mount`.
pub fn collect(gvfs_bases: &[PathBuf], devices: &[UsbDevice]) -> Vec<MtpMount> {
    let mut found: Vec<MtpMount> = Vec::new();
    if devices.is_empty() {
        return found;
    }
    for base in gvfs_bases {
        let entries = match std::fs::read_dir(base) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::debug!(path = %base.display(), error = %e, "cannot list gvfs mounts");
                continue;
            }
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name();
            let Some(host) = name.to_str().and_then(|n| n.strip_prefix(HOST_PREFIX)) else {
                continue;
            };
            let Some(device) = devices.iter().find(|d| matches_host(host, d)) else {
                continue;
            };
            if found.iter().any(|m| m.serial == device.serial) {
                continue;
            }
            found.push(MtpMount {
                serial: device.serial.clone(),
                root: storage_root(&entry.path()),
            });
        }
    }
    found
}

fn matches_host(host: &str, device: &UsbDevice) -> bool {
    // gvfs >= 1.44 builds a friendly host from the USB descriptors, e.g.
    // `Amazon_Kindle_Colorsoft_GN43H2075425044K`. Anchoring on the `_` boundary
    // keeps a short serial from matching the tail of a longer one.
    let by_serial = host.strip_suffix(&device.serial).is_some_and(|prefix| {
        !device.serial.is_empty() && (prefix.is_empty() || prefix.ends_with('_'))
    });
    // Older gvfs used the percent-encoded bus address instead: `[usb:003,013]`.
    by_serial || percent_decode(host) == format!("[usb:{:03},{:03}]", device.busnum, device.devnum)
}

// An MTP device publishes one directory per storage; `documents/` lives inside
// the storage, not at the host root. Everything downstream (`push`, `books`,
// `clean`, `sync`) expects a path with a `documents/` child, so resolve down to
// the storage here and the rest of the code needs no MTP awareness at all.
fn storage_root(host_dir: &Path) -> PathBuf {
    if host_dir.join("documents").is_dir() {
        return host_dir.to_path_buf();
    }
    let mut subdirs: Vec<PathBuf> = match std::fs::read_dir(host_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect(),
        Err(e) => {
            tracing::debug!(path = %host_dir.display(), error = %e, "cannot list MTP storages");
            return host_dir.to_path_buf();
        }
    };
    // read_dir order is unspecified; sort so a device with several storages
    // always resolves to the same one.
    subdirs.sort();
    if let Some(storage) = subdirs.iter().find(|p| p.join("documents").is_dir()) {
        return storage.clone();
    }
    // A sleeping device can enumerate its storage before exposing the contents.
    // `push` creates `documents/` on demand, so the lone storage is still the
    // right answer.
    match subdirs.as_slice() {
        [only] => only.clone(),
        _ => host_dir.to_path_buf(),
    }
}

// gvfs-fuse cannot *create* a file on an MTP mount: the protocol wants the
// object size before the transfer starts and the FUSE `create` callback has no
// way to supply one, so every `open(O_CREAT)` fails with EOPNOTSUPP. It has
// been that way since at least 2015 (Debian #803421) and is not something a
// caller can work around with plain filesystem calls. GIO maps a gvfs-fuse path
// back to its backend and issues a properly sized transfer — the same thing
// file managers do. Read, stat, mkdir, rename and unlink all work through FUSE,
// so this is the only operation that has to leave the filesystem API.
pub fn copy_out_of_band(src: &Path, dest: &Path) -> std::io::Result<()> {
    let output = std::process::Command::new("gio")
        .arg("copy")
        .arg("--no-target-directory")
        .arg(src)
        .arg(dest)
        .output()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "writing to an MTP device needs the `gio` command (package `libglib2.0-bin` on Debian/Ubuntu, `glib2` elsewhere)",
            ),
            _ => e,
        })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(std::io::Error::other(format!(
        "gio copy failed: {}",
        stderr.trim()
    )))
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let decoded = (bytes[i] == b'%' && i + 2 < bytes.len())
            .then(|| std::str::from_utf8(&bytes[i + 1..i + 3]).ok())
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match decoded {
            Some(byte) => {
                out.push(byte as char);
                i += 3;
            }
            None => {
                out.push(bytes[i] as char);
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::{tempdir, TempDir};

    const SERIAL: &str = "GN43H2075425044K";

    fn colorsoft() -> UsbDevice {
        UsbDevice {
            serial: SERIAL.to_string(),
            busnum: 3,
            devnum: 13,
        }
    }

    fn gvfs_base(host: &str, inner: &[&str]) -> TempDir {
        let dir = tempdir().unwrap();
        let host_dir = dir.path().join(format!("{HOST_PREFIX}{host}"));
        for path in inner {
            fs::create_dir_all(host_dir.join(path)).unwrap();
        }
        fs::create_dir_all(&host_dir).unwrap();
        dir
    }

    #[test]
    fn finds_colorsoft_under_its_storage_directory() {
        let base = gvfs_base(
            &format!("Amazon_Kindle_Colorsoft_{SERIAL}"),
            &["Internal Storage/documents", "Internal Storage/fonts"],
        );
        let found = collect(&[base.path().to_path_buf()], &[colorsoft()]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].serial, SERIAL);
        assert!(found[0].root.ends_with("Internal Storage"));
    }

    #[test]
    fn keeps_host_root_when_documents_sits_at_the_top() {
        let base = gvfs_base(&format!("Amazon_Kindle_{SERIAL}"), &["documents"]);
        let found = collect(&[base.path().to_path_buf()], &[colorsoft()]);
        assert_eq!(found.len(), 1);
        assert!(found[0]
            .root
            .ends_with(format!("{HOST_PREFIX}Amazon_Kindle_{SERIAL}")));
    }

    #[test]
    fn falls_back_to_the_lone_storage_without_documents() {
        let base = gvfs_base(&format!("Amazon_Kindle_{SERIAL}"), &["Internal Storage"]);
        let found = collect(&[base.path().to_path_buf()], &[colorsoft()]);
        assert!(found[0].root.ends_with("Internal Storage"));
    }

    #[test]
    fn picks_the_storage_holding_documents_when_several_exist() {
        let base = gvfs_base(
            &format!("Amazon_Kindle_{SERIAL}"),
            &["SD Card", "Internal Storage/documents"],
        );
        let found = collect(&[base.path().to_path_buf()], &[colorsoft()]);
        assert!(found[0].root.ends_with("Internal Storage"));
    }

    #[test]
    fn matches_the_legacy_percent_encoded_bus_host() {
        let base = gvfs_base("%5Busb%3A003%2C013%5D", &["Internal Storage/documents"]);
        let found = collect(&[base.path().to_path_buf()], &[colorsoft()]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].serial, SERIAL);
    }

    #[test]
    fn ignores_hosts_that_match_no_amazon_device() {
        let base = gvfs_base(
            "Google_Pixel_9_ABC123",
            &["Internal shared storage/documents"],
        );
        assert!(collect(&[base.path().to_path_buf()], &[colorsoft()]).is_empty());
    }

    #[test]
    fn ignores_non_mtp_gvfs_backends() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(
            dir.path()
                .join(format!("smb-share:server=nas,share={SERIAL}")),
        )
        .unwrap();
        assert!(collect(&[dir.path().to_path_buf()], &[colorsoft()]).is_empty());
    }

    #[test]
    fn serial_must_end_on_an_underscore_boundary() {
        let base = gvfs_base(&format!("Amazon_KindleX{SERIAL}"), &["documents"]);
        assert!(collect(&[base.path().to_path_buf()], &[colorsoft()]).is_empty());
    }

    #[test]
    fn no_amazon_devices_means_no_scan() {
        let base = gvfs_base(&format!("Amazon_Kindle_{SERIAL}"), &["documents"]);
        assert!(collect(&[base.path().to_path_buf()], &[]).is_empty());
    }

    #[test]
    fn missing_gvfs_base_is_not_fatal() {
        let dir = tempdir().unwrap();
        let gone = dir.path().join("nope");
        assert!(collect(&[gone], &[colorsoft()]).is_empty());
    }

    #[test]
    fn percent_decode_handles_plain_and_encoded_input() {
        assert_eq!(percent_decode("%5Busb%3A003%2C013%5D"), "[usb:003,013]");
        assert_eq!(percent_decode("Amazon_Kindle"), "Amazon_Kindle");
        assert_eq!(percent_decode("trailing%"), "trailing%");
        assert_eq!(percent_decode("bad%zz"), "bad%zz");
    }
}
