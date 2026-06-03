use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_segmentation::UnicodeSegmentation;

use crate::catalog;
use crate::catalog::books::{Book as CatalogBook, ReadingProgress};
use crate::config::ReaderSettings;
use crate::reader as domain;
use crate::reader::layout::{lay_out, page_index_for_line, page_ranges, LaidOut, PageRange};
use crate::reader::Book;
use crate::tui::help::{Binding, Section as HelpSection};
use crate::tui::widgets::StatusMessage;

// html2text width hint. We pass a large value so paragraphs come back as
// near-single-line entries, then `layout::lay_out` reflows to the actual
// viewport width. This avoids reloading the book on every terminal resize.
pub const HTML_RENDER_WIDTH: usize = 10_000;

pub fn open_book(
    catalog_dir: PathBuf,
    book: &CatalogBook,
    settings: ReaderSettings,
) -> Result<State, domain::Error> {
    let loaded = domain::open(&catalog_dir, book, HTML_RENDER_WIDTH)?;
    Ok(State::open(catalog_dir, book.id, loaded, settings))
}

const READER_BINDINGS: &[Binding] = &[
    Binding {
        keys: "h j k l",
        desc: "move cursor (char/line)",
    },
    Binding {
        keys: "w b e",
        desc: "word forward/back/end",
    },
    Binding {
        keys: "0 $",
        desc: "line start/end",
    },
    Binding {
        keys: "gg / G",
        desc: "first / last page of book",
    },
    Binding {
        keys: "H / L",
        desc: "top / bottom of current page",
    },
    Binding {
        keys: "Space / Ctrl+f",
        desc: "page down",
    },
    Binding {
        keys: "Ctrl+b",
        desc: "page up",
    },
    Binding {
        keys: "Ctrl+d / Ctrl+u",
        desc: "half page down / up",
    },
    Binding {
        keys: "] / [",
        desc: "next / prev chapter",
    },
    Binding {
        keys: ":N",
        desc: "go to page N (absolute)",
    },
    Binding {
        keys: ":cN",
        desc: "go to chapter N",
    },
    Binding {
        keys: "Esc",
        desc: "back to library",
    },
];

pub fn help_sections(_state: &State) -> Vec<HelpSection> {
    vec![HelpSection {
        title: "Reader",
        bindings: READER_BINDINGS,
    }]
}

#[derive(Debug)]
pub struct State {
    pub book_id: i64,
    pub catalog_dir: PathBuf,
    pub book: Book,
    pub current_chapter: usize,
    /// Char offset within the current chapter's text.
    pub cursor_offset: usize,
    /// Cached layout per chapter at `layout_width`.
    pub layouts: Vec<Option<LaidOut>>,
    pub layout_width: usize,
    pub page_height: usize,
    pub last_viewport: Option<(u16, u16)>,
    pub pending_g: bool,
    pub status: Option<String>,
    pub settings: ReaderSettings,
}

#[derive(Debug)]
pub enum ReaderAction {
    None,
    Back,
    OpenPalette,
    Status(StatusMessage),
}

impl State {
    pub fn open(catalog_dir: PathBuf, book_id: i64, book: Book, settings: ReaderSettings) -> Self {
        let chapters = book.chapters.len().max(1);
        let first_non_empty = book
            .chapters
            .iter()
            .position(|c| !c.text.trim().is_empty())
            .unwrap_or(0);
        let mut state = Self {
            book_id,
            catalog_dir,
            book,
            current_chapter: first_non_empty,
            cursor_offset: 0,
            layouts: vec![None; chapters],
            layout_width: 0,
            page_height: 1,
            last_viewport: None,
            pending_g: false,
            status: None,
            settings,
        };
        if let Ok(Some(progress)) = state.read_progress() {
            if progress.chapter < state.book.chapters.len() {
                state.current_chapter = progress.chapter;
                state.cursor_offset = progress.offset;
            }
        }
        state
    }

    fn read_progress(&self) -> catalog::books::Result<Option<ReadingProgress>> {
        let conn =
            catalog::open_existing(&self.catalog_dir).map_err(catalog::books::Error::Catalog)?;
        catalog::books::fetch_reading_progress(&conn, self.book_id)
    }

    fn write_progress(&self) {
        let progress = ReadingProgress {
            chapter: self.current_chapter,
            offset: self.cursor_offset,
        };
        match catalog::open_existing(&self.catalog_dir) {
            Ok(conn) => {
                if let Err(err) =
                    catalog::books::update_reading_progress(&conn, self.book_id, progress)
                {
                    tracing::warn!(error = %err, book_id = self.book_id, "failed to persist reading progress");
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, "failed to open catalog while persisting reading progress");
            }
        }
    }

    pub fn current_chapter_text(&self) -> &str {
        self.book
            .chapters
            .get(self.current_chapter)
            .map(|c| c.text.as_str())
            .unwrap_or("")
    }

    pub fn current_chapter_title(&self) -> &str {
        self.book
            .chapters
            .get(self.current_chapter)
            .map(|c| c.title.as_str())
            .unwrap_or("")
    }

    fn ensure_layout(&mut self, idx: usize, width: usize) {
        if self.layouts[idx].is_none() {
            let empty: Vec<crate::reader::style::StyledLine> = Vec::new();
            let lines: &[crate::reader::style::StyledLine] = self
                .book
                .chapters
                .get(idx)
                .map(|c| c.lines.as_slice())
                .unwrap_or(&empty);
            self.layouts[idx] = Some(lay_out(lines, width));
        }
    }

