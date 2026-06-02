use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tempfile::Builder;

use crate::catalog::{self, books, tags};

const HEADER: &str = "\
# cdx edit
# Save and exit to apply changes. Empty/missing fields are stored as NULL.
# Tags are a list, e.g.: tags = [\"sci-fi\", \"classic\"]. Rating is 0-5.
";

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct BookMetadataDoc {
    title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    series_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    series_index: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rating: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    isbn: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    publisher: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    published_date: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

impl BookMetadataDoc {
    fn from_book(book: &books::Book) -> Self {
        Self {
            title: book.title.clone(),
            author: book.author.clone(),
            description: book.description.clone(),
            series_name: book.series_name.clone(),
            series_index: book.series_index,
            rating: book.rating,
            isbn: book.isbn.clone(),
            publisher: book.publisher.clone(),
            language: book.language.clone(),
            published_date: book.published_date.clone(),
            tags: book.tags.clone(),
        }
    }

    fn into_update(self) -> books::BookUpdate {
        books::BookUpdate {
            title: self.title,
            author: self.author,
            description: self.description,
            series_name: self.series_name,
            series_index: self.series_index,
            rating: self.rating,
            isbn: self.isbn,
            publisher: self.publisher,
            language: self.language,
            published_date: self.published_date,
            tags: tags::normalize_many(self.tags),
        }
    }
}

pub fn dispatch(
    target: String,
    data_dir: Option<&Path>,
    catalog_override: Option<&str>,
) -> Result<()> {
    let registry = crate::cli::books::load(data_dir)?;
    let entry = crate::cli::books::resolve_entry(&registry, catalog_override)?.clone();
    let mut conn = catalog::open_existing(&entry.path)
        .with_context(|| format!("failed to open catalog `{}`", entry.name))?;

    let book = books::handle_inspect(&conn, &target)
        .with_context(|| format!("while inspecting `{target}`"))?;
    let id = book.id;

    let doc = BookMetadataDoc::from_book(&book);
    let original_toml = render_toml(&doc, id)?;

    let temp = Builder::new()
        .prefix("cdx-edit-")
        .suffix(".toml")
        .tempfile()
        .context("failed to create temp file for editor session")?;
    fs::write(temp.path(), &original_toml)
        .with_context(|| format!("failed to write temp file at {}", temp.path().display()))?;

    launch_editor(temp.path())?;

    let edited = fs::read_to_string(temp.path())
        .with_context(|| format!("failed to read edited file at {}", temp.path().display()))?;

    if edited == original_toml {
        let stdout = io::stdout();
        let mut out = stdout.lock();
        writeln!(out, "no changes")?;
        return Ok(());
    }

    let parsed: BookMetadataDoc = match toml::from_str::<BookMetadataDoc>(strip_header(&edited)) {
        Ok(v) => v,
        Err(err) => {
            let kept = preserve_tempfile(temp.into_temp_path())?;
            bail!(
                "invalid TOML in edited file: {err}\ntempfile preserved at {}",
                kept.display()
            );
        }
    };

    let update = parsed.into_update();
    match books::handle_update(&mut conn, &entry.path, id, update) {
        Ok(updated) => {
            let stdout = io::stdout();
            let mut out = stdout.lock();
            writeln!(out, "Updated book {id}: {}", updated.title)?;
            Ok(())
        }
        Err(err) => {
            let kept = preserve_tempfile(temp.into_temp_path())?;
            Err(anyhow::Error::new(err).context(format!(
                "update failed; tempfile preserved at {}",
                kept.display()
            )))
        }
    }
}

fn render_toml(doc: &BookMetadataDoc, id: i64) -> Result<String> {
    let body = toml::to_string_pretty(doc).context("failed to serialize book metadata to TOML")?;
    Ok(format!("{HEADER}# book id: {id}\n\n{body}"))
}

fn strip_header(s: &str) -> &str {
    // The header is informational; toml ignores `#` lines anyway, so we
    // can hand the whole string off as-is. Kept as a hook in case future
    // header formats (e.g. a YAML-style fence) need stripping.
    s
}

fn launch_editor(path: &Path) -> Result<()> {
    let raw = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    let mut parts = raw.split_whitespace();
    let program = parts
        .next()
        .with_context(|| "$EDITOR/$VISUAL is set but empty")?;
    let mut cmd = Command::new(program);
    for a in parts {
        cmd.arg(a);
    }
    cmd.arg(path);
    let status = cmd
        .status()
        .with_context(|| format!("failed to launch editor `{program}`"))?;
    if !status.success() {
        bail!("editor `{program}` exited with non-zero status");
    }
    Ok(())
}

fn preserve_tempfile(temp_path: tempfile::TempPath) -> Result<PathBuf> {
    let path: PathBuf = temp_path
        .keep()
        .context("failed to preserve tempfile after edit error")?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::books::EmbedStatus;

    fn sample_book() -> books::Book {
        books::Book {
            id: 7,
            title: "Sapiens".into(),
            author: Some("Yuval Noah Harari".into()),
            format: "epub".into(),
            file_path: "books/7/Yuval_Noah_Harari_-_Sapiens.epub".into(),
            added_at: "2025-01-01".into(),
            description: Some("A brief history".into()),
            series_name: None,
            series_index: None,
            rating: Some(4),
            isbn: Some("9780099590088".into()),
            publisher: Some("Vintage".into()),
            language: Some("en".into()),
            published_date: Some("2014-09-04".into()),
            tags: vec!["history".into(), "nonfiction".into()],
            embed_status: EmbedStatus::Pending,
            embed_synced_at: None,
        }
    }

    #[test]
    fn doc_roundtrip_preserves_fields() {
        let doc = BookMetadataDoc::from_book(&sample_book());
        let s = toml::to_string_pretty(&doc).unwrap();
        let back: BookMetadataDoc = toml::from_str(&s).unwrap();
        assert_eq!(doc, back);
    }

    #[test]
    fn doc_rejects_unknown_fields() {
        let bad = r#"
            title = "x"
            bogus = 1
        "#;
        let err = toml::from_str::<BookMetadataDoc>(bad).unwrap_err();
        assert!(err.to_string().contains("bogus"), "got: {err}");
    }

    #[test]
    fn doc_missing_title_is_error() {
        let bad = r#"author = "X""#;
        let err = toml::from_str::<BookMetadataDoc>(bad).unwrap_err();
        assert!(err.to_string().contains("title"), "got: {err}");
    }

    #[test]
    fn doc_rating_out_of_range_serde_layer() {
        // serde rejects values >255 before our handle_update sees them.
        let bad = r#"
            title = "x"
            rating = 999
        "#;
        let err = toml::from_str::<BookMetadataDoc>(bad).unwrap_err();
        assert!(
            err.to_string().to_ascii_lowercase().contains("rating")
                || err.to_string().to_ascii_lowercase().contains("u8")
                || err.to_string().contains("999")
        );
    }

    #[test]
    fn into_update_normalizes_tags() {
        let doc = BookMetadataDoc {
            title: "x".into(),
            author: None,
            description: None,
            series_name: None,
            series_index: None,
            rating: None,
            isbn: None,
            publisher: None,
            language: None,
            published_date: None,
            tags: vec!["  Sci-Fi ".into(), "sci-fi".into(), "classic".into()],
        };
        let upd = doc.into_update();
        assert_eq!(upd.tags, vec!["Sci-Fi", "classic"]);
    }

    #[test]
    fn render_toml_starts_with_header_and_has_book_id_comment() {
        let doc = BookMetadataDoc::from_book(&sample_book());
        let out = render_toml(&doc, 7).unwrap();
        assert!(out.starts_with("# cdx edit"));
        assert!(out.contains("# book id: 7"));
        assert!(out.contains("title = \"Sapiens\""));
    }
}
