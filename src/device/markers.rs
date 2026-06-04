use std::path::Path;

pub fn looks_like_kindle(mount_root: &Path) -> bool {
    mount_root.join("documents").is_dir() && mount_root.join("system").is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn true_when_both_marker_dirs_exist() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("documents")).unwrap();
        fs::create_dir(dir.path().join("system")).unwrap();
        assert!(looks_like_kindle(dir.path()));
    }

    #[test]
    fn false_when_a_marker_is_missing() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("documents")).unwrap();
        assert!(!looks_like_kindle(dir.path()));
    }

    #[test]
    fn false_on_empty_root() {
        let dir = tempdir().unwrap();
        assert!(!looks_like_kindle(dir.path()));
    }
}
