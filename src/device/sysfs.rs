use std::fs;
use std::path::{Component, Path, PathBuf};

use super::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbIdentity {
    pub id_vendor: String,
    pub serial: String,
}

pub fn resolve_usb_identity(sys_root: &Path, dev_path: &Path) -> Result<UsbIdentity> {
    let name = dev_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| Error::UnknownBlockDevice {
            name: dev_path.display().to_string(),
        })?;
    let class_entry = sys_root.join("class/block").join(name);
    let block_dir = follow_class_link(&class_entry, name)?;

    let mut dir = block_dir.as_path();
    while let Some(parent) = dir.parent() {
        if parent == sys_root || !parent.starts_with(sys_root) {
            break;
        }
        if let Some(id_vendor) = read_sysfs_attr(parent, "idVendor")? {
            let serial = read_sysfs_attr(parent, "serial")?
                .filter(|s| !s.is_empty())
                .ok_or_else(|| Error::NoSerial {
                    name: name.to_string(),
                })?;
            return Ok(UsbIdentity { id_vendor, serial });
        }
        dir = parent;
    }
    Err(Error::NoUsbAncestor {
        name: name.to_string(),
    })
}

// `/sys/class/block/<name>` is a symlink with a relative target like
// `../../devices/.../block/sdb/sdb1`. Resolving it logically (instead of
// `fs::canonicalize`) keeps the result inside an injected fake `sys_root`
// during tests, where the real `/sys` layout is rebuilt under a tempdir.
fn follow_class_link(class_entry: &Path, name: &str) -> Result<PathBuf> {
    match fs::read_link(class_entry) {
        Ok(target) => {
            let base = class_entry
                .parent()
                .ok_or_else(|| Error::UnknownBlockDevice {
                    name: name.to_string(),
                })?;
            Ok(normalize_relative(base, &target))
        }
        // Fake trees (and some sysfs variants) expose a plain directory.
        Err(_) if class_entry.is_dir() => Ok(class_entry.to_path_buf()),
        Err(source) => Err(Error::Io {
            path: class_entry.to_path_buf(),
            source,
        }),
    }
}

fn normalize_relative(base: &Path, target: &Path) -> PathBuf {
    let mut out = base.to_path_buf();
    for component in target.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

fn read_sysfs_attr(dir: &Path, attr: &str) -> Result<Option<String>> {
    let path = dir.join(attr);
    match fs::read_to_string(&path) {
        Ok(raw) => Ok(Some(raw.trim_end_matches('\n').to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Io { path, source }),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;

    // Rebuilds the sysfs shape that matters: a class/block symlink into a
    // devices/ tree where the USB device dir holds idVendor/serial and the
    // interface dir between it and the disk holds neither.
    fn fake_usb_tree(root: &Path, disk: &str, partition: Option<&str>, vendor: &str, serial: &str) {
        let usb_dev = root.join("devices/pci0000:00/usb1/1-2");
        let block = usb_dev.join("1-2:1.0/host4/target4:0:0/4:0:0:0/block");
        let disk_dir = block.join(disk);
        fs::create_dir_all(&disk_dir).unwrap();
        fs::write(usb_dev.join("idVendor"), format!("{vendor}\n")).unwrap();
        fs::write(usb_dev.join("idProduct"), "0004\n").unwrap();
        if !serial.is_empty() {
            fs::write(usb_dev.join("serial"), format!("{serial}\n")).unwrap();
        }

        let class = root.join("class/block");
        fs::create_dir_all(&class).unwrap();
        let depth_to_root = "../..";
        if let Some(part) = partition {
            fs::create_dir_all(disk_dir.join(part)).unwrap();
            let target = format!(
                "{depth_to_root}/devices/pci0000:00/usb1/1-2/1-2:1.0/host4/target4:0:0/4:0:0:0/block/{disk}/{part}"
            );
            symlink(target, class.join(part)).unwrap();
        }
        let target = format!(
            "{depth_to_root}/devices/pci0000:00/usb1/1-2/1-2:1.0/host4/target4:0:0/4:0:0:0/block/{disk}"
        );
        symlink(target, class.join(disk)).unwrap();
    }

    #[test]
    fn resolves_partition_to_usb_identity() {
        let dir = tempdir().unwrap();
        fake_usb_tree(dir.path(), "sdb", Some("sdb1"), "1949", "G000AA1234567890");
        let id = resolve_usb_identity(dir.path(), Path::new("/dev/sdb1")).unwrap();
        assert_eq!(id.id_vendor, "1949");
        assert_eq!(id.serial, "G000AA1234567890");
    }

    #[test]
    fn resolves_whole_disk_node() {
        let dir = tempdir().unwrap();
        fake_usb_tree(dir.path(), "sdb", None, "1949", "G000AA1234567890");
        let id = resolve_usb_identity(dir.path(), Path::new("/dev/sdb")).unwrap();
        assert_eq!(id.serial, "G000AA1234567890");
    }

    #[test]
    fn no_usb_ancestor_for_non_usb_disk() {
        let dir = tempdir().unwrap();
        // SATA-like tree: no idVendor anywhere up the chain.
        let disk_dir = dir.path().join("devices/pci0000:00/ata1/host0/block/sda");
        fs::create_dir_all(&disk_dir).unwrap();
        let class = dir.path().join("class/block");
        fs::create_dir_all(&class).unwrap();
        symlink(
            "../../devices/pci0000:00/ata1/host0/block/sda",
            class.join("sda"),
        )
        .unwrap();
        let err = resolve_usb_identity(dir.path(), Path::new("/dev/sda")).unwrap_err();
        assert!(matches!(err, Error::NoUsbAncestor { .. }));
    }

    #[test]
    fn no_serial_when_usb_device_lacks_one() {
        let dir = tempdir().unwrap();
        fake_usb_tree(dir.path(), "sdb", Some("sdb1"), "1949", "");
        let err = resolve_usb_identity(dir.path(), Path::new("/dev/sdb1")).unwrap_err();
        assert!(matches!(err, Error::NoSerial { .. }));
    }

    #[test]
    fn io_error_for_missing_class_entry() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("class/block")).unwrap();
        let err = resolve_usb_identity(dir.path(), Path::new("/dev/sdz1")).unwrap_err();
        assert!(matches!(err, Error::Io { .. }));
    }

    #[test]
    fn normalize_relative_pops_parent_components() {
        let out = normalize_relative(Path::new("/sys/class/block"), Path::new("../../devices/x"));
        assert_eq!(out, Path::new("/sys/devices/x"));
    }
}
