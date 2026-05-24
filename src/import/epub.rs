use std::io::{Cursor, Read};
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::reader::Reader;

use super::{Error, Metadata, Result};
use crate::catalog::tags;

pub fn extract(path: &Path) -> Result<Metadata> {
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut archive =
        zip::ZipArchive::new(Cursor::new(&bytes)).map_err(|source| Error::EpubZip {
            path: path.to_path_buf(),
            source,
        })?;
    let opf_path = find_opf_path(&mut archive, path)?;
    let opf_bytes = read_entry(&mut archive, &opf_path, path)?;
    Ok(parse_opf_metadata(&opf_bytes))
}

fn read_entry<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
    path: &Path,
) -> Result<Vec<u8>> {
    let mut entry = archive.by_name(name).map_err(|source| Error::EpubZip {
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
            .map_err(|source| Error::EpubXml {
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
    Err(Error::EpubMissingOpf {
        path: path.to_path_buf(),
    })
}

fn local_name(qname: &[u8]) -> &str {
    let s = std::str::from_utf8(qname).unwrap_or("");
    match s.rsplit_once(':') {
        Some((_, local)) => local,
        None => s,
    }
}

fn parse_opf_metadata(opf: &[u8]) -> Metadata {
    let mut reader = Reader::from_reader(opf);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut in_metadata = false;
    let mut depth = 0usize;

    let mut current: Option<MetaCollector> = None;
    let mut meta = Metadata::default();
    let mut creators: Vec<String> = Vec::new();
    let mut subjects: Vec<String> = Vec::new();
    let mut isbn: Option<String> = None;

    loop {
        let event = match reader.read_event_into(&mut buf) {
            Ok(ev) => ev,
            Err(_) => break,
        };
        match event {
            Event::Eof => break,
            Event::Start(ref e) => {
                depth += 1;
                let local = local_name(e.name().as_ref()).to_string();
                if !in_metadata && local == "metadata" {
                    in_metadata = true;
                    continue;
                }
                if in_metadata && depth >= 2 {
                    current = Some(MetaCollector {
                        local: local.clone(),
                        attrs: collect_attrs(e),
                        text: String::new(),
                    });
                }
            }
            Event::Text(ref t) => {
                if let Some(c) = current.as_mut() {
                    if let Ok(s) = t.decode() {
                        c.text.push_str(&s);
                    }
                }
            }
            Event::CData(ref t) => {
                if let Some(c) = current.as_mut() {
                    c.text.push_str(&String::from_utf8_lossy(t.as_ref()));
                }
            }
            Event::End(ref e) => {
                let local = local_name(e.name().as_ref()).to_string();
                if in_metadata && local == "metadata" {
                    in_metadata = false;
                }
                if let Some(c) = current.take() {
                    apply_meta(c, &mut meta, &mut creators, &mut subjects, &mut isbn);
                }
                depth = depth.saturating_sub(1);
            }
            Event::Empty(ref e) => {
                let local = local_name(e.name().as_ref()).to_string();
                if in_metadata {
                    let collector = MetaCollector {
                        local,
                        attrs: collect_attrs(e),
                        text: String::new(),
                    };
                    apply_meta(
                        collector,
                        &mut meta,
                        &mut creators,
                        &mut subjects,
                        &mut isbn,
                    );
                }
            }
            _ => {}
        }
        buf.clear();
    }

    meta.author = creators.into_iter().find(|s| !s.is_empty());
    meta.tags = tags::normalize_many(subjects);
    meta.isbn = isbn;
    meta
}

struct MetaCollector {
    local: String,
    attrs: Vec<(String, String)>,
    text: String,
}

fn collect_attrs(e: &quick_xml::events::BytesStart<'_>) -> Vec<(String, String)> {
    e.attributes()
        .filter_map(|a| a.ok())
        .map(|a| {
            (
                local_name(a.key.as_ref()).to_string(),
                String::from_utf8_lossy(a.value.as_ref()).into_owned(),
            )
        })
        .collect()
}

fn apply_meta(
    c: MetaCollector,
    meta: &mut Metadata,
    creators: &mut Vec<String>,
    subjects: &mut Vec<String>,
    isbn: &mut Option<String>,
) {
    let value = c.text.trim().to_string();
    let get = |k: &str| {
        c.attrs
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
    };
    match c.local.as_str() {
        "title" if !value.is_empty() && meta.title.is_none() => {
            meta.title = Some(value);
        }
        "creator" if !value.is_empty() => {
            creators.push(value);
        }
        "subject" if !value.is_empty() => {
            subjects.push(value);
        }
        "description" if !value.is_empty() && meta.description.is_none() => {
            meta.description = Some(value);
        }
        "publisher" if !value.is_empty() && meta.publisher.is_none() => {
            meta.publisher = Some(value);
        }
        "language" if !value.is_empty() && meta.language.is_none() => {
            meta.language = Some(value);
        }
        "date" if !value.is_empty() && meta.published_date.is_none() => {
            meta.published_date = Some(value);
        }
        "identifier" => {
            let scheme = get("scheme").unwrap_or_default();
            if scheme.eq_ignore_ascii_case("isbn") && !value.is_empty() {
                *isbn = Some(value);
            } else if isbn.is_none() {
                let lower = value.to_ascii_lowercase();
                if let Some(rest) = lower.strip_prefix("isbn:") {
                    *isbn = Some(rest.trim().to_string());
                }
            }
        }
        "meta" => {
            // Legacy OPF 2 meta: name="..." content="..."
            let name = get("name").unwrap_or_default();
            let content = get("content").unwrap_or_default();
            match name.as_str() {
                "calibre:series" if !content.trim().is_empty() => {
                    meta.series_name = Some(content.trim().to_string());
                }
                "calibre:series_index" => {
                    if let Ok(v) = content.trim().parse::<f64>() {
                        meta.series_index = Some(v);
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest.join("tests").join("fixtures").join(name)
    }

    #[test]
    fn extracts_basic_fields_from_sample_fixture() {
        let meta = extract(&fixture("sample.epub")).unwrap();
        assert_eq!(meta.title.as_deref(), Some("Sample Book"));
        assert_eq!(meta.author.as_deref(), Some("Jane Doe"));
        assert_eq!(meta.language.as_deref(), Some("en"));
    }

    #[test]
    fn parse_opf_handles_calibre_style_metadata() {
        let opf = br#"<?xml version='1.0' encoding='utf-8'?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:title>contrato compromisso</dc:title>
    <dc:creator opf:role="aut">Daniel Lima</dc:creator>
    <dc:subject>contrato</dc:subject>
    <dc:description>Muito Legal</dc:description>
    <dc:publisher>Daniel</dc:publisher>
    <dc:language>Portugues</dc:language>
    <dc:date>Maio</dc:date>
    <dc:identifier opf:scheme="ISBN">12345</dc:identifier>
    <dc:identifier id="cal" opf:scheme="calibre">c6af8c37</dc:identifier>
    <meta name="calibre:series" content="My Series"/>
    <meta name="calibre:series_index" content="2.5"/>
  </metadata>
  <manifest>
    <item id="id1" href="docx.css" media-type="text/css"/>
  </manifest>
</package>"#;
        let m = parse_opf_metadata(opf);
        assert_eq!(m.title.as_deref(), Some("contrato compromisso"));
        assert_eq!(m.author.as_deref(), Some("Daniel Lima"));
        assert_eq!(m.tags, vec!["contrato"]);
        assert_eq!(m.description.as_deref(), Some("Muito Legal"));
        assert_eq!(m.publisher.as_deref(), Some("Daniel"));
        assert_eq!(m.language.as_deref(), Some("Portugues"));
        assert_eq!(m.published_date.as_deref(), Some("Maio"));
        assert_eq!(m.isbn.as_deref(), Some("12345"));
        assert_eq!(m.series_name.as_deref(), Some("My Series"));
        assert_eq!(m.series_index, Some(2.5));
    }

    #[test]
    fn parse_opf_ignores_outside_metadata_block() {
        let opf = br#"<?xml version="1.0"?>
<package>
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Inside</dc:title>
  </metadata>
  <other>
    <dc:title>Outside should be ignored</dc:title>
  </other>
</package>"#;
        let m = parse_opf_metadata(opf);
        assert_eq!(m.title.as_deref(), Some("Inside"));
    }

    #[test]
    fn parse_opf_full_calibre_epub3_output() {
        // Mirrors what `calibre` writes when exporting/editing an EPUB 3,
        // including multiple identifiers, contributor records, and the
        // EPUB-3-required `<meta property="dcterms:modified">`.
        let opf = br#"<?xml version='1.0' encoding='utf-8'?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="uuid_id" version="3.0">
  <metadata xmlns:calibre="http://calibre.kovidgoyal.net/2009/metadata"
            xmlns:dc="http://purl.org/dc/elements/1.1/"
            xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:identifier opf:scheme="calibre" id="calibre_id">abc-123</dc:identifier>
    <dc:identifier opf:scheme="uuid" id="uuid_id">xyz-789</dc:identifier>
    <dc:title>The Book</dc:title>
    <dc:creator opf:file-as="Doe, Jane" opf:role="aut">Jane Doe</dc:creator>
    <dc:creator opf:role="aut">Second Author</dc:creator>
    <dc:contributor opf:file-as="calibre" opf:role="bkp">calibre (5.0)</dc:contributor>
    <dc:publisher>Publisher Inc</dc:publisher>
    <dc:identifier opf:scheme="ISBN">9781234567890</dc:identifier>
    <dc:identifier opf:scheme="DOI">10.1234/abc</dc:identifier>
    <dc:date>2020-01-15T00:00:00+00:00</dc:date>
    <dc:description>A description of the book.</dc:description>
    <dc:language>en</dc:language>
    <dc:subject>Fiction</dc:subject>
    <dc:subject>Sci-Fi</dc:subject>
    <meta name="calibre:series" content="My Series"/>
    <meta name="calibre:series_index" content="3.0"/>
    <meta name="calibre:rating" content="8"/>
    <meta name="calibre:timestamp" content="2024-01-01T00:00:00+00:00"/>
    <meta name="calibre:title_sort" content="Book, The"/>
    <meta property="dcterms:modified">2024-01-15T00:00:00Z</meta>
  </metadata>
</package>"#;
        let m = parse_opf_metadata(opf);
        assert_eq!(m.title.as_deref(), Some("The Book"));
        // First non-empty creator wins (matches Calibre's display behavior).
        assert_eq!(m.author.as_deref(), Some("Jane Doe"));
        assert_eq!(m.description.as_deref(), Some("A description of the book."));
        assert_eq!(m.publisher.as_deref(), Some("Publisher Inc"));
        assert_eq!(m.language.as_deref(), Some("en"));
        assert_eq!(
            m.published_date.as_deref(),
            Some("2020-01-15T00:00:00+00:00")
        );
        // Of the three identifiers, only the ISBN one is picked.
        assert_eq!(m.isbn.as_deref(), Some("9781234567890"));
        assert_eq!(m.series_name.as_deref(), Some("My Series"));
        assert_eq!(m.series_index, Some(3.0));
        assert_eq!(m.tags, vec!["Fiction", "Sci-Fi"]);
    }

    #[test]
    fn parse_opf_dedups_subjects_case_insensitive() {
        let opf = br#"<package><metadata xmlns:dc="x">
            <dc:subject>Fiction</dc:subject>
            <dc:subject>fiction</dc:subject>
            <dc:subject>Sci-Fi</dc:subject>
        </metadata></package>"#;
        let m = parse_opf_metadata(opf);
        assert_eq!(m.tags, vec!["Fiction", "Sci-Fi"]);
    }
}
