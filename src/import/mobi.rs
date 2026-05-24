use std::path::Path;

use mobi::headers::Language;
use mobi::Mobi;

use super::{Error, Metadata, Result};
use crate::catalog::tags;

pub fn extract(path: &Path) -> Result<Metadata> {
    let book = Mobi::from_path(path).map_err(|source| Error::Mobi {
        path: path.to_path_buf(),
        source,
    })?;
    let title = nonempty(book.title().trim().to_string());
    let author = book.author().and_then(|a| nonempty(a.trim().to_string()));
    let description = book
        .description()
        .and_then(|s| nonempty(s.trim().to_string()));
    let publisher = book
        .publisher()
        .and_then(|s| nonempty(s.trim().to_string()));
    let isbn = book.isbn().and_then(|s| nonempty(s.trim().to_string()));
    let published_date = book
        .publish_date()
        .and_then(|s| nonempty(s.trim().to_string()));
    let language = match book.language() {
        Language::Neutral => None,
        other => Some(format!("{other:?}").to_lowercase()),
    };
    let tags = book
        .metadata
        .subjects()
        .map(tags::normalize_many)
        .unwrap_or_default();

    Ok(Metadata {
        title,
        author,
        description,
        series_name: None,
        series_index: None,
        isbn,
        publisher,
        language,
        published_date,
        tags,
    })
}

fn nonempty(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}
