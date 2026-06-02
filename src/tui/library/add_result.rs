use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::catalog::books::{AddOutcome, AddStatus};
use crate::tui::widgets::centered_rect;

// Summary modal shown after an `add` when something needs the user's attention
// (a skipped duplicate or a failure). A clean all-imported run keeps the lighter
// status-line path instead of forcing a dismiss.
#[derive(Debug)]
pub struct State {
    imported: Vec<String>,
    duplicates: Vec<String>,
    failed: Vec<String>,
    scroll: u16,
    content_lines: u16,
}

impl State {
    pub fn from_outcome(outcome: &AddOutcome) -> Self {
        let mut imported = Vec::new();
        let mut duplicates = Vec::new();
        let mut failed = Vec::new();
        for row in &outcome.rows {
            let name = row
                .source
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string();
            match &row.status {
                AddStatus::Imported => imported.push(match row.book_id {
                    Some(id) => format!("{name}  → id {id}"),
                    None => name,
                }),
                AddStatus::Duplicate { existing_id } => {
                    duplicates.push(format!("{name}  → already book #{existing_id}"))
                }
                AddStatus::Failed { reason } => failed.push(format!("{name}  — {reason}")),
            }
        }
        let mut state = Self {
            imported,
            duplicates,
            failed,
            scroll: 0,
            content_lines: 0,
        };
        state.content_lines = state.lines().len() as u16;
        state
    }

    // Whether anything happened that the user should explicitly acknowledge.
    pub fn is_noteworthy(&self) -> bool {
        !self.duplicates.is_empty() || !self.failed.is_empty()
    }

    pub fn scroll_down(&mut self) {
        let max = self.content_lines.saturating_sub(1);
        self.scroll = (self.scroll + 1).min(max);
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    fn lines(&self) -> Vec<Line<'_>> {
        let mut lines: Vec<Line<'_>> = Vec::new();

        let summary = format!(
            "{} imported · {} skipped · {} failed",
            self.imported.len(),
            self.duplicates.len(),
            self.failed.len()
        );
        lines.push(Line::from(Span::styled(
            summary,
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));

        push_section(&mut lines, "Imported", Color::Green, &self.imported);
        lines.push(Line::from(""));
        push_section(
            &mut lines,
            "Skipped (duplicates)",
            Color::Yellow,
            &self.duplicates,
        );
        if !self.failed.is_empty() {
            lines.push(Line::from(""));
            push_section(&mut lines, "Failed", Color::Red, &self.failed);
        }

        lines
    }
}

fn push_section<'a>(
    lines: &mut Vec<Line<'a>>,
    label: &'a str,
    color: Color,
    entries: &'a [String],
) {
    lines.push(Line::from(Span::styled(
        format!("{label} ({})", entries.len()),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )));
    if entries.is_empty() {
        lines.push(Line::from(Span::styled(
            "  none",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for entry in entries {
            lines.push(Line::from(Span::raw(format!("  · {entry}"))));
        }
    }
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let target_w = area.width.saturating_mul(4) / 5;
    let target_h = area.height.saturating_mul(4) / 5;
    let w = target_w.max(50).min(area.width);
    let h = target_h.max(12).min(area.height);
    let rect = centered_rect(w, h, area);
    frame.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" add: done ")
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let body = Paragraph::new(state.lines()).scroll((state.scroll, 0));
    frame.render_widget(body, layout[0]);

    let scrollable = state.content_lines > layout[0].height;
    let hint = if scrollable {
        "↑/↓ scroll · Esc/Enter close"
    } else {
        "Esc/Enter close"
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        ))),
        layout[1],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::books::AddRow;
    use std::path::PathBuf;

    fn row(name: &str, status: AddStatus) -> AddRow {
        AddRow {
            source: PathBuf::from(name),
            status,
            book_id: None,
            stored_path: None,
        }
    }

    #[test]
    fn noteworthy_only_when_duplicate_or_failure() {
        let clean = AddOutcome {
            rows: vec![row("a.epub", AddStatus::Imported)],
        };
        assert!(!State::from_outcome(&clean).is_noteworthy());

        let dup = AddOutcome {
            rows: vec![row("a.epub", AddStatus::Duplicate { existing_id: 1 })],
        };
        assert!(State::from_outcome(&dup).is_noteworthy());

        let fail = AddOutcome {
            rows: vec![row("a.epub", AddStatus::Failed { reason: "x".into() })],
        };
        assert!(State::from_outcome(&fail).is_noteworthy());
    }

    #[test]
    fn buckets_rows_by_status() {
        let outcome = AddOutcome {
            rows: vec![
                row("ok.epub", AddStatus::Imported),
                row("dup.epub", AddStatus::Duplicate { existing_id: 2 }),
                row(
                    "bad.epub",
                    AddStatus::Failed {
                        reason: "boom".into(),
                    },
                ),
            ],
        };
        let state = State::from_outcome(&outcome);
        assert_eq!(state.imported.len(), 1);
        assert_eq!(state.duplicates.len(), 1);
        assert_eq!(state.failed.len(), 1);
        assert!(state.content_lines > 0);
    }

    #[test]
    fn scroll_is_clamped_to_content() {
        let outcome = AddOutcome {
            rows: vec![row("dup.epub", AddStatus::Duplicate { existing_id: 1 })],
        };
        let mut state = State::from_outcome(&outcome);
        for _ in 0..100 {
            state.scroll_down();
        }
        assert!(state.scroll <= state.content_lines.saturating_sub(1));
        state.scroll_up();
        for _ in 0..100 {
            state.scroll_up();
        }
        assert_eq!(state.scroll, 0);
    }
}
