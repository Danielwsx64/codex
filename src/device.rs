use std::path::{Path, PathBuf};

use thiserror::Error;

pub mod markers;
pub mod mounts;
pub mod sysfs;

pub const AMAZON_VENDOR_ID: &str = "1949";

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
}
