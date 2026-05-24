use std::io::{Cursor, Read, Write};
use std::path::Path;

use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;
use zip::write::{SimpleFileOptions, ZipWriter};

use super::{write_atomic, Error, Result};
use crate::catalog::books::Book;

pub fn write(path: &Path, book: &Book) -> Result<()> {
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).map_err(|source| Error::Zip {
        path: path.to_path_buf(),
        source,
    })?;

    let opf_path = find_opf_path(&mut archive, path)?;
    let opf_bytes = read_entry(&mut archive, &opf_path, path)?;
    let new_opf = rewrite_opf(&opf_bytes, book, path)?;

    write_atomic(path, |file| {
        let mut writer = ZipWriter::new(file);
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|source| Error::Zip {
                path: path.to_path_buf(),
                source,
            })?;
            let name = entry.name().to_string();
            let method = entry.compression();
            let options = SimpleFileOptions::default().compression_method(method);
            if name == opf_path {
                writer
                    .start_file(&name, options)
                    .map_err(|source| Error::Zip {
                        path: path.to_path_buf(),
                        source,
                    })?;
                writer.write_all(&new_opf).map_err(|source| Error::Io {
                    path: path.to_path_buf(),
                    source,
                })?;
            } else if entry.is_dir() {
                writer
                    .add_directory(&name, options)
                    .map_err(|source| Error::Zip {
                        path: path.to_path_buf(),
                        source,
                    })?;
            } else {
                writer
                    .start_file(&name, options)
                    .map_err(|source| Error::Zip {
                        path: path.to_path_buf(),
                        source,
                    })?;
                let mut buf = Vec::with_capacity(entry.size() as usize);
                entry.read_to_end(&mut buf).map_err(|source| Error::Io {
                    path: path.to_path_buf(),
                    source,
                })?;
                writer.write_all(&buf).map_err(|source| Error::Io {
                    path: path.to_path_buf(),
                    source,
                })?;
            }
        }
        writer.finish().map_err(|source| Error::Zip {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(())
    })
}

fn find_opf_path<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    path: &Path,
) -> Result<String> {
    let container = read_entry(archive, "META-INF/container.xml", path)?;
    let mut reader = Reader::from_reader(container.as_slice());
    let mut buf = Vec::new();
    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|source| Error::Xml {
                path: path.to_path_buf(),
                source,
            })? {
            Event::Empty(e) | Event::Start(e) if local_name(e.name().as_ref()) == "rootfile" => {
                for attr in e.attributes().filter_map(|a| a.ok()) {
                    if local_name(attr.key.as_ref()) == "full-path" {
                        return Ok(String::from_utf8_lossy(attr.value.as_ref()).into_owned());
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Err(Error::InvalidEpub {
        path: path.to_path_buf(),
        reason: "META-INF/container.xml has no rootfile/full-path".into(),
    })
}

fn read_entry<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
    path: &Path,
) -> Result<Vec<u8>> {
    let mut entry = archive.by_name(name).map_err(|source| Error::Zip {
        path: path.to_path_buf(),
        source,
    })?;
    let mut buf = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut buf).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(buf)
}

fn local_name(qname: &[u8]) -> &str {
    let s = std::str::from_utf8(qname).unwrap_or("");
    match s.rsplit_once(':') {
        Some((_, local)) => local,
        None => s,
    }
}

fn rewrite_opf(opf: &[u8], book: &Book, src_path: &Path) -> Result<Vec<u8>> {
    let mut reader = Reader::from_reader(opf);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Cursor::new(Vec::new()));

    let mut buf = Vec::new();
    let mut in_metadata = false;
    let mut depth: usize = 0;
    let mut skip_until_depth: Option<usize> = None;

    loop {
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|source| Error::Xml {
                path: src_path.to_path_buf(),
                source,
            })?;
        match event {
            Event::Eof => break,
            Event::Start(ref e) => {
                depth += 1;
                let local = local_name(e.name().as_ref()).to_string();
                if !in_metadata && local == "metadata" {
                    in_metadata = true;
                    writer
                        .write_event(Event::Start(e.clone()))
                        .map_err(io_err(src_path))?;
                    continue;
                }
                if in_metadata && skip_until_depth.is_none() && is_managed_dc(&local, e) {
                    skip_until_depth = Some(depth - 1);
                    continue;
                }
                if skip_until_depth.is_none() {
                    writer
                        .write_event(Event::Start(e.clone()))
                        .map_err(io_err(src_path))?;
                }
            }
            Event::End(ref e) => {
                let local = local_name(e.name().as_ref()).to_string();
                if let Some(d) = skip_until_depth {
                    if depth == d + 1 {
                        skip_until_depth = None;
                        depth -= 1;
                        continue;
                    }
                }
                if in_metadata && local == "metadata" {
                    emit_managed(&mut writer, book, src_path)?;
                    in_metadata = false;
                }
                if skip_until_depth.is_none() {
                    writer
                        .write_event(Event::End(e.clone()))
                        .map_err(io_err(src_path))?;
                }
                depth -= 1;
            }
            Event::Empty(ref e) => {
                let local = local_name(e.name().as_ref()).to_string();
                if in_metadata && skip_until_depth.is_none() && is_managed_empty(&local, e) {
                    continue;
                }
                if skip_until_depth.is_none() {
                    writer
                        .write_event(Event::Empty(e.clone()))
                        .map_err(io_err(src_path))?;
                }
            }
            other => {
                if skip_until_depth.is_none() {
                    writer.write_event(other).map_err(io_err(src_path))?;
                }
            }
        }
        buf.clear();
    }

    Ok(writer.into_inner().into_inner())
}

