use std::io::{self, StdoutLock, Write};
use std::path::Path;

use serde::Serialize;
use tabwriter::TabWriter;

use crate::catalog::books::{
    AddOutcome as BookAddOutcome, AddStatus, Book, RateOutcome, RmOutcome as BookRmOutcome,
    SeriesOutcome, TagOpOutcome,
};
use crate::catalog::columns::LibraryColumn;
use crate::catalog::handlers::{AddOutcome, CatalogRow, InitOutcome, RmOutcome, UseOutcome};
use crate::device::DeviceRow;

// Single funnel used by every dispatcher that picks between a human and a
// JSONL renderer. Locks stdout, runs the matching closure, flushes.
pub fn emit<H, J>(json: bool, human: H, jsonl: J) -> io::Result<()>
where
    H: FnOnce(&mut StdoutLock<'_>) -> io::Result<()>,
    J: FnOnce(&mut StdoutLock<'_>) -> io::Result<()>,
{
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if json {
        jsonl(&mut out)?;
    } else {
        human(&mut out)?;
    }
    out.flush()
}

#[derive(Serialize)]
struct LsJson<'a> {
    name: &'a str,
    path: &'a std::path::Path,
    description: Option<&'a str>,
    current: bool,
    missing: bool,
}

pub fn render_ls_human<W: Write>(rows: &[CatalogRow], w: &mut W) -> io::Result<()> {
    if rows.is_empty() {
        writeln!(
            w,
            "No catalogs registered. Run `cdx catalog init <name> <path>`."
        )?;
        return Ok(());
    }
    let mut tw = TabWriter::new(w).padding(2);
    writeln!(&mut tw, "CURRENT\tNAME\tPATH\tDESCRIPTION\tSTATUS")?;
    for row in rows {
        let current = if row.current { "*" } else { " " };
        let status = if row.missing { "(missing)" } else { "ok" };
        let desc = row.description.as_deref().unwrap_or("");
        writeln!(
            &mut tw,
            "{current}\t{name}\t{path}\t{desc}\t{status}",
            name = row.name,
            path = row.path.display(),
        )?;
    }
    tw.flush()?;
    Ok(())
}

pub fn render_ls_jsonl<W: Write>(rows: &[CatalogRow], w: &mut W) -> io::Result<()> {
    for row in rows {
        let value = LsJson {
            name: &row.name,
            path: &row.path,
            description: row.description.as_deref(),
            current: row.current,
            missing: row.missing,
        };
        serde_json::to_writer(&mut *w, &value)?;
        writeln!(w)?;
    }
    Ok(())
}

#[derive(Serialize)]
struct ActionJson<'a> {
    action: &'a str,
    name: &'a str,
    path: &'a std::path::Path,
    #[serde(skip_serializing_if = "Option::is_none")]
    became_current: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    purged: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cleared_current: Option<bool>,
}

pub fn render_init_human<W: Write>(outcome: &InitOutcome, w: &mut W) -> io::Result<()> {
    writeln!(
        w,
        "Initialized catalog `{}` at {}{}",
        outcome.name,
        outcome.path.display(),
        if outcome.became_current {
            " (now current)"
        } else {
            ""
        }
    )
}

pub fn render_init_jsonl<W: Write>(outcome: &InitOutcome, w: &mut W) -> io::Result<()> {
    let value = ActionJson {
        action: "init",
        name: &outcome.name,
        path: &outcome.path,
        became_current: Some(outcome.became_current),
        purged: None,
        cleared_current: None,
    };
    serde_json::to_writer(&mut *w, &value)?;
    writeln!(w)
}

pub fn render_add_human<W: Write>(outcome: &AddOutcome, w: &mut W) -> io::Result<()> {
    writeln!(
        w,
        "Registered catalog `{}` at {}{}",
        outcome.name,
        outcome.path.display(),
        if outcome.became_current {
            " (now current)"
        } else {
            ""
        }
    )
}

pub fn render_add_jsonl<W: Write>(outcome: &AddOutcome, w: &mut W) -> io::Result<()> {
    let value = ActionJson {
        action: "add",
        name: &outcome.name,
        path: &outcome.path,
        became_current: Some(outcome.became_current),
        purged: None,
        cleared_current: None,
    };
    serde_json::to_writer(&mut *w, &value)?;
    writeln!(w)
}

