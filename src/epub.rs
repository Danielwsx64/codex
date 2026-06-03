use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::reader::Reader;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io error on {}: {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("epub `{}` is not a valid zip: {source}", .path.display())]
    Zip {
        path: PathBuf,
        #[source]
        source: zip::result::ZipError,
    },
    #[error("epub `{}` xml error: {source}", .path.display())]
    Xml {
        path: PathBuf,
        #[source]
        source: quick_xml::Error,
    },
    #[error("epub `{}` is missing META-INF/container.xml or the OPF rootfile", .path.display())]
    MissingOpf { path: PathBuf },
    #[error("epub `{}` spine references unknown manifest id `{idref}`", .path.display())]
    BrokenSpine { path: PathBuf, idref: String },
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub struct SpineItem {
    pub idref: String,
    pub href: String,
    pub media_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TocEntry {
    pub title: String,
    pub href: String,
}

pub struct Epub {
    path: PathBuf,
    archive: zip::ZipArchive<Cursor<Vec<u8>>>,
    opf_dir: String,
    manifest: HashMap<String, ManifestItem>,
    spine: Vec<SpineItem>,
    toc: Vec<TocEntry>,
}

#[derive(Debug, Clone)]
struct ManifestItem {
    href: String,
    media_type: Option<String>,
}

impl Epub {
    pub fn open(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let mut archive =
            zip::ZipArchive::new(Cursor::new(bytes)).map_err(|source| Error::Zip {
                path: path.to_path_buf(),
                source,
            })?;

        let opf_path = find_opf_path(&mut archive, path)?;
        let opf_dir = parent_dir(&opf_path).to_string();
        let opf_bytes = read_entry(&mut archive, &opf_path, path)?;
        let parsed = parse_opf(&opf_bytes, path)?;

        let mut spine = Vec::with_capacity(parsed.spine_idrefs.len());
        for idref in &parsed.spine_idrefs {
            let item = parsed
                .manifest
                .get(idref)
                .ok_or_else(|| Error::BrokenSpine {
                    path: path.to_path_buf(),
                    idref: idref.clone(),
                })?;
            spine.push(SpineItem {
                idref: idref.clone(),
                href: item.href.clone(),
                media_type: item.media_type.clone(),
            });
        }

        let toc = read_toc(&mut archive, &parsed, &opf_dir, path).unwrap_or_default();

        Ok(Self {
            path: path.to_path_buf(),
            archive,
            opf_dir,
            manifest: parsed.manifest,
            spine,
            toc,
        })
    }

    pub fn spine(&self) -> &[SpineItem] {
        &self.spine
    }

    pub fn toc(&self) -> &[TocEntry] {
        &self.toc
    }

    pub fn read_spine_item(&mut self, idx: usize) -> Result<Vec<u8>> {
        let item = self.spine.get(idx).ok_or_else(|| Error::BrokenSpine {
            path: self.path.clone(),
            idref: format!("(index {idx})"),
        })?;
        let full = resolve_relative(&self.opf_dir, &item.href);
        read_entry(&mut self.archive, &full, &self.path)
    }

    pub fn manifest_lookup(&self, href: &str) -> Option<&str> {
        for (idref, item) in &self.manifest {
            if item.href == href {
                return Some(idref);
            }
        }
        None
    }
}

struct ParsedOpf {
    manifest: HashMap<String, ManifestItem>,
    spine_idrefs: Vec<String>,
    ncx_idref: Option<String>,
    nav_idref: Option<String>,
}

fn parse_opf(opf: &[u8], path: &Path) -> Result<ParsedOpf> {
    let mut reader = Reader::from_reader(opf);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut manifest: HashMap<String, ManifestItem> = HashMap::new();
    let mut spine_idrefs: Vec<String> = Vec::new();
    let mut ncx_idref: Option<String> = None;
    let mut nav_idref: Option<String> = None;

    loop {
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|source| Error::Xml {
                path: path.to_path_buf(),
                source,
            })?;
        match event {
            Event::Eof => break,
            Event::Start(ref e) | Event::Empty(ref e) => {
                let local = local_name(e.name().as_ref()).to_string();
                if local == "item" {
                    let attrs = collect_attrs(e);
                    let id = attrs_get(&attrs, "id");
                    let href = attrs_get(&attrs, "href");
                    let media_type = attrs_get(&attrs, "media-type");
                    let properties = attrs_get(&attrs, "properties").unwrap_or_default();
                    if let (Some(id), Some(href)) = (id, href) {
                        if properties.split_whitespace().any(|p| p == "nav") {
                            nav_idref = Some(id.clone());
                        }
                        manifest.insert(
                            id,
                            ManifestItem {
                                href,
                                media_type: media_type.clone(),
                            },
                        );
                    }
                } else if local == "itemref" {
                    let attrs = collect_attrs(e);
                    if let Some(idref) = attrs_get(&attrs, "idref") {
                        spine_idrefs.push(idref);
                    }
                } else if local == "spine" {
                    let attrs = collect_attrs(e);
                    ncx_idref = attrs_get(&attrs, "toc");
                }
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(ParsedOpf {
        manifest,
        spine_idrefs,
        ncx_idref,
        nav_idref,
    })
}

fn read_toc<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    parsed: &ParsedOpf,
    opf_dir: &str,
    path: &Path,
) -> Result<Vec<TocEntry>> {
    if let Some(ncx_id) = &parsed.ncx_idref {
        if let Some(item) = parsed.manifest.get(ncx_id) {
            let full = resolve_relative(opf_dir, &item.href);
            if let Ok(bytes) = read_entry(archive, &full, path) {
                let entries = parse_ncx(&bytes);
                if !entries.is_empty() {
                    return Ok(entries);
                }
            }
        }
    }
    if let Some(nav_id) = &parsed.nav_idref {
        if let Some(item) = parsed.manifest.get(nav_id) {
            let full = resolve_relative(opf_dir, &item.href);
            if let Ok(bytes) = read_entry(archive, &full, path) {
                let entries = parse_nav(&bytes);
                if !entries.is_empty() {
                    return Ok(entries);
                }
            }
        }
    }
    Ok(Vec::new())
}

fn parse_ncx(bytes: &[u8]) -> Vec<TocEntry> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    let mut entries: Vec<TocEntry> = Vec::new();
    let mut current_label: Option<String> = None;
    let mut capture_text = false;
    let mut in_navlabel = false;

    loop {
        let Ok(event) = reader.read_event_into(&mut buf) else {
            break;
        };
        match event {
            Event::Eof => break,
            Event::Start(ref e) | Event::Empty(ref e) => {
                let local = local_name(e.name().as_ref()).to_string();
                if local == "navPoint" {
                    current_label = None;
                } else if local == "navLabel" {
                    in_navlabel = true;
                } else if local == "text" && in_navlabel {
                    capture_text = true;
                } else if local == "content" {
                    let attrs = collect_attrs(e);
                    if let Some(src) = attrs_get(&attrs, "src") {
                        let title = current_label.clone().unwrap_or_else(|| src.clone());
                        entries.push(TocEntry { title, href: src });
                    }
                }
            }
            Event::Text(ref t) if capture_text => {
                if let Ok(s) = t.decode() {
                    current_label = Some(s.into_owned());
                }
            }
            Event::End(ref e) => {
                let local = local_name(e.name().as_ref()).to_string();
                if local == "text" {
                    capture_text = false;
                } else if local == "navLabel" {
                    in_navlabel = false;
                } else if local == "navPoint" {
                    current_label = None;
                }
            }
            _ => {}
        }
        buf.clear();
    }
    entries
}

fn parse_nav(bytes: &[u8]) -> Vec<TocEntry> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut entries: Vec<TocEntry> = Vec::new();
    let mut in_toc_nav = false;
    let mut nav_depth = 0i32;
    let mut current_href: Option<String> = None;
    let mut current_text = String::new();
    let mut in_anchor = false;

    loop {
        let Ok(event) = reader.read_event_into(&mut buf) else {
            break;
        };
        match event {
            Event::Eof => break,
            Event::Start(ref e) => {
                let local = local_name(e.name().as_ref()).to_string();
                if local == "nav" {
                    let attrs = collect_attrs(e);
                    let role = attrs_get(&attrs, "type").unwrap_or_default();
                    if role.eq_ignore_ascii_case("toc") || !in_toc_nav {
                        in_toc_nav =
                            role.eq_ignore_ascii_case("toc") || in_toc_nav || nav_depth == 0;
                    }
                    nav_depth += 1;
                } else if local == "a" && in_toc_nav {
                    let attrs = collect_attrs(e);
                    current_href = attrs_get(&attrs, "href");
                    current_text.clear();
                    in_anchor = true;
                }
            }
            Event::Text(ref t) if in_anchor => {
                if let Ok(s) = t.decode() {
                    current_text.push_str(s.as_ref());
                }
            }
            Event::End(ref e) => {
                let local = local_name(e.name().as_ref()).to_string();
                if local == "nav" {
                    nav_depth -= 1;
                    if nav_depth == 0 {
                        in_toc_nav = false;
                    }
                } else if local == "a" && in_anchor {
                    if let Some(href) = current_href.take() {
                        let title = current_text.trim().to_string();
                        let final_title = if title.is_empty() {
                            href.clone()
                        } else {
                            title
                        };
                        entries.push(TocEntry {
                            title: final_title,
                            href,
                        });
                    }
                    current_text.clear();
                    in_anchor = false;
                }
            }
            _ => {}
        }
        buf.clear();
    }
    entries
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
    Err(Error::MissingOpf {
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

fn attrs_get(attrs: &[(String, String)], key: &str) -> Option<String> {
    attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

fn parent_dir(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..i],
        None => "",
    }
}

fn resolve_relative(base: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or(href);
    if base.is_empty() {
        return href.to_string();
    }
    let mut parts: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
    for segment in href.split('/') {
        match segment {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_relative_joins_paths() {
        assert_eq!(resolve_relative("OEBPS", "ch1.xhtml"), "OEBPS/ch1.xhtml");
        assert_eq!(
            resolve_relative("OEBPS/text", "../images/cover.png"),
            "OEBPS/images/cover.png"
        );
        assert_eq!(resolve_relative("", "ch1.xhtml"), "ch1.xhtml");
    }

    #[test]
    fn resolve_relative_strips_fragments() {
        assert_eq!(
            resolve_relative("OEBPS", "ch1.xhtml#section-2"),
            "OEBPS/ch1.xhtml"
        );
    }

    #[test]
    fn parent_dir_picks_directory_component() {
        assert_eq!(parent_dir("OEBPS/content.opf"), "OEBPS");
        assert_eq!(parent_dir("content.opf"), "");
        assert_eq!(parent_dir("a/b/c.opf"), "a/b");
    }

    #[test]
    fn opens_sample_fixture_and_walks_spine() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let fixture = manifest.join("tests").join("fixtures").join("sample.epub");
        let mut book = Epub::open(&fixture).expect("opens sample epub");
        assert!(!book.spine().is_empty(), "spine has at least one entry");
        let first = book.read_spine_item(0).expect("reads first spine item");
        assert!(!first.is_empty(), "first spine item has bytes");
    }
}
