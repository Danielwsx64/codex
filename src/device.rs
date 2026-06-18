use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::catalog::devices::KnownDevice;
use crate::import::Format;

pub mod markers;
pub mod mounts;
pub mod sysfs;

pub const AMAZON_VENDOR_ID: &str = "1949";

// Test seam: when set, `detect()` skips the host scan and reports no devices.
// Integration tests drive the *known* (DB) path and must not depend on whatever
// USB hardware happens to be plugged into the machine running the suite.
pub const DISABLE_SCAN_ENV: &str = "CDX_NO_DEVICE_SCAN";

const CANDIDATE_FSTYPES: &[&str] = &["vfat", "exfat", "fuseblk", "ntfs"];

#[derive(Debug, Error)]
pub enum Error {
    #[error("io error on `{}`: {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("`{name}` is not a recognizable block device")]
    UnknownBlockDevice { name: String },
    #[error("no USB ancestor found for `{name}` in sysfs")]
    NoUsbAncestor { name: String },
    #[error("USB device for `{name}` has no readable serial")]
    NoSerial { name: String },
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedDevice {
    pub serial: String,
    pub mount_path: PathBuf,
}

#[cfg(target_os = "linux")]
pub fn detect() -> Vec<DetectedDevice> {
    if std::env::var_os(DISABLE_SCAN_ENV).is_some() {
        return Vec::new();
    }
    let contents = match std::fs::read_to_string("/proc/mounts") {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "cannot read /proc/mounts; device scan skipped");
            return Vec::new();
        }
    };
    collect(Path::new("/sys"), &contents)
}

#[cfg(not(target_os = "linux"))]
pub fn detect() -> Vec<DetectedDevice> {
    Vec::new()
}

fn collect(sys_root: &Path, mounts: &str) -> Vec<DetectedDevice> {
    let mut found: Vec<DetectedDevice> = Vec::new();
    for entry in mounts::parse(mounts) {
        if !is_candidate(&entry) {
            continue;
        }
        let identity = match sysfs::resolve_usb_identity(sys_root, &entry.device) {
            Ok(identity) => identity,
            Err(e) => {
                tracing::warn!(
                    device = %entry.device.display(),
                    error = %e,
                    "skipping mount during device scan"
                );
                continue;
            }
        };
        if identity.id_vendor != AMAZON_VENDOR_ID {
            continue;
        }
        if !markers::looks_like_kindle(&entry.mount_point) {
            // Vendor id is authoritative; the marker dirs are only a sanity
            // note (firmware variations may lay the filesystem out differently).
            tracing::debug!(
                mount = %entry.mount_point.display(),
                "Amazon device without documents/ + system/ markers"
            );
        }
        if found.iter().any(|d| d.serial == identity.serial) {
            continue;
        }
        found.push(DetectedDevice {
            serial: identity.serial,
            mount_path: entry.mount_point,
        });
    }
    found
}

fn is_candidate(entry: &mounts::MountEntry) -> bool {
    entry.device.starts_with("/dev")
        && !entry
            .device
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("loop"))
        && CANDIDATE_FSTYPES.contains(&entry.fstype.as_str())
}

// A device as shown by `cdx device ls`: the union of a known DB row (alias,
// last_seen) with live detection state (connected, mount, free space, book
// count). Disconnected devices keep the DB fields and leave the rest empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceRow {
    pub alias: Option<String>,
    pub serial: String,
    pub connected: bool,
    pub mount_path: Option<PathBuf>,
    pub free_bytes: Option<u64>,
    pub book_count: Option<usize>,
    pub last_seen_at: String,
}