pub fn render_use_human<W: Write>(outcome: &UseOutcome, w: &mut W) -> io::Result<()> {
    writeln!(
        w,
        "Switched to catalog `{}` ({})",
        outcome.name,
        outcome.path.display()
    )
}

pub fn render_use_jsonl<W: Write>(outcome: &UseOutcome, w: &mut W) -> io::Result<()> {
    let value = ActionJson {
        action: "use",
        name: &outcome.name,
        path: &outcome.path,
        became_current: None,
        purged: None,
        cleared_current: None,
    };
    serde_json::to_writer(&mut *w, &value)?;
    writeln!(w)
}

pub fn render_rm_human<W: Write>(outcome: &RmOutcome, w: &mut W) -> io::Result<()> {
    let detail = match (outcome.purged, outcome.cleared_current) {
        (true, true) => " (purged from disk; current cleared)",
        (true, false) => " (purged from disk)",
        (false, true) => " (current cleared)",
        (false, false) => "",
    };
    writeln!(
        w,
        "Removed catalog `{}` ({}){}",
        outcome.name,
        outcome.path.display(),
        detail
    )
}

pub fn render_rm_jsonl<W: Write>(outcome: &RmOutcome, w: &mut W) -> io::Result<()> {
    let value = ActionJson {
        action: "rm",
        name: &outcome.name,
        path: &outcome.path,
        became_current: None,
        purged: Some(outcome.purged),
        cleared_current: Some(outcome.cleared_current),
    };
    serde_json::to_writer(&mut *w, &value)?;
    writeln!(w)
}

// --- Book renderers -----------------------------------------------------

#[derive(Serialize)]
struct BookAddJson<'a> {
    source: &'a Path,
    status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    existing_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stored_path: Option<&'a Path>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
}

pub fn render_book_add_human<W: Write>(outcome: &BookAddOutcome, w: &mut W) -> io::Result<()> {
    if outcome.rows.is_empty() {
        return Ok(());
    }
    for row in &outcome.rows {
        match &row.status {
            AddStatus::Imported => {
                let id = row.book_id.unwrap_or(0);
                let stored = row
                    .stored_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                writeln!(
                    w,
                    "Imported `{src}` as id {id} ({stored})",
                    src = row.source.display(),
                )?;
            }
            AddStatus::Duplicate { existing_id } => {
                writeln!(
                    w,
                    "Skipped `{src}`: duplicate of book #{existing_id} (use --force to import anyway)",
                    src = row.source.display(),
                )?;
            }
            AddStatus::Failed { reason } => {
                writeln!(
                    w,
                    "Failed to import `{src}`: {reason}",
                    src = row.source.display(),
                )?;
            }
        }
    }
    Ok(())
}

pub fn render_book_add_jsonl<W: Write>(outcome: &BookAddOutcome, w: &mut W) -> io::Result<()> {
    for row in &outcome.rows {
        let mut existing_id = None;
        let (status, reason) = match &row.status {
            AddStatus::Imported => ("imported", None),
            AddStatus::Duplicate { existing_id: id } => {
                existing_id = Some(*id);
                ("duplicate", None)
            }
            AddStatus::Failed { reason } => ("failed", Some(reason.as_str())),
        };
        let value = BookAddJson {
            source: &row.source,
            status,
            id: row.book_id,
            existing_id,
            stored_path: row.stored_path.as_deref(),
            reason,
        };
        serde_json::to_writer(&mut *w, &value)?;
        writeln!(w)?;
    }
    Ok(())
}

pub fn render_book_ls_human<W: Write>(
    rows: &[Book],
    columns: &[LibraryColumn],
    w: &mut W,
) -> io::Result<()> {
    if rows.is_empty() {
        writeln!(w, "No books in the current catalog. Try `cdx add <file>`.")?;
        return Ok(());
    }
    let mut tw = TabWriter::new(w).padding(2);
    let header = columns
        .iter()
        .map(|c| c.slug().to_ascii_uppercase())
        .collect::<Vec<_>>()
        .join("\t");
    writeln!(&mut tw, "{header}")?;
    for b in rows {
        let row = columns
            .iter()
            .map(|c| c.render(b))
            .collect::<Vec<_>>()
            .join("\t");
        writeln!(&mut tw, "{row}")?;
    }
    tw.flush()
}

