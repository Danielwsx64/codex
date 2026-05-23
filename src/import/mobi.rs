use std::path::Path;

use mobi::Mobi;

use super::{Error, Metadata, Result};

pub fn extract(path: &Path) -> Result<Metadata> {
    let book = Mobi::from_path(path).map_err(|source| Error::Mobi {
        path: path.to_path_buf(),
        source,
    })?;
    let title_raw = book.title();
    let title = if title_raw.trim().is_empty() {
        None
    } else {
        Some(title_raw.trim().to_string())
    };
    let author = book.author().and_then(|a| {
        let trimmed = a.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    Ok(Metadata { title, author })
}