fn io_err(path: &Path) -> impl Fn(std::io::Error) -> Error + '_ {
    move |source| Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn is_managed_dc(local: &str, e: &BytesStart<'_>) -> bool {
    match local {
        "title" | "creator" | "subject" | "description" | "publisher" | "language" | "date" => true,
        "identifier" => e.attributes().filter_map(|a| a.ok()).any(|a| {
            local_name(a.key.as_ref()) == "scheme" && a.value.as_ref().eq_ignore_ascii_case(b"isbn")
        }),
        _ => false,
    }
}

fn is_managed_empty(local: &str, e: &BytesStart<'_>) -> bool {
    if local == "meta" {
        let mut attr_name: Option<Vec<u8>> = None;
        for attr in e.attributes().filter_map(|a| a.ok()) {
            if local_name(attr.key.as_ref()) == "name" {
                attr_name = Some(attr.value.into_owned());
            }
        }
        if let Some(name) = attr_name {
            return name == b"calibre:series" || name == b"calibre:series_index";
        }
    }
    false
}

fn emit_managed<W: Write>(writer: &mut Writer<W>, book: &Book, src_path: &Path) -> Result<()> {
    let map_err = io_err(src_path);

    emit_dc(writer, "dc:title", &book.title, &[], &map_err)?;
    if let Some(author) = &book.author {
        emit_dc(
            writer,
            "dc:creator",
            author,
            &[("opf:role", "aut")],
            &map_err,
        )?;
    }
    for tag in &book.tags {
        emit_dc(writer, "dc:subject", tag, &[], &map_err)?;
    }
    if let Some(desc) = &book.description {
        emit_dc(writer, "dc:description", desc, &[], &map_err)?;
    }
    if let Some(publisher) = &book.publisher {
        emit_dc(writer, "dc:publisher", publisher, &[], &map_err)?;
    }
    if let Some(language) = &book.language {
        emit_dc(writer, "dc:language", language, &[], &map_err)?;
    }
    if let Some(date) = &book.published_date {
        emit_dc(writer, "dc:date", date, &[], &map_err)?;
    }
    if let Some(isbn) = &book.isbn {
        emit_dc(
            writer,
            "dc:identifier",
            isbn,
            &[("opf:scheme", "ISBN")],
            &map_err,
        )?;
    }
    if let Some(series) = &book.series_name {
        let mut tag = BytesStart::new("meta");
        tag.push_attribute(("name", "calibre:series"));
        tag.push_attribute(("content", series.as_str()));
        writer.write_event(Event::Empty(tag)).map_err(&map_err)?;
        if let Some(idx) = book.series_index {
            let idx_str = format_index(idx);
            let mut tag = BytesStart::new("meta");
            tag.push_attribute(("name", "calibre:series_index"));
            tag.push_attribute(("content", idx_str.as_str()));
            writer.write_event(Event::Empty(tag)).map_err(&map_err)?;
        }
    }
    Ok(())
}