// Merge known (DB, authoritative + already sorted) with detected (live). Every
// detected device is `record_seen`'d before this runs, so `known` is the full
// set; detection only marks which rows are currently connected. Pure: the
// filesystem-touching fields stay `None` here and are filled by `enrich`.
pub fn build_device_rows(detected: &[DetectedDevice], known: &[KnownDevice]) -> Vec<DeviceRow> {
    known
        .iter()
        .map(|k| {
            let mount = detected
                .iter()
                .find(|d| d.serial == k.serial)
                .map(|d| d.mount_path.clone());
            DeviceRow {
                alias: k.alias.clone(),
                serial: k.serial.clone(),
                connected: mount.is_some(),
                mount_path: mount,
                free_bytes: None,
                book_count: None,
                last_seen_at: k.last_seen_at.clone(),
            }
        })
        .collect()
}

// Fill in free space and book count for connected devices. A flaky mount must
// never break the listing, so both probes return `None` (logged) on error.
pub fn enrich(rows: &mut [DeviceRow]) {
    for row in rows.iter_mut() {
        if let Some(mount) = row.mount_path.clone() {
            row.free_bytes = mount_free_bytes(&mount);
            row.book_count = count_documents(&mount);
        }
    }
}

fn mount_free_bytes(mount: &Path) -> Option<u64> {
    match fs4::available_space(mount) {
        Ok(bytes) => Some(bytes),
        Err(e) => {
            tracing::warn!(mount = %mount.display(), error = %e, "cannot read free space");
            None
        }
    }
}

fn count_documents(mount: &Path) -> Option<usize> {
    let documents = mount.join("documents");
    if !documents.is_dir() {
        return None;
    }
    let mut count = 0usize;
    let mut stack = vec![documents];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!(dir = %dir.display(), error = %e, "skipping directory in book count");
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if is_ebook(&path) {
                count += 1;
            }
        }
    }
    Some(count)
}

