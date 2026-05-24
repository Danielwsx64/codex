use std::path::Path;

use lopdf::{dictionary, Document, Object, ObjectId};

use super::{write_atomic, Error, Result};
use crate::catalog::books::Book;

pub fn write(path: &Path, book: &Book) -> Result<()> {
    let mut doc = Document::load(path).map_err(|source| Error::Pdf {
        path: path.to_path_buf(),
        source,
    })?;

    let info_id = ensure_info(&mut doc);
    let info = doc
        .objects
        .entry(info_id)
        .or_insert_with(|| Object::Dictionary(dictionary! {}));
    let Object::Dictionary(info_dict) = info else {
        return Err(Error::Pdf {
            path: path.to_path_buf(),
            source: lopdf::Error::Type,
        });
    };

    set_or_remove(info_dict, "Title", Some(book.title.as_str()));
    set_or_remove(info_dict, "Author", book.author.as_deref());
    set_or_remove(info_dict, "Subject", book.description.as_deref());
    let keywords = if book.tags.is_empty() {
        None
    } else {
        Some(book.tags.join(", "))
    };
    set_or_remove(info_dict, "Keywords", keywords.as_deref());

    write_atomic(path, |file| {
        doc.save_to(file).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })
    })
}

fn ensure_info(doc: &mut Document) -> ObjectId {
    if let Ok(id) = doc.trailer.get(b"Info").and_then(Object::as_reference) {
        return id;
    }
    let id = doc.add_object(Object::Dictionary(dictionary! {}));
    doc.trailer.set("Info", Object::Reference(id));
    id
}

fn set_or_remove(dict: &mut lopdf::Dictionary, key: &'static str, value: Option<&str>) {
    match value {
        Some(v) if !v.is_empty() => dict.set(key, Object::string_literal(v)),
        _ => {
            let _ = dict.remove(key.as_bytes());
        }
    }
}
