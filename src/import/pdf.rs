use std::path::Path;

use lopdf::{Document, Object};

use super::{Error, Metadata, Result};

pub fn extract(path: &Path) -> Result<Metadata> {
    let doc = Document::load(path).map_err(|source| Error::Pdf {
        path: path.to_path_buf(),
        source,
    })?;
    let info_id = match doc.trailer.get(b"Info").and_then(Object::as_reference) {
        Ok(id) => id,
        Err(_) => return Ok(Metadata::default()),
    };
    let info = match doc.get_dictionary(info_id) {
        Ok(d) => d,
        Err(_) => return Ok(Metadata::default()),
    };
    let title = info.get(b"Title").ok().and_then(decode_pdf_string);
    let author = info.get(b"Author").ok().and_then(decode_pdf_string);
    Ok(Metadata { title, author })
}

fn decode_pdf_string(obj: &Object) -> Option<String> {
    let bytes = obj.as_str().ok()?;
    if bytes.is_empty() {
        return None;
    }
    let s = if bytes.starts_with(&[0xFE, 0xFF]) {
        // UTF-16BE with BOM.
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    };
    let trimmed = s.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