pub fn render_book_ls_jsonl<W: Write>(
    rows: &[Book],
    columns: &[LibraryColumn],
    w: &mut W,
) -> io::Result<()> {
    for b in rows {
        let mut obj = serde_json::Map::new();
        for c in columns {
            obj.insert(c.slug().to_string(), c.json_value(b));
        }
        serde_json::to_writer(&mut *w, &serde_json::Value::Object(obj))?;
        writeln!(w)?;
    }
    Ok(())
}

#[derive(Serialize)]
struct BookInspectJson<'a> {
    id: i64,
    title: &'a str,
    author: Option<&'a str>,
    format: &'a str,
    file_path: &'a Path,
    added_at: &'a str,
    tags: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    series_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    series_index: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rating: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    publisher: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    published_date: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    isbn: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
}

pub fn render_book_inspect_human<W: Write>(
    book: &Book,
    absolute_path: &Path,
    w: &mut W,
) -> io::Result<()> {
    let mut tw = TabWriter::new(w).padding(2);
    writeln!(&mut tw, "id\t{id}", id = book.id)?;
    writeln!(&mut tw, "title\t{}", book.title)?;
    writeln!(
        &mut tw,
        "author\t{}",
        book.author.as_deref().unwrap_or("(unknown)")
    )?;
    writeln!(&mut tw, "format\t{}", book.format)?;
    writeln!(&mut tw, "file\t{}", absolute_path.display())?;
    writeln!(&mut tw, "added\t{}", book.added_at)?;
    if !book.tags.is_empty() {
        writeln!(&mut tw, "tags\t{}", book.tags.join(", "))?;
    }
    if let Some(s) = &book.series_name {
        let line = match book.series_index {
            Some(idx) => format!("{s} #{}", format_index(idx)),
            None => s.clone(),
        };
        writeln!(&mut tw, "series\t{line}")?;
    }
    if let Some(r) = book.rating {
        writeln!(&mut tw, "rating\t{r}/5")?;
    }
    if let Some(p) = &book.publisher {
        writeln!(&mut tw, "publisher\t{p}")?;
    }
    if let Some(l) = &book.language {
        writeln!(&mut tw, "language\t{l}")?;
    }
    if let Some(d) = &book.published_date {
        writeln!(&mut tw, "published\t{d}")?;
    }
    if let Some(i) = &book.isbn {
        writeln!(&mut tw, "isbn\t{i}")?;
    }
    if let Some(d) = &book.description {
        writeln!(&mut tw, "description\t{d}")?;
    }
    tw.flush()
}

pub fn render_book_inspect_jsonl<W: Write>(
    book: &Book,
    absolute_path: &Path,
    w: &mut W,
) -> io::Result<()> {
    let value = BookInspectJson {
        id: book.id,
        title: &book.title,
        author: book.author.as_deref(),
        format: &book.format,
        file_path: absolute_path,
        added_at: &book.added_at,
        tags: &book.tags,
        series_name: book.series_name.as_deref(),
        series_index: book.series_index,
        rating: book.rating,
        publisher: book.publisher.as_deref(),
        language: book.language.as_deref(),
        published_date: book.published_date.as_deref(),
        isbn: book.isbn.as_deref(),
        description: book.description.as_deref(),
    };
    serde_json::to_writer(&mut *w, &value)?;
    writeln!(w)
}

fn format_index(idx: f64) -> String {
    if idx.fract() == 0.0 {
        format!("{}", idx as i64)
    } else {
        format!("{idx}")
    }
}

#[derive(Serialize)]
struct BookRmJson<'a> {
    action: &'a str,
    id: i64,
    title: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    kept_at: Option<&'a Path>,
}

pub fn render_book_rm_human<W: Write>(outcome: &BookRmOutcome, w: &mut W) -> io::Result<()> {
    match &outcome.kept_at {
        Some(path) => writeln!(
            w,
            "Removed book id {id} (`{title}`); file kept at {p}",
            id = outcome.book.id,
            title = outcome.book.title,
            p = path.display(),
        ),
        None => writeln!(
            w,
            "Removed book id {id} (`{title}`) and deleted its file",
            id = outcome.book.id,
            title = outcome.book.title,
        ),
    }
}

