use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::widgets::centered_rect;

#[derive(Debug, Clone, Copy)]
pub struct Binding {
    pub keys: &'static str,
    pub desc: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct Section {
    pub title: &'static str,
    pub bindings: &'static [Binding],
}

pub const GLOBAL: Section = Section {
    title: "Global",
    bindings: &[
        Binding {
            keys: "?",
            desc: "this help",
        },
        Binding {
            keys: ":",
            desc: "command palette",
        },
        Binding {
            keys: "Esc",
            desc: "back / close top layer",
        },
        Binding {
            keys: "q",
            desc: "quit",
        },
        Binding {
            keys: "Ctrl+C",
            desc: "quit",
        },
    ],
};

#[derive(Debug, Default, Clone, Copy)]
pub struct State;

pub enum HelpAction {
    None,
    Close,
}

pub fn handle_key(key: KeyEvent) -> HelpAction {
    match key.code {
        KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => HelpAction::Close,
        _ => HelpAction::None,
    }
}

const KEYS_COL_PAD: usize = 4;
const HORIZ_PAD: usize = 2;
const COLUMN_GAP: usize = 3;
const MAX_COLUMNS: usize = 4;
const TITLE: &str = " help ";

pub fn render(frame: &mut Frame<'_>, area: Rect, sections: &[Section]) {
    let inner_h = area.height.saturating_sub(2) as usize;
    let inner_w = area.width.saturating_sub(4) as usize;

    let columns = pick_layout(sections, inner_w, inner_h);

    let height_lines = columns.iter().map(|c| c.lines.len()).max().unwrap_or(1);
    let total_w: usize = columns.iter().map(|c| c.width).sum::<usize>()
        + COLUMN_GAP * columns.len().saturating_sub(1);

    let height = u16::try_from(height_lines)
        .unwrap_or(u16::MAX)
        .saturating_add(2)
        .min(area.height);
    let width = u16::try_from(total_w.max(TITLE.len() + 4) + 4)
        .unwrap_or(u16::MAX)
        .min(area.width);
    let rect = centered_rect(width, height, area);

    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(TITLE)
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let mut constraints: Vec<Constraint> = Vec::with_capacity(columns.len() * 2);
    for (i, col) in columns.iter().enumerate() {
        if i > 0 {
            constraints.push(Constraint::Length(COLUMN_GAP as u16));
        }
        constraints.push(Constraint::Length(
            u16::try_from(col.width).unwrap_or(u16::MAX),
        ));
    }
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(inner);

    for (i, col) in columns.into_iter().enumerate() {
        let chunk_idx = if i == 0 { 0 } else { i * 2 };
        let Some(chunk) = chunks.get(chunk_idx) else {
            continue;
        };
        frame.render_widget(Paragraph::new(col.lines), *chunk);
    }
}

struct Column {
    lines: Vec<Line<'static>>,
    width: usize,
}

fn pick_layout(sections: &[Section], inner_w: usize, inner_h: usize) -> Vec<Column> {
    let max_cols = sections.len().clamp(1, MAX_COLUMNS);
    let mut fallback = pack_into_columns(sections, max_cols);
    for n in 1..=max_cols {
        let cols = pack_into_columns(sections, n);
        let max_h = cols.iter().map(|c| c.lines.len()).max().unwrap_or(0);
        let total_w: usize =
            cols.iter().map(|c| c.width).sum::<usize>() + COLUMN_GAP * cols.len().saturating_sub(1);
        if max_h <= inner_h && total_w <= inner_w {
            return cols;
        }
        if n == max_cols {
            fallback = cols;
        }
    }
    fallback
}

fn pack_into_columns(sections: &[Section], n_cols: usize) -> Vec<Column> {
    let n_cols = n_cols.clamp(1, sections.len().max(1));

    let groups: Vec<SectionLines> = sections.iter().map(section_lines).collect();

    let total_lines: usize =
        groups.iter().map(|g| g.lines.len()).sum::<usize>() + groups.len().saturating_sub(1);
    let target = total_lines.div_ceil(n_cols);

    let mut buckets: Vec<Vec<SectionLines>> = (0..n_cols).map(|_| Vec::new()).collect();
    let mut heights = vec![0usize; n_cols];
    let mut cur = 0;
    for group in groups {
        let sep = if buckets[cur].is_empty() { 0 } else { 1 };
        let cost = group.lines.len() + sep;
        if cur + 1 < n_cols && heights[cur] > 0 && heights[cur] + cost > target {
            cur += 1;
        }
        let sep = if buckets[cur].is_empty() { 0 } else { 1 };
        heights[cur] += group.lines.len() + sep;
        buckets[cur].push(group);
    }

    buckets
        .into_iter()
        .filter(|b| !b.is_empty())
        .map(flatten_bucket)
        .collect()
}

struct SectionLines {
    lines: Vec<Line<'static>>,
    width: usize,
}

fn section_lines(section: &Section) -> SectionLines {
    let key_col_width = section
        .bindings
        .iter()
        .map(|b| b.keys.chars().count())
        .max()
        .unwrap_or(0)
        + KEYS_COL_PAD;

    let pad = " ".repeat(HORIZ_PAD);
    let mut max_width = HORIZ_PAD + section.title.chars().count();

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(section.bindings.len() + 1);
    lines.push(Line::from(Span::styled(
        format!("{pad}{}", section.title),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));

    for binding in section.bindings {
        let keys_padded = format!("{:<width$}", binding.keys, width = key_col_width);
        let row_w = HORIZ_PAD + keys_padded.chars().count() + binding.desc.chars().count();
        max_width = max_width.max(row_w);
        lines.push(Line::from(vec![
            Span::raw(pad.clone()),
            Span::styled(
                keys_padded,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(binding.desc.to_string(), Style::default().fg(Color::Gray)),
        ]));
    }

    SectionLines {
        lines,
        width: max_width,
    }
}

fn flatten_bucket(bucket: Vec<SectionLines>) -> Column {
    let width = bucket.iter().map(|s| s.width).max().unwrap_or(0);
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (i, section) in bucket.into_iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        lines.extend(section.lines);
    }
    Column { lines, width }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn esc_closes() {
        assert!(matches!(handle_key(key(KeyCode::Esc)), HelpAction::Close));
    }

    #[test]
    fn question_mark_closes() {
        assert!(matches!(
            handle_key(key(KeyCode::Char('?'))),
            HelpAction::Close
        ));
    }

    #[test]
    fn q_closes() {
        assert!(matches!(
            handle_key(key(KeyCode::Char('q'))),
            HelpAction::Close
        ));
    }

    #[test]
    fn other_keys_do_nothing() {
        assert!(matches!(handle_key(key(KeyCode::Enter)), HelpAction::None));
        assert!(matches!(
            handle_key(key(KeyCode::Char('e'))),
            HelpAction::None
        ));
        assert!(matches!(
            handle_key(key(KeyCode::Char(':'))),
            HelpAction::None
        ));
    }

    #[test]
    fn single_column_when_height_fits() {
        let demo = Section {
            title: "Demo",
            bindings: &[Binding {
                keys: "x",
                desc: "do x",
            }],
        };
        let cols = pick_layout(&[GLOBAL, demo], 200, 200);
        assert_eq!(cols.len(), 1);
        // 5 bindings + title in GLOBAL = 6, blank separator = 1, demo title = 1, demo binding = 1 -> 9 lines
        assert_eq!(cols[0].lines.len(), 9);
    }

    #[test]
    fn multi_column_when_height_too_small() {
        let demo = Section {
            title: "Demo",
            bindings: &[Binding {
                keys: "x",
                desc: "do x",
            }],
        };
        // GLOBAL alone is 6 lines (title + 5 bindings). With inner_h = 5,
        // single column overflows so a second column is needed.
        let cols = pick_layout(&[GLOBAL, demo], 200, 5);
        assert!(cols.len() >= 2, "expected multi-column when height < 6");
    }

    #[test]
    fn falls_back_to_max_columns_when_nothing_fits() {
        // Many short sections; very small inner_h forces packing to MAX_COLUMNS.
        let bindings: &[Binding] = &[Binding {
            keys: "a",
            desc: "b",
        }];
        let sections: Vec<Section> = (0..MAX_COLUMNS + 2)
            .map(|_| Section {
                title: "S",
                bindings,
            })
            .collect();
        let cols = pick_layout(&sections, 500, 1);
        assert!(cols.len() <= MAX_COLUMNS);
        assert!(!cols.is_empty());
    }

    #[test]
    fn pack_preserves_all_section_titles() {
        let demo = Section {
            title: "Demo",
            bindings: &[Binding {
                keys: "x",
                desc: "do x",
            }],
        };
        let cols = pack_into_columns(&[GLOBAL, demo], 2);
        let collected: String = cols
            .iter()
            .flat_map(|c| c.lines.iter())
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect::<Vec<_>>()
            .join("");
        assert!(collected.contains("Global"));
        assert!(collected.contains("Demo"));
    }

    #[test]
    fn section_lines_align_keys_within_section() {
        let section = Section {
            title: "Demo",
            bindings: &[
                Binding {
                    keys: "x",
                    desc: "short",
                },
                Binding {
                    keys: "Ctrl+W",
                    desc: "wide",
                },
            ],
        };
        let sl = section_lines(&section);
        // skip title (index 0); both binding rows must have equal-width keys span.
        let row_x = &sl.lines[1];
        let row_ctrl = &sl.lines[2];
        let x_keys = row_x.spans[1].content.as_ref();
        let ctrl_keys = row_ctrl.spans[1].content.as_ref();
        assert_eq!(x_keys.chars().count(), ctrl_keys.chars().count());
        assert!(x_keys.starts_with('x'));
        assert!(ctrl_keys.starts_with("Ctrl+W"));
    }
}