    fn invalidate_layouts(&mut self, new_width: usize) {
        for entry in self.layouts.iter_mut() {
            *entry = None;
        }
        self.layout_width = new_width;
    }

    fn current_line_index(&mut self) -> usize {
        self.ensure_layout(self.current_chapter, self.layout_width);
        let layout = self.layouts[self.current_chapter]
            .as_ref()
            .expect("layout populated");
        layout.line_for_offset(self.cursor_offset)
    }

    fn cursor_line_and_col(&mut self) -> (usize, usize) {
        self.ensure_layout(self.current_chapter, self.layout_width);
        let layout = self.layouts[self.current_chapter]
            .as_ref()
            .expect("layout populated");
        let line = layout.line_for_offset(self.cursor_offset);
        let line_start = layout.line_offset(line);
        let col = self.cursor_offset.saturating_sub(line_start);
        let line_len = layout.line_char_count(line);
        (line, col.min(line_len))
    }

    fn total_pages(&mut self) -> usize {
        let height = self.page_height;
        let mut total = 0usize;
        for idx in 0..self.book.chapters.len() {
            self.ensure_layout(idx, self.layout_width);
            let lc = self.layouts[idx]
                .as_ref()
                .expect("layout populated")
                .line_count();
            total += page_ranges(lc, height).len();
        }
        total.max(1)
    }

    fn absolute_page_index(&mut self) -> usize {
        let height = self.page_height;
        let mut acc = 0usize;
        for idx in 0..self.current_chapter {
            self.ensure_layout(idx, self.layout_width);
            let lc = self.layouts[idx]
                .as_ref()
                .expect("layout populated")
                .line_count();
            acc += page_ranges(lc, height).len();
        }
        let line = self.current_line_index();
        acc + page_index_for_line(line, height)
    }

    fn chapter_pages(&mut self, idx: usize) -> Vec<PageRange> {
        self.ensure_layout(idx, self.layout_width);
        let lc = self.layouts[idx]
            .as_ref()
            .expect("layout populated")
            .line_count();
        page_ranges(lc, self.page_height)
    }
}

pub fn handle_key(state: &mut State, key: KeyEvent) -> ReaderAction {
    state.status = None;
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    // Two-key leader `gg`: if we are awaiting the second `g`, only that key
    // completes the motion. Any other key cancels the pending leader.
    if state.pending_g {
        state.pending_g = false;
        if matches!(key.code, KeyCode::Char('g')) {
            go_to_first_page(state);
            return ReaderAction::None;
        }
    }

    match key.code {
        KeyCode::Esc => {
            state.write_progress();
            ReaderAction::Back
        }
        KeyCode::Char(':') if !ctrl => ReaderAction::OpenPalette,
        KeyCode::Char('h') if !ctrl => {
            move_cursor_horizontal(state, -1);
            ReaderAction::None
        }
        KeyCode::Char('l') if !ctrl => {
            move_cursor_horizontal(state, 1);
            ReaderAction::None
        }
        KeyCode::Char('j') if !ctrl => {
            move_cursor_vertical(state, 1);
            ReaderAction::None
        }
        KeyCode::Char('k') if !ctrl => {
            move_cursor_vertical(state, -1);
            ReaderAction::None
        }
        KeyCode::Char('0') if !ctrl => {
            move_to_line_edge(state, LineEdge::Start);
            ReaderAction::None
        }
        KeyCode::Char('$') if !ctrl => {
            move_to_line_edge(state, LineEdge::End);
            ReaderAction::None
        }
        KeyCode::Char('w') if !ctrl => {
            move_word(state, WordMotion::Forward);
            ReaderAction::None
        }
        KeyCode::Char('b') if !ctrl => {
            move_word(state, WordMotion::Backward);
            ReaderAction::None
        }
        KeyCode::Char('e') if !ctrl => {
            move_word(state, WordMotion::EndForward);
            ReaderAction::None
        }
        KeyCode::Char('g') if !ctrl => {
            state.pending_g = true;
            ReaderAction::None
        }
        KeyCode::Char('G') if !ctrl => {
            go_to_last_page(state);
            ReaderAction::None
        }
        KeyCode::Char('H') if !ctrl => {
            move_to_page_edge(state, PageEdge::Top);
            ReaderAction::None
        }
        KeyCode::Char('L') if !ctrl => {
            move_to_page_edge(state, PageEdge::Bottom);
            ReaderAction::None
        }
        KeyCode::Char(' ') => {
            page_step(state, PageStep::Down);
            ReaderAction::None
        }
        KeyCode::Char('f') if ctrl => {
            page_step(state, PageStep::Down);
            ReaderAction::None
        }
        KeyCode::Char('b') if ctrl => {
            page_step(state, PageStep::Up);
            ReaderAction::None
        }
        KeyCode::Char('d') if ctrl => {
            page_step(state, PageStep::HalfDown);
            ReaderAction::None
        }
        KeyCode::Char('u') if ctrl => {
            page_step(state, PageStep::HalfUp);
            ReaderAction::None
        }
        KeyCode::Char(']') if !ctrl => {
            change_chapter(state, 1);
            ReaderAction::None
        }
        KeyCode::Char('[') if !ctrl => {
            change_chapter(state, -1);
            ReaderAction::None
        }
        _ => ReaderAction::None,
    }
}

pub fn captures_text_input(_state: &State) -> bool {
    false
}