fn is_ebook(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .and_then(Format::parse_label)
        .is_some()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;

    fn fake_usb_tree(root: &Path, disk: &str, partition: &str, vendor: &str, serial: &str) {
        let usb_dev = root.join("devices").join(format!("usb-{disk}"));
        let part_dir = usb_dev.join("iface/host/block").join(disk).join(partition);
        fs::create_dir_all(&part_dir).unwrap();
        fs::write(usb_dev.join("idVendor"), format!("{vendor}\n")).unwrap();
        fs::write(usb_dev.join("serial"), format!("{serial}\n")).unwrap();
        let class = root.join("class/block");
        fs::create_dir_all(&class).unwrap();
        symlink(
            format!("../../devices/usb-{disk}/iface/host/block/{disk}/{partition}"),
            class.join(partition),
        )
        .unwrap();
    }

    #[test]
    fn collect_keeps_only_amazon_devices() {
        let dir = tempdir().unwrap();
        fake_usb_tree(dir.path(), "sdb", "sdb1", "1949", "KINDLE_SERIAL");
        fake_usb_tree(dir.path(), "sdc", "sdc1", "0781", "SANDISK_SERIAL");
        let mounts = "\
/dev/nvme0n1p2 / ext4 rw 0 0
/dev/sdb1 /media/user/Kindle vfat rw 0 0
/dev/sdc1 /media/user/STICK vfat rw 0 0
garbage-line
";
        let devices = collect(dir.path(), mounts);
        assert_eq!(
            devices,
            vec![DetectedDevice {
                serial: "KINDLE_SERIAL".to_string(),
                mount_path: PathBuf::from("/media/user/Kindle"),
            }]
        );
    }

    #[test]
    fn collect_dedups_partitions_sharing_a_serial() {
        let dir = tempdir().unwrap();
        fake_usb_tree(dir.path(), "sdb", "sdb1", "1949", "KINDLE_SERIAL");
        fake_usb_tree(dir.path(), "sdd", "sdd1", "1949", "KINDLE_SERIAL");
        let mounts = "\
/dev/sdb1 /media/user/Kindle vfat rw 0 0
/dev/sdd1 /media/user/Kindle2 vfat rw 0 0
";
        let devices = collect(dir.path(), mounts);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].mount_path, PathBuf::from("/media/user/Kindle"));
    }

    #[test]
    fn collect_detects_multiple_distinct_kindles() {
        let dir = tempdir().unwrap();
        fake_usb_tree(dir.path(), "sdb", "sdb1", "1949", "SERIAL_A");
        fake_usb_tree(dir.path(), "sdc", "sdc1", "1949", "SERIAL_B");
        let mounts = "\
/dev/sdb1 /media/user/KindleA vfat rw 0 0
/dev/sdc1 /media/user/KindleB exfat rw 0 0
";
        let devices = collect(dir.path(), mounts);
        assert_eq!(devices.len(), 2);
    }

    #[test]
    fn collect_skips_unresolvable_mounts() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("class/block")).unwrap();
        let devices = collect(dir.path(), "/dev/sdz1 /media/user/X vfat rw 0 0\n");
        assert!(devices.is_empty());
    }

    #[test]
    fn loop_devices_are_not_candidates() {
        let entry = mounts::MountEntry {
            device: PathBuf::from("/dev/loop3"),
            mount_point: PathBuf::from("/snap/foo"),
            fstype: "vfat".to_string(),
        };
        assert!(!is_candidate(&entry));
    }

    fn known(serial: &str, alias: Option<&str>) -> KnownDevice {
        KnownDevice {
            serial: serial.to_string(),
            alias: alias.map(str::to_string),
            last_seen_at: "2026-06-08 12:00:00".to_string(),
        }
    }

    fn detected(serial: &str, mount: &str) -> DetectedDevice {
        DetectedDevice {
            serial: serial.to_string(),
            mount_path: PathBuf::from(mount),
        }
    }

    #[test]
    fn build_rows_marks_connected_devices() {
        let known_devices = vec![known("A111", Some("paperwhite")), known("B222", None)];
        let detected_devices = vec![detected("A111", "/media/user/Kindle")];
        let rows = build_device_rows(&detected_devices, &known_devices);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].serial, "A111");
        assert!(rows[0].connected);
        assert_eq!(
            rows[0].mount_path,
            Some(PathBuf::from("/media/user/Kindle"))
        );
        assert_eq!(rows[0].alias.as_deref(), Some("paperwhite"));

        assert_eq!(rows[1].serial, "B222");
        assert!(!rows[1].connected);
        assert_eq!(rows[1].mount_path, None);
    }

    #[test]
    fn build_rows_preserves_known_order_and_count() {
        // `known` is authoritative even when nothing is detected.
        let known_devices = vec![known("A111", None), known("Z999", None)];
        let rows = build_device_rows(&[], &known_devices);
        assert_eq!(
            rows.iter().map(|r| r.serial.clone()).collect::<Vec<_>>(),
            vec!["A111", "Z999"]
        );
        assert!(rows.iter().all(|r| !r.connected));
    }

    #[test]
    fn count_documents_counts_only_ebooks_recursively() {
        let dir = tempdir().unwrap();
        let docs = dir.path().join("documents");
        fs::create_dir_all(docs.join("nested")).unwrap();
        fs::write(docs.join("a.epub"), b"x").unwrap();
        fs::write(docs.join("b.azw3"), b"x").unwrap();
        fs::write(docs.join("notes.sdr"), b"x").unwrap();
        fs::write(docs.join("cover.jpg"), b"x").unwrap();
        fs::write(docs.join("nested/c.txt"), b"x").unwrap();

        assert_eq!(count_documents(dir.path()), Some(3));
    }

    #[test]
    fn count_documents_is_none_without_documents_dir() {
        let dir = tempdir().unwrap();
        assert_eq!(count_documents(dir.path()), None);
    }

    #[test]
    fn mount_free_bytes_reports_some_for_a_real_dir() {
        let dir = tempdir().unwrap();
        assert!(mount_free_bytes(dir.path()).is_some());
    }
}
