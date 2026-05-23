use std::path::Path;

use rbook::Ebook;

use super::{Error, Metadata, Result};

pub fn extract(path: &Path) -> Result<Metadata> {
    let epub = rbook::Epub::new(path).map_err(|source| Error::Epub {
        path: path.to_path_buf(),
        source,
    })?;
    let meta = epub.metadata();
    let title = meta.title().map(|el| el.value().to_string());
    let author = first_creator(meta);
    Ok(Metadata { title, author })
}

fn first_creator(meta: &rbook::epub::Metadata) -> Option<String> {
    let creators = meta.creators();
    let mut iter = creators.into_iter().map(|el| el.value().trim().to_string());
    iter.find(|s| !s.is_empty())
}