pub fn go_to_page(state: &mut State, n: usize) {
    if n == 0 {
        state.status = Some("page numbers start at 1".to_string());
        return;
    }
    let target = n.saturating_sub(1);
    let height = state.page_height;
    let mut remaining = target;
    for idx in 0..state.book.chapters.len() {
        let pages = state.chapter_pages(idx);
        if remaining < pages.len() {
            state.current_chapter = idx;
            let layout = state.layouts[idx].as_ref().expect("layout populated");
            let target_line = pages[remaining]
                .start
                .min(layout.line_count().saturating_sub(1));
            state.cursor_offset = layout.line_offset(target_line);
            state.write_progress();
            return;
        }
        remaining -= pages.len();
    }
    // Past the end: clamp to last page.
    if let Some(last_idx) = state.book.chapters.len().checked_sub(1) {
        state.current_chapter = last_idx;
        let pages = state.chapter_pages(last_idx);
        if let Some(last_page) = pages.last() {
            let layout = state.layouts[last_idx].as_ref().expect("layout populated");
            let target_line = last_page.start.min(layout.line_count().saturating_sub(1));
            state.cursor_offset = layout.line_offset(target_line);
        }
        state.write_progress();
        state.status = Some(format!("clamped to last page ({})", state.total_pages()));
    }
    let _ = height;
}

pub fn go_to_chapter(state: &mut State, n: usize) {
    if n == 0 || n > state.book.chapters.len() {
        state.status = Some(format!("chapters are 1..={}", state.book.chapters.len()));
        return;
    }
    state.current_chapter = n - 1;
    state.cursor_offset = 0;
    state.pending_g = false;
    state.write_progress();
}

enum LineEdge {
    Start,
    End,
}

enum PageEdge {
    Top,
    Bottom,
}

fn move_to_page_edge(state: &mut State, edge: PageEdge) {
    state.ensure_layout(state.current_chapter, state.layout_width);
    let layout = state.layouts[state.current_chapter]
        .as_ref()
        .expect("layout populated");
    let height = state.page_height;
    let pages = page_ranges(layout.line_count(), height);
    let cursor_line = layout.line_for_offset(state.cursor_offset);
    let page_idx = page_index_for_line(cursor_line, height);
    let Some(range) = pages.get(page_idx).copied() else {
        return;
    };
    let target = match edge {
        PageEdge::Top => range.start,
        PageEdge::Bottom => range.end.saturating_sub(1).max(range.start),
    };
    state.cursor_offset = layout.line_offset(target);
}

fn move_to_line_edge(state: &mut State, edge: LineEdge) {
    let (line_idx, _col) = state.cursor_line_and_col();
    state.ensure_layout(state.current_chapter, state.layout_width);
    let layout = state.layouts[state.current_chapter]
        .as_ref()
        .expect("layout populated");
    let line_start = layout.line_offset(line_idx);
    let line_len = layout.line_char_count(line_idx);
    state.cursor_offset = match edge {
        LineEdge::Start => line_start,
        LineEdge::End => line_start + line_len.saturating_sub(1).max(0),
    };
}

fn move_cursor_horizontal(state: &mut State, delta: i64) {
    let (line_idx, col) = state.cursor_line_and_col();
    state.ensure_layout(state.current_chapter, state.layout_width);
    let layout = state.layouts[state.current_chapter]
        .as_ref()
        .expect("layout populated");
    let line_len = layout.line_char_count(line_idx);
    let new_col = (col as i64 + delta).clamp(0, line_len.saturating_sub(1).max(0) as i64) as usize;
    state.cursor_offset = layout.line_offset(line_idx) + new_col;
}

fn move_cursor_vertical(state: &mut State, delta: i64) {
    let (line_idx, col) = state.cursor_line_and_col();
    state.ensure_layout(state.current_chapter, state.layout_width);
    let layout = state.layouts[state.current_chapter]
        .as_ref()
        .expect("layout populated");
    let last_line = layout.line_count().saturating_sub(1);
    let new_line = (line_idx as i64 + delta).clamp(0, last_line as i64) as usize;
    let target_line_len = layout.line_char_count(new_line);
    let new_col = col.min(target_line_len.saturating_sub(1).max(0));
    state.cursor_offset = layout.line_offset(new_line) + new_col;
}

enum WordMotion {
    Forward,
    Backward,
    EndForward,
}

fn move_word(state: &mut State, motion: WordMotion) {
    let text = state.current_chapter_text();
    let total = text.chars().count();
    if total == 0 {
        return;
    }
    let words: Vec<(usize, &str)> = text
        .split_word_bound_indices()
        .filter(|(_, w)| !w.chars().all(|c| c.is_whitespace()))
        .collect();
    if words.is_empty() {
        return;
    }
    // Convert byte indices to char offsets.
    let char_offsets: Vec<(usize, usize)> = words
        .iter()
        .map(|(byte_idx, w)| {
            let prefix_chars = text[..*byte_idx].chars().count();
            let word_chars = w.chars().count();
            (prefix_chars, prefix_chars + word_chars - 1)
        })
        .collect();

    let cur = state.cursor_offset;
    let new_offset = match motion {
        WordMotion::Forward => char_offsets
            .iter()
            .find(|(start, _)| *start > cur)
            .map(|(start, _)| *start)
            .unwrap_or(cur),
        WordMotion::Backward => char_offsets
            .iter()
            .rev()
            .find(|(start, _)| *start < cur)
            .map(|(start, _)| *start)
            .unwrap_or(cur),
        WordMotion::EndForward => char_offsets
            .iter()
            .find(|(_, end)| *end > cur)
            .map(|(_, end)| *end)
            .unwrap_or(cur),
    };
    state.cursor_offset = new_offset.min(total.saturating_sub(1));
}

