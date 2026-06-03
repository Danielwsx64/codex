use std::path::Path;

use super::{Metadata, Result};

pub fn extract(path: &Path) -> Result<Metadata> {
    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(humanize_stem)
        .filter(|s| !s.is_empty());
    Ok(Metadata {
        title,
        ..Metadata::default()
    })
}

fn humanize_stem(stem: &str) -> String {
    let mut out = String::with_capacity(stem.len());
    let mut last_was_space = false;
    for ch in stem.chars() {
        let mapped = if ch == '_' || ch == '-' { ' ' } else { ch };
        if mapped == ' ' {
            if !last_was_space && !out.is_empty() {
                out.push(' ');
            }
            last_was_space = true;
        } else {
            out.push(mapped);
            last_was_space = false;
        }
    }
    while out.ends_with(' ') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn touch(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, b"").unwrap();
        p
    }

    #[test]
    fn extract_uses_filename_stem_as_title() {
        let dir = tempdir().unwrap();
        let p = touch(dir.path(), "Cosmere_Notes.txt");
        let meta = extract(&p).unwrap();
        assert_eq!(meta.title.as_deref(), Some("Cosmere Notes"));
    }

    #[test]
    fn extract_strips_hyphens_and_collapses_whitespace() {
        let dir = tempdir().unwrap();
        let p = touch(dir.path(), "war-of-the-roses__chapter-1.md");
        let meta = extract(&p).unwrap();
        assert_eq!(meta.title.as_deref(), Some("war of the roses chapter 1"));
    }

    #[test]
    fn extract_returns_dotfile_stem_verbatim() {
        // `.txt` has no embedded `.`, so Path::file_stem keeps it whole.
        // We just round-trip whatever the stem yields here.
        let dir = tempdir().unwrap();
        let p = touch(dir.path(), ".txt");
        let meta = extract(&p).unwrap();
        assert_eq!(meta.title.as_deref(), Some(".txt"));
    }
}
