use std::path::Path;

// `documents/` alone, deliberately: an MTP Kindle publishes its user storage
// but not the `system/` partition, so requiring both would reject every
// Colorsoft/Scribe. This is advisory anyway — the USB vendor id decides.
pub fn looks_like_kindle(mount_root: &Path) -> bool {
    mount_root.join("documents").is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn true_on_a_mass_storage_layout() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("documents")).unwrap();
        fs::create_dir(dir.path().join("system")).unwrap();
        assert!(looks_like_kindle(dir.path()));
    }

    #[test]
    fn true_on_an_mtp_storage_without_system() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("documents")).unwrap();
        assert!(looks_like_kindle(dir.path()));
    }

    #[test]
    fn false_when_documents_is_missing() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("system")).unwrap();
        assert!(!looks_like_kindle(dir.path()));
    }

    #[test]
    fn false_on_empty_root() {
        let dir = tempdir().unwrap();
        assert!(!looks_like_kindle(dir.path()));
    }
}