enum PageStep {
    Up,
    Down,
    HalfUp,
    HalfDown,
}

fn page_step(state: &mut State, step: PageStep) {
    let half = (state.page_height / 2).max(1) as i64;
    match step {
        PageStep::Down => move_to_next_page(state),
        PageStep::Up => move_to_prev_page(state),
        PageStep::HalfDown => scroll_within_chapter(state, half),
        PageStep::HalfUp => scroll_within_chapter(state, -half),
    }
    state.write_progress();
}

/// Strict page navigation: jump to the first line of the next page in
/// reading order. Within a chapter, that's the next page. On the chapter's
/// last page, it crosses to the first page of the next chapter. At the very
/// end of the book it stays put.
fn move_to_next_page(state: &mut State) {
    state.ensure_layout(state.current_chapter, state.layout_width);
    let layout = state.layouts[state.current_chapter]
        .as_ref()
        .expect("layout populated");
    let pages = page_ranges(layout.line_count(), state.page_height);
    let cursor_line = layout.line_for_offset(state.cursor_offset);
    let current_page = page_index_for_line(cursor_line, state.page_height);
    if current_page + 1 < pages.len() {
        let range = pages[current_page + 1];
        let line = range.start.min(layout.line_count().saturating_sub(1));
        state.cursor_offset = layout.line_offset(line);
        return;
    }
    // Last page → roll over to the next chapter at its first line.
    if state.current_chapter + 1 < state.book.chapters.len() {
        state.current_chapter += 1;
        state.cursor_offset = 0;
    }
}

fn move_to_prev_page(state: &mut State) {
    state.ensure_layout(state.current_chapter, state.layout_width);
    let layout = state.layouts[state.current_chapter]
        .as_ref()
        .expect("layout populated");
    let pages = page_ranges(layout.line_count(), state.page_height);
    let cursor_line = layout.line_for_offset(state.cursor_offset);
    let current_page = page_index_for_line(cursor_line, state.page_height);
    if current_page > 0 {
        let range = pages[current_page - 1];
        let line = range.start.min(layout.line_count().saturating_sub(1));
        state.cursor_offset = layout.line_offset(line);
        return;
    }
    // First page → roll back to the previous chapter at its last page.
    if state.current_chapter == 0 {
        return;
    }
    state.current_chapter -= 1;
    state.ensure_layout(state.current_chapter, state.layout_width);
    let prev = state.layouts[state.current_chapter]
        .as_ref()
        .expect("layout populated");
    let prev_pages = page_ranges(prev.line_count(), state.page_height);
    if let Some(last) = prev_pages.last() {
        let line = last.start.min(prev.line_count().saturating_sub(1));
        state.cursor_offset = prev.line_offset(line);
    }
}

/// Half-page (Ctrl+d / Ctrl+u) shifts by `delta` lines but never crosses
/// the current chapter's boundary — same containment policy as the strict
/// page commands above.
fn scroll_within_chapter(state: &mut State, delta_lines: i64) {
    state.ensure_layout(state.current_chapter, state.layout_width);
    let layout = state.layouts[state.current_chapter]
        .as_ref()
        .expect("layout populated");
    let line_idx = layout.line_for_offset(state.cursor_offset) as i64;
    let last_line = layout.line_count().saturating_sub(1) as i64;
    let target = (line_idx + delta_lines).clamp(0, last_line.max(0)) as usize;
    state.cursor_offset = layout.line_offset(target);
}

fn change_chapter(state: &mut State, delta: i64) {
    let max = state.book.chapters.len() as i64;
    let new_chapter = (state.current_chapter as i64 + delta).clamp(0, (max - 1).max(0));
    state.current_chapter = new_chapter as usize;
    state.cursor_offset = 0;
    state.write_progress();
}

fn go_to_first_page(state: &mut State) {
    // Mirror the "open" behavior: skip empty front-matter spine entries so
    // `gg` lands on actual content. `:c1` remains the way to reach a literal
    // empty first chapter when that's what the user wants.
    state.current_chapter = state
        .book
        .chapters
        .iter()
        .position(|c| !c.text.trim().is_empty())
        .unwrap_or(0);
    state.cursor_offset = 0;
    state.write_progress();
}

fn go_to_last_page(state: &mut State) {
    let last_chapter = state.book.chapters.len().saturating_sub(1);
    state.current_chapter = last_chapter;
    state.ensure_layout(state.current_chapter, state.layout_width);
    let layout = state.layouts[state.current_chapter]
        .as_ref()
        .expect("layout populated");
    let pages = page_ranges(layout.line_count(), state.page_height);
    if let Some(last) = pages.last() {
        let line = last.start.min(layout.line_count().saturating_sub(1));
        state.cursor_offset = layout.line_offset(line);
    }
    state.write_progress();
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &mut State) {
    let frame_rects = compute_frame(area, &state.settings);
    let width = frame_rects.content.width.max(1) as usize;
    let page_height = frame_rects.content.height.max(1) as usize;

    if state.layout_width != width {
        state.invalidate_layouts(width);
    }
    state.page_height = page_height;
    state.last_viewport = Some((area.width, area.height));
    state.ensure_layout(state.current_chapter, state.layout_width);

    render_page(frame, frame_rects.content, state);
    render_footer(frame, frame_rects.footer, state);
}

#[derive(Debug, Clone, Copy)]
struct FrameRects {
    /// Centered, margin-respecting rect where the page text is rendered.
    content: Rect,
    /// Always 1 row tall; spans the full viewport width so the contrast bar
    /// reads as a hard separator from the body.
    footer: Rect,
}