#[derive(Serialize)]
struct TagJson<'a> {
    action: &'a str,
    id: i64,
    title: &'a str,
    added: &'a [String],
    already_present: &'a [String],
}

#[derive(Serialize)]
struct UntagJson<'a> {
    action: &'a str,
    id: i64,
    title: &'a str,
    removed: &'a [String],
    not_present: &'a [String],
}

pub fn render_tag_human<W: Write>(op: &TagOpOutcome, w: &mut W) -> io::Result<()> {
    if op.changed.is_empty() && op.unchanged.is_empty() {
        writeln!(w, "Book {} ({}): no tags given", op.book.id, op.book.title)?;
        return Ok(());
    }
    if op.changed.is_empty() {
        writeln!(
            w,
            "Book {} ({}): no changes (already present: {})",
            op.book.id,
            op.book.title,
            op.unchanged.join(", "),
        )?;
        return Ok(());
    }
    let plus = op
        .changed
        .iter()
        .map(|s| format!("+{s}"))
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(w, "Tagged book {} ({}): {plus}", op.book.id, op.book.title)?;
    if !op.unchanged.is_empty() {
        writeln!(w, "  already present: {}", op.unchanged.join(", "))?;
    }
    Ok(())
}

pub fn render_tag_jsonl<W: Write>(op: &TagOpOutcome, w: &mut W) -> io::Result<()> {
    let value = TagJson {
        action: "tag",
        id: op.book.id,
        title: &op.book.title,
        added: &op.changed,
        already_present: &op.unchanged,
    };
    serde_json::to_writer(&mut *w, &value)?;
    writeln!(w)
}

pub fn render_untag_human<W: Write>(op: &TagOpOutcome, w: &mut W) -> io::Result<()> {
    if op.changed.is_empty() && op.unchanged.is_empty() {
        writeln!(
            w,
            "Book {} ({}): no tags to remove",
            op.book.id, op.book.title
        )?;
        return Ok(());
    }
    if op.changed.is_empty() {
        writeln!(
            w,
            "Book {} ({}): no changes (not present: {})",
            op.book.id,
            op.book.title,
            op.unchanged.join(", "),
        )?;
        return Ok(());
    }
    let minus = op
        .changed
        .iter()
        .map(|s| format!("-{s}"))
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(
        w,
        "Untagged book {} ({}): {minus}",
        op.book.id, op.book.title
    )?;
    if !op.unchanged.is_empty() {
        writeln!(w, "  not present: {}", op.unchanged.join(", "))?;
    }
    Ok(())
}

pub fn render_untag_jsonl<W: Write>(op: &TagOpOutcome, w: &mut W) -> io::Result<()> {
    let value = UntagJson {
        action: "untag",
        id: op.book.id,
        title: &op.book.title,
        removed: &op.changed,
        not_present: &op.unchanged,
    };
    serde_json::to_writer(&mut *w, &value)?;
    writeln!(w)
}

#[derive(Serialize)]
struct RateJson<'a> {
    action: &'a str,
    id: i64,
    title: &'a str,
    rating: Option<u8>,
    previous_rating: Option<u8>,
    changed: bool,
    embed_status: &'a str,
}

#[derive(Serialize)]
struct SeriesJson<'a> {
    action: &'a str,
    id: i64,
    title: &'a str,
    series_name: Option<&'a str>,
    series_index: Option<f64>,
    previous_series_name: Option<&'a str>,
    previous_series_index: Option<f64>,
    changed: bool,
    embed_status: &'a str,
}

fn stars(rating: Option<u8>) -> String {
    match rating {
        None => "unrated".to_string(),
        Some(r) => {
            let filled = r.min(5) as usize;
            let mut s = String::with_capacity(5);
            for _ in 0..filled {
                s.push('★');
            }
            for _ in filled..5 {
                s.push('☆');
            }
            s
        }
    }
}

