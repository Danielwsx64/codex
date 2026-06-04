use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountEntry {
    pub device: PathBuf,
    pub mount_point: PathBuf,
    pub fstype: String,
}

pub fn parse(contents: &str) -> Vec<MountEntry> {
    contents
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_ascii_whitespace();
            let device = fields.next()?;
            let mount_point = fields.next()?;
            let fstype = fields.next()?;
            Some(MountEntry {
                device: PathBuf::from(unescape_octal(device)),
                mount_point: PathBuf::from(unescape_octal(mount_point)),
                fstype: fstype.to_string(),
            })
        })
        .collect()
}

// /proc/mounts escapes space, tab, newline and backslash in paths as \040,
// \011, \012 and \134 so the line stays whitespace-splittable.
fn unescape_octal(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let mut chars = field.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let digits: String = chars.clone().take(3).collect();
        match (digits.len() == 3).then(|| u8::from_str_radix(&digits, 8).ok()) {
            Some(Some(byte)) => {
                out.push(byte as char);
                chars.nth(2);
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn parses_typical_mounts_file() {
        let sample = "\
proc /proc proc rw,nosuid,nodev,noexec,relatime 0 0
/dev/nvme0n1p2 / ext4 rw,relatime 0 0
/dev/sdb1 /media/user/Kindle vfat rw,nosuid,nodev,relatime 0 0
";
        let entries = parse(sample);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[2].device, Path::new("/dev/sdb1"));
        assert_eq!(entries[2].mount_point, Path::new("/media/user/Kindle"));
        assert_eq!(entries[2].fstype, "vfat");
    }

    #[test]
    fn skips_short_and_empty_lines() {
        let entries = parse("garbage\n\n/dev/sda1 /mnt\n");
        assert!(entries.is_empty());
    }

    #[test]
    fn decodes_octal_escapes_in_mount_point() {
        let entries = parse("/dev/sdb1 /media/My\\040Kindle\\134x vfat rw 0 0\n");
        assert_eq!(entries[0].mount_point, Path::new("/media/My Kindle\\x"));
    }

    #[test]
    fn keeps_lone_backslash_without_octal_digits() {
        assert_eq!(unescape_octal("a\\zb"), "a\\zb");
        assert_eq!(unescape_octal("trailing\\"), "trailing\\");
    }
}