fn compute_frame(area: Rect, settings: &ReaderSettings) -> FrameRects {
    // Footer is always exactly 1 row at the bottom of the viewport.
    let footer_y = area.y + area.height.saturating_sub(1);
    let footer = Rect {
        x: area.x,
        y: footer_y,
        width: area.width,
        height: area.height.min(1),
    };

    let body_top = area.y + settings.vertical_margin.min(area.height);
    let body_bottom_max = footer_y; // body cannot overlap the footer row
    let body_height_after_top = body_bottom_max.saturating_sub(body_top);
    let body_height = body_height_after_top.saturating_sub(settings.vertical_margin);
    let body_height = body_height.max(1);

    // Horizontal: enforce the minimum margin, then center within max_content_width
    // when the terminal is wider than the configured target.
    let usable_width = area.width.saturating_sub(2 * settings.horizontal_margin);
    let content_width = if settings.max_content_width == 0 {
        usable_width
    } else {
        usable_width.min(settings.max_content_width)
    }
    .max(1);
    let leftover = area.width.saturating_sub(content_width);
    let body_x = area.x + leftover / 2;

    let content = Rect {
        x: body_x,
        y: body_top,
        width: content_width,
        height: body_height,
    };
    FrameRects { content, footer }
}

fn render_page(frame: &mut Frame<'_>, area: Rect, state: &mut State) {
    let layout_width = state.layout_width;
    state.ensure_layout(state.current_chapter, layout_width);
    let layout = state.layouts[state.current_chapter]
        .as_ref()
        .expect("layout populated");
    let page_height = state.page_height;
    let cursor_line = layout.line_for_offset(state.cursor_offset);
    let page_idx = page_index_for_line(cursor_line, page_height);
    let pages = page_ranges(layout.line_count(), page_height);
    let range = pages
        .get(page_idx)
        .copied()
        .unwrap_or(PageRange { start: 0, end: 0 });
    let cursor_col = state
        .cursor_offset
        .saturating_sub(layout.line_offset(cursor_line));
    let cursor_in_page_row = cursor_line.saturating_sub(range.start);

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(range.len());
    for (row_in_page, line_idx) in (range.start..range.end).enumerate() {
        let Some(styled) = layout.lines.get(line_idx).cloned() else {
            lines.push(Line::from(""));
            continue;
        };
        if row_in_page == cursor_in_page_row {
            lines.push(styled_line_with_cursor(&styled, cursor_col));
        } else {
            lines.push(styled_line_to_ratatui(&styled));
        }
    }
    // Pad to fill viewport height with blank lines.
    while lines.len() < (range.end.saturating_sub(range.start)).max(page_height) {
        if lines.len() >= page_height {
            break;
        }
        lines.push(Line::from(""));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

fn style_for_span(style: &crate::reader::style::TextStyle) -> Style {
    let mut out = Style::default();
    let mut mods = Modifier::empty();
    if style.bold {
        mods |= Modifier::BOLD;
    }
    if style.italic {
        mods |= Modifier::ITALIC;
    }
    if style.underline {
        mods |= Modifier::UNDERLINED;
    }
    if style.strikethrough {
        mods |= Modifier::CROSSED_OUT;
    }
    if let Some(level) = style.heading_level {
        // Headings always render bold. Level 1/2 also get a colour accent so
        // the eye lands on them quickly — terminals can't change font size,
        // so colour + bold is the substitute.
        mods |= Modifier::BOLD;
        out = match level {
            1 => out.fg(Color::Cyan),
            2 => out.fg(Color::LightCyan),
            3 => out.fg(Color::LightBlue),
            _ => out,
        };
    }
    if style.link {
        out = out.fg(Color::Blue);
    }
    if style.code {
        out = out.bg(Color::Rgb(40, 40, 40));
    }
    out.add_modifier(mods)
}

fn styled_line_to_ratatui(line: &crate::reader::style::StyledLine) -> Line<'static> {
    if line.spans.is_empty() {
        return Line::from("");
    }
    let spans: Vec<Span<'static>> = line
        .spans
        .iter()
        .map(|s| Span::styled(s.text.clone(), style_for_span(&s.style)))
        .collect();
    Line::from(spans)
}

fn styled_line_with_cursor(
    line: &crate::reader::style::StyledLine,
    cursor_col: usize,
) -> Line<'static> {
    if line.is_empty() {
        return Line::from(Span::styled(
            " ".to_string(),
            Style::default().add_modifier(Modifier::REVERSED),
        ));
    }
    let total = line.char_count();
    let col = cursor_col.min(total.saturating_sub(1).max(0));

    let mut out_spans: Vec<Span<'static>> = Vec::with_capacity(line.spans.len() + 2);
    let mut consumed = 0usize;
    for span in &line.spans {
        let span_len = span.text.chars().count();
        let span_start = consumed;
        let span_end = consumed + span_len;
        consumed = span_end;
        if span_len == 0 {
            continue;
        }
        let base = style_for_span(&span.style);
        if col < span_start || col >= span_end {
            out_spans.push(Span::styled(span.text.clone(), base));
            continue;
        }
        // The cursor lands inside this span — split into [before][cursor][after].
        let rel = col - span_start;
        let before: String = span.text.chars().take(rel).collect();
        let cursor_ch: String = span
            .text
            .chars()
            .nth(rel)
            .map(|c| c.to_string())
            .unwrap_or_else(|| " ".to_string());
        let after: String = span.text.chars().skip(rel + 1).collect();
        if !before.is_empty() {
            out_spans.push(Span::styled(before, base));
        }
        out_spans.push(Span::styled(
            cursor_ch,
            base.add_modifier(Modifier::REVERSED),
        ));
        if !after.is_empty() {
            out_spans.push(Span::styled(after, base));
        }
    }
    Line::from(out_spans)
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, state: &mut State) {
    let chapters = state.book.chapters.len().max(1);
    let current_chapter = state.current_chapter + 1;
    let total_pages = state.total_pages();
    let abs_page = state.absolute_page_index() + 1;
    let title = state.book.title.clone();
    let chapter_title = state.current_chapter_title().to_string();
    let status = state.status.clone();

    // Footer carries a distinct background so it can't blend into the body
    // text. Spans inherit the base style and layer their own fg/modifiers
    // on top — that keeps the contrast bar uniform end-to-end.
    let base = Style::default().bg(Color::DarkGray).fg(Color::White);

    // Pin the page counter to the right edge and let the title/chapter/status
    // flow on the left. We size the right rect to the exact width of the
    // counter (plus one trailing space) so the two paragraphs don't overlap.
    let right_text = format!("pág {abs_page}/{total_pages} ");
    let right_width = u16::try_from(right_text.chars().count()).unwrap_or(0);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(right_width)])
        .split(area);

    let mut left_spans: Vec<Span<'static>> = vec![
        Span::styled(format!(" {title}"), base.add_modifier(Modifier::BOLD)),
        Span::styled("  ·  ", base),
        Span::styled(format!("cap {current_chapter}/{chapters}"), base),
    ];
    if !chapter_title.is_empty() {
        left_spans.push(Span::styled(
            format!(" — {chapter_title}"),
            base.fg(Color::Gray),
        ));
    }
    if let Some(message) = status {
        left_spans.push(Span::styled("  ·  ", base));
        left_spans.push(Span::styled(message, base.fg(Color::Yellow)));
    }
    let left = Paragraph::new(Line::from(left_spans)).style(base);
    frame.render_widget(left, chunks[0]);

    let right = Paragraph::new(Line::from(Span::styled(right_text, base)))
        .style(base)
        .alignment(Alignment::Right);
    frame.render_widget(right, chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::Chapter;
    use std::path::PathBuf;

    fn build_state(chapters: Vec<Chapter>) -> State {
        let book = Book {
            id: 1,
            title: "Book".into(),
            author: None,
            chapters,
        };
        let mut s = State::open(
            PathBuf::from("/tmp/cdx-test-dummy"),
            1,
            book,
            ReaderSettings::default(),
        );
        s.layout_width = 20;
        s.page_height = 5;
        for idx in 0..s.book.chapters.len() {
            s.ensure_layout(idx, 20);
        }
        s
    }

    #[test]
    fn cursor_starts_at_offset_zero() {
        let s = build_state(vec![Chapter::from_text(
            "A".into(),
            "alpha bravo charlie".into(),
        )]);
        assert_eq!(s.cursor_offset, 0);
        assert_eq!(s.current_chapter, 0);
    }

    #[test]
    fn move_cursor_horizontal_clamps_to_line_end() {
        let mut s = build_state(vec![Chapter::from_text("A".into(), "abc def".into())]);
        for _ in 0..100 {
            move_cursor_horizontal(&mut s, 1);
        }
        let (line, col) = s.cursor_line_and_col();
        assert_eq!(line, 0);
        // Line "abc def" has 7 chars; col 6 is the last index.
        assert_eq!(col, 6);
    }

    #[test]
    fn move_cursor_vertical_walks_lines() {
        let mut s = build_state(vec![Chapter::from_text(
            "A".into(),
            "line one\nline two\nline three".into(),
        )]);
        move_cursor_vertical(&mut s, 1);
        let (line, _) = s.cursor_line_and_col();
        assert_eq!(line, 1);
        move_cursor_vertical(&mut s, 1);
        let (line, _) = s.cursor_line_and_col();
        assert_eq!(line, 2);
        // Past the end clamps.
        move_cursor_vertical(&mut s, 10);
        let (line, _) = s.cursor_line_and_col();
        assert_eq!(line, 2);
    }

    #[test]
    fn word_forward_jumps_to_next_word() {
        let mut s = build_state(vec![Chapter::from_text(
            "A".into(),
            "alpha bravo charlie".into(),
        )]);
        move_word(&mut s, WordMotion::Forward);
        assert_eq!(s.cursor_offset, 6); // start of "bravo"
        move_word(&mut s, WordMotion::Forward);
        assert_eq!(s.cursor_offset, 12); // start of "charlie"
    }

    #[test]
    fn word_back_jumps_to_previous_word() {
        let mut s = build_state(vec![Chapter::from_text(
            "A".into(),
            "alpha bravo charlie".into(),
        )]);
        s.cursor_offset = 12;
        move_word(&mut s, WordMotion::Backward);
        assert_eq!(s.cursor_offset, 6);
    }

    #[test]
    fn page_step_down_advances_by_page_height() {
        // 12 lines of text so we have multiple pages at height 5.
        let body = (0..12)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut s = build_state(vec![Chapter::from_text("A".into(), body)]);
        page_step(&mut s, PageStep::Down);
        let line = s.current_line_index();
        assert_eq!(line, 5);
        page_step(&mut s, PageStep::Down);
        let line = s.current_line_index();
        assert_eq!(line, 10);
    }

    #[test]
    fn go_to_chapter_clamps() {
        let mut s = build_state(vec![
            Chapter::from_text("1".into(), "a".into()),
            Chapter::from_text("2".into(), "b".into()),
        ]);
        go_to_chapter(&mut s, 2);
        assert_eq!(s.current_chapter, 1);
        go_to_chapter(&mut s, 10);
        // unchanged + status set
        assert_eq!(s.current_chapter, 1);
        assert!(s.status.is_some());
    }

    #[test]
    fn go_to_page_lands_on_correct_chapter() {
        let body_a = (0..7)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let body_b = (0..7)
            .map(|i| format!("b{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut s = build_state(vec![
            Chapter::from_text("1".into(), body_a),
            Chapter::from_text("2".into(), body_b),
        ]);
        // chapter 1 has 7 lines / page=5 → 2 pages
        // chapter 2 has 7 lines / page=5 → 2 pages
        // page 3 (1-indexed) = first page of chapter 2 (0-indexed page 0)
        go_to_page(&mut s, 3);
        assert_eq!(s.current_chapter, 1);
        let line = s.current_line_index();
        assert_eq!(line, 0);
    }

    #[test]
    fn pending_g_triggers_first_page_on_second_g() {
        let mut s = build_state(vec![Chapter::from_text(
            "1".into(),
            "line0\nline1\nline2".into(),
        )]);
        s.cursor_offset = 10; // somewhere in line 1 or 2
        let event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        handle_key(&mut s, event);
        assert!(s.pending_g);
        let event2 = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        handle_key(&mut s, event2);
        assert!(!s.pending_g);
        assert_eq!(s.cursor_offset, 0);
        assert_eq!(s.current_chapter, 0);
    }

    #[test]
    fn pending_g_resets_on_other_key() {
        let mut s = build_state(vec![Chapter::from_text(
            "1".into(),
            "line0\nline1\nline2".into(),
        )]);
        let event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        handle_key(&mut s, event);
        assert!(s.pending_g);
        // any other key resets pending_g
        let event2 = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        handle_key(&mut s, event2);
        assert!(!s.pending_g);
    }

    #[test]
    fn capital_g_goes_to_last_page() {
        let mut s = build_state(vec![
            Chapter::from_text("1".into(), "a\nb".into()),
            Chapter::from_text("2".into(), "c\nd\ne".into()),
        ]);
        let event = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE);
        handle_key(&mut s, event);
        assert_eq!(s.current_chapter, 1);
    }

    #[test]
    fn compute_frame_centers_content_when_terminal_wider_than_max() {
        let area = Rect::new(0, 0, 120, 30);
        let settings = ReaderSettings {
            max_content_width: 80,
            horizontal_margin: 2,
            vertical_margin: 1,
        };
        let frame = compute_frame(area, &settings);
        assert_eq!(frame.content.width, 80);
        // (120 - 80) / 2 = 20 leftover on the left
        assert_eq!(frame.content.x, 20);
        // top margin = 1, vertical_margin between bottom of body and footer = 1, footer = 1
        assert_eq!(frame.content.y, 1);
        assert_eq!(frame.content.height, 27); // 30 - 1 (top margin) - 1 (bottom margin) - 1 (footer)
        assert_eq!(frame.footer.y, 29);
        assert_eq!(frame.footer.height, 1);
        assert_eq!(frame.footer.width, 120);
    }

    #[test]
    fn compute_frame_uses_full_width_when_terminal_narrower_than_max() {
        let area = Rect::new(0, 0, 50, 20);
        let settings = ReaderSettings {
            max_content_width: 80,
            horizontal_margin: 2,
            vertical_margin: 0,
        };
        let frame = compute_frame(area, &settings);
        // 50 - 2*2 = 46 usable
        assert_eq!(frame.content.width, 46);
        assert_eq!(frame.content.x, 2);
    }

    #[test]
    fn compute_frame_max_zero_means_no_cap() {
        let area = Rect::new(0, 0, 200, 30);
        let settings = ReaderSettings {
            max_content_width: 0,
            horizontal_margin: 4,
            vertical_margin: 1,
        };
        let frame = compute_frame(area, &settings);
        assert_eq!(frame.content.width, 200 - 8);
        assert_eq!(frame.content.x, 4);
    }

    #[test]
    fn opening_skips_empty_leading_chapters() {
        // First two chapters are empty front-matter (cover, title page);
        // opening should land on the first non-empty one.
        let s = build_state(vec![
            Chapter::from_text("Cover".into(), "".into()),
            Chapter::from_text("Title".into(), "  \n  ".into()),
            Chapter::from_text("Intro".into(), "real content here".into()),
        ]);
        assert_eq!(s.current_chapter, 2);
    }

    #[test]
    fn gg_skips_empty_leading_chapters() {
        let mut s = build_state(vec![
            Chapter::from_text("Cover".into(), "".into()),
            Chapter::from_text("Intro".into(), "real content here".into()),
        ]);
        // Move forward so we have somewhere to come back from.
        s.current_chapter = 1;
        s.cursor_offset = 5;
        go_to_first_page(&mut s);
        assert_eq!(s.current_chapter, 1);
        assert_eq!(s.cursor_offset, 0);
    }

    #[test]
    fn esc_returns_back_action() {
        let mut s = build_state(vec![Chapter::from_text("1".into(), "x".into())]);
        let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = handle_key(&mut s, event);
        assert!(matches!(action, ReaderAction::Back));
    }

    #[test]
    fn page_step_down_advances_within_chapter_then_rolls_to_next() {
        // page_height = 5 (set by build_state). Chapter 1 has 12 lines → 3
        // pages (lines 0..5, 5..10, 10..12). Chapter 2 has 12 lines too.
        let ch1 = (0..12)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let ch2 = (0..12)
            .map(|i| format!("b{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut s = build_state(vec![
            Chapter::from_text("1".into(), ch1),
            Chapter::from_text("2".into(), ch2),
        ]);
        // Page 1 → 2 (still in chapter 1).
        page_step(&mut s, PageStep::Down);
        assert_eq!(s.current_chapter, 0);
        assert_eq!(s.current_line_index(), 5);
        // Page 2 → 3 (last page of chapter 1).
        page_step(&mut s, PageStep::Down);
        assert_eq!(s.current_chapter, 0);
        assert_eq!(s.current_line_index(), 10);
        // Page 3 → chapter 2 page 1.
        page_step(&mut s, PageStep::Down);
        assert_eq!(s.current_chapter, 1, "Space rolls into the next chapter");
        assert_eq!(s.current_line_index(), 0);
    }

    #[test]
    fn page_step_down_at_end_of_book_stays_put() {
        let body = (0..12)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut s = build_state(vec![Chapter::from_text("1".into(), body)]);
        // Walk to the last page (page 3 of 3).
        page_step(&mut s, PageStep::Down);
        page_step(&mut s, PageStep::Down);
        assert_eq!(s.current_line_index(), 10);
        // Pressing Space on the last page of the last chapter has nowhere
        // to go — stay put.
        page_step(&mut s, PageStep::Down);
        assert_eq!(s.current_chapter, 0);
        assert_eq!(s.current_line_index(), 10);
    }

    #[test]
    fn page_step_up_rolls_back_to_previous_chapter_last_page() {
        let ch1 = (0..12)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut s = build_state(vec![
            Chapter::from_text("1".into(), ch1),
            Chapter::from_text("2".into(), "later".into()),
        ]);
        s.current_chapter = 1;
        s.cursor_offset = 0;
        page_step(&mut s, PageStep::Up);
        assert_eq!(
            s.current_chapter, 0,
            "Ctrl+b rolls back to the previous chapter's last page"
        );
        assert_eq!(s.current_line_index(), 10);
    }

    #[test]
    fn page_step_up_at_start_of_book_stays_put() {
        let body = (0..12)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut s = build_state(vec![Chapter::from_text("1".into(), body)]);
        page_step(&mut s, PageStep::Up);
        assert_eq!(s.current_chapter, 0);
        assert_eq!(s.current_line_index(), 0);
    }

    #[test]
    fn half_page_step_clamps_to_chapter_bounds() {
        let body = (0..12)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut s = build_state(vec![
            Chapter::from_text("1".into(), body),
            Chapter::from_text("2".into(), "later".into()),
        ]);
        // Move cursor near the end and press Ctrl+d (half page = 2 lines with
        // page_height=5). Must clamp to last line, not spill to chapter 2.
        move_cursor_vertical(&mut s, 11); // last line of chapter 1
        page_step(&mut s, PageStep::HalfDown);
        assert_eq!(s.current_chapter, 0);
        assert_eq!(s.current_line_index(), 11);
    }

    #[test]
    fn capital_h_jumps_to_top_of_page() {
        let body = (0..12)
            .map(|i| format!("L{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut s = build_state(vec![Chapter::from_text("1".into(), body)]);
        // Move into the middle of the first page (height = 5 in build_state).
        move_cursor_vertical(&mut s, 3);
        assert_eq!(s.current_line_index(), 3);
        let event = KeyEvent::new(KeyCode::Char('H'), KeyModifiers::NONE);
        handle_key(&mut s, event);
        assert_eq!(s.current_line_index(), 0);
    }

    #[test]
    fn capital_l_jumps_to_bottom_of_page() {
        let body = (0..12)
            .map(|i| format!("L{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut s = build_state(vec![Chapter::from_text("1".into(), body)]);
        // Cursor starts at line 0; page_height is 5 so the bottom of page 0
        // is line 4. `L` should jump there.
        let event = KeyEvent::new(KeyCode::Char('L'), KeyModifiers::NONE);
        handle_key(&mut s, event);
        assert_eq!(s.current_line_index(), 4);
    }

    #[test]
    fn capital_h_and_l_stay_within_current_page_after_paging_down() {
        let body = (0..12)
            .map(|i| format!("L{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut s = build_state(vec![Chapter::from_text("1".into(), body)]);
        page_step(&mut s, PageStep::Down);
        // Now on page 1 (lines 5..10). H lands on line 5, L on line 9.
        let event = KeyEvent::new(KeyCode::Char('H'), KeyModifiers::NONE);
        handle_key(&mut s, event);
        assert_eq!(s.current_line_index(), 5);
        let event = KeyEvent::new(KeyCode::Char('L'), KeyModifiers::NONE);
        handle_key(&mut s, event);
        assert_eq!(s.current_line_index(), 9);
    }

    #[test]
    fn ctrl_b_pages_up() {
        let body = (0..12)
            .map(|i| format!("L{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut s = build_state(vec![Chapter::from_text("1".into(), body)]);
        page_step(&mut s, PageStep::Down);
        let after_down = s.current_line_index();
        assert!(after_down > 0);
        let event = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
        handle_key(&mut s, event);
        let after_up = s.current_line_index();
        assert!(after_up < after_down);
    }

    #[test]
    fn pending_g_does_not_trigger_when_chapter_brackets_pressed() {
        let mut s = build_state(vec![
            Chapter::from_text("1".into(), "a".into()),
            Chapter::from_text("2".into(), "b".into()),
        ]);
        let event = KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE);
        handle_key(&mut s, event);
        assert_eq!(s.current_chapter, 1);
    }
}