pub fn render_rate_human<W: Write>(op: &RateOutcome, w: &mut W) -> io::Result<()> {
    let id = op.book.id;
    let title = &op.book.title;
    let now = stars(op.book.rating);
    let was = stars(op.previous_rating);
    if !op.changed {
        writeln!(w, "Book {id} ({title}): rating unchanged ({now})")?;
        return Ok(());
    }
    match op.book.rating {
        None => writeln!(w, "Cleared rating for book {id} ({title}) (was {was})"),
        Some(_) => writeln!(w, "Rated book {id} ({title}): {now} (was {was})"),
    }
}

pub fn render_rate_jsonl<W: Write>(op: &RateOutcome, w: &mut W) -> io::Result<()> {
    let value = RateJson {
        action: "rate",
        id: op.book.id,
        title: &op.book.title,
        rating: op.book.rating,
        previous_rating: op.previous_rating,
        changed: op.changed,
        embed_status: op.book.embed_status.as_str(),
    };
    serde_json::to_writer(&mut *w, &value)?;
    writeln!(w)
}

fn series_label(name: Option<&str>, index: Option<f64>) -> String {
    match (name, index) {
        (None, _) => "none".to_string(),
        (Some(n), None) => n.to_string(),
        (Some(n), Some(i)) => format!("{n} #{i}"),
    }
}

pub fn render_series_human<W: Write>(op: &SeriesOutcome, w: &mut W) -> io::Result<()> {
    let id = op.book.id;
    let title = &op.book.title;
    let now = series_label(op.book.series_name.as_deref(), op.book.series_index);
    let was = series_label(op.previous_name.as_deref(), op.previous_index);
    if !op.changed {
        writeln!(w, "Book {id} ({title}): series unchanged ({now})")?;
        return Ok(());
    }
    match op.book.series_name {
        None => writeln!(w, "Cleared series for book {id} ({title}) (was: {was})"),
        Some(_) => writeln!(w, "Set series for book {id} ({title}): {now} (was: {was})"),
    }
}

pub fn render_series_jsonl<W: Write>(op: &SeriesOutcome, w: &mut W) -> io::Result<()> {
    let value = SeriesJson {
        action: "series",
        id: op.book.id,
        title: &op.book.title,
        series_name: op.book.series_name.as_deref(),
        series_index: op.book.series_index,
        previous_series_name: op.previous_name.as_deref(),
        previous_series_index: op.previous_index,
        changed: op.changed,
        embed_status: op.book.embed_status.as_str(),
    };
    serde_json::to_writer(&mut *w, &value)?;
    writeln!(w)
}

pub fn render_book_rm_jsonl<W: Write>(outcome: &BookRmOutcome, w: &mut W) -> io::Result<()> {
    let value = BookRmJson {
        action: "book_rm",
        id: outcome.book.id,
        title: &outcome.book.title,
        kept_at: outcome.kept_at.as_deref(),
    };
    serde_json::to_writer(&mut *w, &value)?;
    writeln!(w)
}

#[derive(Serialize)]
struct DeviceLsJson<'a> {
    alias: Option<&'a str>,
    serial: &'a str,
    connected: bool,
    mount_path: Option<&'a Path>,
    free_bytes: Option<u64>,
    book_count: Option<usize>,
    last_seen_at: &'a str,
}

pub fn render_device_ls_human<W: Write>(rows: &[DeviceRow], w: &mut W) -> io::Result<()> {
    if rows.is_empty() {
        writeln!(
            w,
            "No devices known. Connect a Kindle over USB and run `cdx device ls`."
        )?;
        return Ok(());
    }
    let mut tw = TabWriter::new(w).padding(2);
    writeln!(
        &mut tw,
        "ALIAS\tSERIAL\tCONNECTED\tMOUNT\tFREE\tBOOKS\tLAST SEEN"
    )?;
    for row in rows {
        let alias = row.alias.as_deref().unwrap_or(&row.serial);
        let connected = if row.connected { "yes" } else { "no" };
        let mount = row
            .mount_path
            .as_deref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "-".to_string());
        let free = row
            .free_bytes
            .map(format_bytes)
            .unwrap_or_else(|| "-".to_string());
        let books = row
            .book_count
            .map(|n| n.to_string())
            .unwrap_or_else(|| "-".to_string());
        writeln!(
            &mut tw,
            "{alias}\t{serial}\t{connected}\t{mount}\t{free}\t{books}\t{last_seen}",
            serial = row.serial,
            last_seen = row.last_seen_at,
        )?;
    }
    tw.flush()?;
    Ok(())
}