fn emit_dc<W: Write, F>(
    writer: &mut Writer<W>,
    name: &str,
    value: &str,
    attrs: &[(&str, &str)],
    map_err: &F,
) -> Result<()>
where
    F: Fn(std::io::Error) -> Error,
{
    let mut start = BytesStart::new(name);
    for (k, v) in attrs {
        start.push_attribute((*k, *v));
    }
    writer
        .write_event(Event::Start(start.clone()))
        .map_err(map_err)?;
    writer
        .write_event(Event::Text(BytesText::new(value)))
        .map_err(map_err)?;
    writer
        .write_event(Event::End(BytesEnd::new(name)))
        .map_err(map_err)?;
    Ok(())
}

fn format_index(idx: f64) -> String {
    if idx.fract() == 0.0 {
        format!("{}", idx as i64)
    } else {
        format!("{idx}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn fixture_path(name: &str) -> PathBuf {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest.join("tests").join("fixtures").join(name)
    }

    fn make_book(title: &str, author: &str) -> Book {
        Book {
            id: 1,
            title: title.to_string(),
            author: Some(author.to_string()),
            format: "epub".to_string(),
            file_path: "books/1/x.epub".to_string(),
            added_at: "now".to_string(),
            description: Some("a description".into()),
            series_name: Some("Sample Series".into()),
            series_index: Some(2.5),
            rating: Some(3),
            isbn: Some("9780000000001".into()),
            publisher: Some("Acme".into()),
            language: Some("en".into()),
            published_date: Some("2025-01-01".into()),
            tags: vec!["sci-fi".into(), "classic".into()],
        }
    }

    fn read_opf(path: &Path) -> String {
        let bytes = std::fs::read(path).unwrap();
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        let opf_path = find_opf_path(&mut archive, path).unwrap();
        let mut entry = archive.by_name(&opf_path).unwrap();
        let mut s = String::new();
        entry.read_to_string(&mut s).unwrap();
        s
    }

    #[test]
    fn write_embeds_managed_fields_in_opf() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("book.epub");
        std::fs::copy(fixture_path("sample.epub"), &target).unwrap();
        let book = make_book("New Title", "New Author");

        write(&target, &book).unwrap();

        let opf = read_opf(&target);
        assert!(opf.contains("<dc:title>New Title</dc:title>"), "{opf}");
        assert!(
            opf.contains("opf:role=\"aut\"") && opf.contains(">New Author<"),
            "{opf}"
        );
        assert!(opf.contains("<dc:subject>sci-fi</dc:subject>"), "{opf}");
        assert!(opf.contains("<dc:subject>classic</dc:subject>"), "{opf}");
        assert!(
            opf.contains("opf:scheme=\"ISBN\"") && opf.contains(">9780000000001<"),
            "{opf}"
        );
        assert!(opf.contains("calibre:series"), "{opf}");
        assert!(opf.contains("Sample Series"), "{opf}");
        // Existing non-managed identifier (UUID) must be preserved.
        assert!(opf.contains("urn:uuid:"), "{opf}");
    }

    #[test]
    fn write_then_extract_reads_back_managed_fields() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("book.epub");
        std::fs::copy(fixture_path("sample.epub"), &target).unwrap();
        let book = make_book("Roundtrip Title", "Roundtrip Author");

        write(&target, &book).unwrap();

        let meta = crate::import::epub::extract(&target).unwrap();
        assert_eq!(meta.title.as_deref(), Some("Roundtrip Title"));
        assert_eq!(meta.author.as_deref(), Some("Roundtrip Author"));
        assert_eq!(meta.description.as_deref(), Some("a description"));
        assert_eq!(meta.publisher.as_deref(), Some("Acme"));
        assert_eq!(meta.language.as_deref(), Some("en"));
        assert_eq!(meta.published_date.as_deref(), Some("2025-01-01"));
        assert_eq!(meta.isbn.as_deref(), Some("9780000000001"));
        assert_eq!(meta.series_name.as_deref(), Some("Sample Series"));
        assert_eq!(meta.series_index, Some(2.5));
        let mut tags = meta.tags.clone();
        tags.sort();
        assert_eq!(tags, vec!["classic", "sci-fi"]);
    }

    #[test]
    fn write_preserves_manifest_and_spine() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("book.epub");
        std::fs::copy(fixture_path("sample.epub"), &target).unwrap();
        let book = make_book("Whatever", "Someone");

        write(&target, &book).unwrap();

        let opf = read_opf(&target);
        assert!(opf.contains("<manifest>"));
        assert!(opf.contains("<spine>"));
        assert!(opf.contains("nav.xhtml"));
        assert!(opf.contains("chap1.xhtml"));
    }
}
