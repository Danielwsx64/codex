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
const TITLE: &str = " help ";

pub fn render(frame: &mut Frame<'_>, area: Rect, sections: &[Section]) {
    let lines = build_lines(sections);

    let max_text_width = lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.chars().count())
                .sum::<usize>()
        })
        .max()
        .unwrap_or(20);

    let height = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2);
    let width = u16::try_from(max_text_width.max(TITLE.len() + 4) + 4).unwrap_or(u16::MAX);
    let rect = centered_rect(width.min(area.width), height.min(area.height), area);

    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(TITLE)
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1)])
        .split(inner);
    frame.render_widget(Paragraph::new(lines), layout[0]);
}

fn build_lines(sections: &[Section]) -> Vec<Line<'static>> {
    let key_col_width = sections
        .iter()
        .flat_map(|s| s.bindings.iter())
        .map(|b| b.keys.chars().count())
        .max()
        .unwrap_or(0)
        + KEYS_COL_PAD;

    let pad = " ".repeat(HORIZ_PAD);
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (idx, section) in sections.iter().enumerate() {
        if idx > 0 {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            format!("{pad}{}", section.title),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
        for binding in section.bindings {
            let keys_padded = format!("{:<width$}", binding.keys, width = key_col_width);
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
    }
    lines
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
    fn build_lines_emits_global_section_plus_blank_separator() {
        let lines = build_lines(&[
            GLOBAL,
            Section {
                title: "Demo",
                bindings: &[Binding {
                    keys: "x",
                    desc: "do x",
                }],
            },
        ]);
        // 5 bindings in GLOBAL + 1 title + blank + 1 title + 1 binding = 9.
        assert_eq!(lines.len(), 1 + GLOBAL.bindings.len() + 1 + 1 + 1);
        // First line is the global title text.
        let first_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(first_text.contains("Global"));
    }

    #[test]
    fn build_lines_aligns_keys_column_by_widest_keys() {
        let lines = build_lines(&[Section {
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
        }]);
        // Find the two binding rows (skip title).
        let row_x = &lines[1];
        let row_ctrl = &lines[2];
        let x_keys = row_x.spans[1].content.as_ref();
        let ctrl_keys = row_ctrl.spans[1].content.as_ref();
        assert_eq!(x_keys.chars().count(), ctrl_keys.chars().count());
        assert!(x_keys.starts_with('x'));
        assert!(ctrl_keys.starts_with("Ctrl+W"));
    }
}