pub fn render_device_ls_jsonl<W: Write>(rows: &[DeviceRow], w: &mut W) -> io::Result<()> {
    for row in rows {
        let value = DeviceLsJson {
            alias: row.alias.as_deref(),
            serial: &row.serial,
            connected: row.connected,
            mount_path: row.mount_path.as_deref(),
            free_bytes: row.free_bytes,
            book_count: row.book_count,
            last_seen_at: &row.last_seen_at,
        };
        serde_json::to_writer(&mut *w, &value)?;
        writeln!(w)?;
    }
    Ok(())
}

// Binary units (KiB/MiB/GiB) so the column stays narrow on devices with tens
// of gigabytes free. Bytes under 1 KiB print as a raw count.
fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn row(name: &str, path: &str, current: bool, missing: bool) -> CatalogRow {
        CatalogRow {
            name: name.to_string(),
            path: PathBuf::from(path),
            description: None,
            current,
            missing,
        }
    }

    #[test]
    fn ls_jsonl_empty_emits_nothing() {
        let mut buf = Vec::new();
        render_ls_jsonl(&[], &mut buf).unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn ls_jsonl_one_object_per_line() {
        let rows = vec![row("a", "/p1", true, false), row("b", "/p2", false, true)];
        let mut buf = Vec::new();
        render_ls_jsonl(&rows, &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        for line in lines {
            let _v: serde_json::Value =
                serde_json::from_str(line).expect("each line is valid JSON");
        }
    }

    #[test]
    fn ls_human_includes_header_and_marker() {
        let rows = vec![row("main", "/lib", true, false)];
        let mut buf = Vec::new();
        render_ls_human(&rows, &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("NAME"));
        assert!(text.contains("PATH"));
        assert!(text.contains("main"));
        assert!(text.contains('*'));
    }

    #[test]
    fn ls_human_marks_missing() {
        let rows = vec![row("gone", "/lib", false, true)];
        let mut buf = Vec::new();
        render_ls_human(&rows, &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("(missing)"));
    }

    #[test]
    fn ls_human_empty_prints_hint() {
        let mut buf = Vec::new();
        render_ls_human(&[], &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("catalog init"));
    }

    fn device_row(serial: &str, alias: Option<&str>, connected: bool) -> DeviceRow {
        DeviceRow {
            alias: alias.map(str::to_string),
            serial: serial.to_string(),
            connected,
            mount_path: connected.then(|| PathBuf::from("/media/user/Kindle")),
            free_bytes: connected.then_some(2_147_483_648),
            book_count: connected.then_some(42),
            last_seen_at: "2026-06-08 12:00:00".to_string(),
        }
    }

    #[test]
    fn device_ls_human_empty_prints_hint() {
        let mut buf = Vec::new();
        render_device_ls_human(&[], &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("No devices"));
    }

    #[test]
    fn device_ls_human_falls_back_to_serial_and_dashes() {
        let rows = vec![device_row("SERIAL_X", None, false)];
        let mut buf = Vec::new();
        render_device_ls_human(&rows, &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("SERIAL_X"));
        assert!(text.contains("no"));
        // disconnected columns collapse to "-"
        assert!(text.contains('-'));
    }

    #[test]
    fn device_ls_jsonl_empty_emits_nothing() {
        let mut buf = Vec::new();
        render_device_ls_jsonl(&[], &mut buf).unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn device_ls_jsonl_shape_for_connected_device() {
        let rows = vec![device_row("SERIAL_C", Some("paperwhite"), true)];
        let mut buf = Vec::new();
        render_device_ls_jsonl(&rows, &mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.len(), 1);
        let v: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(v["serial"], "SERIAL_C");
        assert_eq!(v["alias"], "paperwhite");
        assert_eq!(v["connected"], true);
        assert_eq!(v["book_count"], 42);
        assert_eq!(v["free_bytes"], 2_147_483_648u64);
    }

    #[test]
    fn format_bytes_uses_binary_units() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2_147_483_648), "2.0 GiB");
    }
}
