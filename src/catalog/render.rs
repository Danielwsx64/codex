use std::io::{self, Write};

use serde::Serialize;
use tabwriter::TabWriter;

use crate::catalog::handlers::{AddOutcome, CatalogRow, InitOutcome, RmOutcome, UseOutcome};

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
}
