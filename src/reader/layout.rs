use unicode_segmentation::UnicodeSegmentation;

use crate::reader::style::{StyledLine, StyledSpan, TextStyle};

#[derive(Debug, Clone)]
pub struct LaidOut {
    /// Wrapped, styled lines ready for rendering.
    pub lines: Vec<StyledLine>,
    /// Char offset of each wrapped line's first char within the source
    /// chapter (paragraphs joined by '\n'). Used by the cursor.
    pub offsets: Vec<usize>,
}

impl LaidOut {
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn line_offset(&self, idx: usize) -> usize {
        self.offsets.get(idx).copied().unwrap_or(0)
    }

    pub fn line_for_offset(&self, char_offset: usize) -> usize {
        let mut lo = 0usize;
        let mut hi = self.offsets.len();
        while lo + 1 < hi {
            let mid = (lo + hi) / 2;
            if self.offsets[mid] <= char_offset {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }

    pub fn line_char_count(&self, idx: usize) -> usize {
        self.lines.get(idx).map(StyledLine::char_count).unwrap_or(0)
    }
}

pub fn lay_out(paragraphs: &[StyledLine], width: usize) -> LaidOut {
    let width = width.max(1);
    let mut lines: Vec<StyledLine> = Vec::new();
    let mut offsets: Vec<usize> = Vec::new();
    let mut char_cursor: usize = 0;

    for paragraph in paragraphs {
        let paragraph_start = char_cursor;
        let plain = paragraph.plain_text();
        if plain.is_empty() {
            lines.push(StyledLine::empty());
            offsets.push(paragraph_start);
            char_cursor += 1;
            continue;
        }
        let wrapped = wrap_paragraph(paragraph, &plain, width);
        for (styled, rel_offset) in wrapped {
            offsets.push(paragraph_start + rel_offset);
            lines.push(styled);
        }
        char_cursor += plain.chars().count() + 1;
    }

    if lines.is_empty() {
        lines.push(StyledLine::empty());
        offsets.push(0);
    }

    LaidOut { lines, offsets }
}

struct StyleIndex {
    boundaries: Vec<(usize, TextStyle)>,
}

impl StyleIndex {
    fn build(line: &StyledLine) -> Self {
        let mut boundaries = Vec::with_capacity(line.spans.len());
        let mut acc = 0usize;
        for span in &line.spans {
            boundaries.push((acc, span.style));
            acc += span.char_count();
        }
        Self { boundaries }
    }

    fn style_at(&self, char_offset: usize) -> TextStyle {
        let mut found = TextStyle::default();
        for (start, style) in &self.boundaries {
            if *start <= char_offset {
                found = *style;
            } else {
                break;
            }
        }
        found
    }
}

struct LineBuilder {
    spans: Vec<StyledSpan>,
    width: usize,
}

impl LineBuilder {
    fn new() -> Self {
        Self {
            spans: Vec::new(),
            width: 0,
        }
    }

    fn is_empty(&self) -> bool {
        self.spans.iter().all(|s| s.text.is_empty())
    }

    fn push_char(&mut self, ch: char, style: TextStyle) {
        self.width += 1;
        if let Some(last) = self.spans.last_mut() {
            if last.style == style {
                last.text.push(ch);
                return;
            }
        }
        self.spans.push(StyledSpan {
            text: ch.to_string(),
            style,
        });
    }

    fn push_str(&mut self, text: &str, style: TextStyle) {
        self.width += text.chars().count();
        if let Some(last) = self.spans.last_mut() {
            if last.style == style {
                last.text.push_str(text);
                return;
            }
        }
        self.spans.push(StyledSpan {
            text: text.to_string(),
            style,
        });
    }

    fn finish_trimmed(mut self) -> StyledLine {
        while let Some(last) = self.spans.last_mut() {
            while last.text.ends_with(' ') {
                last.text.pop();
            }
            if last.text.is_empty() {
                self.spans.pop();
            } else {
                break;
            }
        }
        StyledLine { spans: self.spans }
    }

    fn finish(self) -> StyledLine {
        StyledLine { spans: self.spans }
    }
}

fn wrap_paragraph(paragraph: &StyledLine, plain: &str, width: usize) -> Vec<(StyledLine, usize)> {
    if plain.chars().count() <= width {
        return vec![(paragraph.clone(), 0)];
    }

    let style_index = StyleIndex::build(paragraph);
    let mut out: Vec<(StyledLine, usize)> = Vec::new();
    let mut current = LineBuilder::new();
    let mut current_offset = 0usize;
    let mut input_consumed = 0usize;

    for word in plain.split_word_bounds() {
        let word_width = word.chars().count();
        let is_whitespace = word.chars().all(|c| c.is_whitespace());

        if is_whitespace {
            if current.width + word_width > width {
                if !current.is_empty() {
                    let line = std::mem::replace(&mut current, LineBuilder::new()).finish_trimmed();
                    out.push((line, current_offset));
                    current_offset = input_consumed + word_width;
                }
            } else {
                // Preserve the style of the preceding char so adjacent
                // whitespace inherits the run it belongs to.
                let style = style_index.style_at(input_consumed.saturating_sub(1));
                current.push_str(word, style);
            }
            input_consumed += word_width;
            continue;
        }

        if word_width > width {
            if !current.is_empty() {
                let line = std::mem::replace(&mut current, LineBuilder::new()).finish_trimmed();
                out.push((line, current_offset));
                current_offset = input_consumed;
            }
            for (i, ch) in word.chars().enumerate() {
                if current.width + 1 > width {
                    let line = std::mem::replace(&mut current, LineBuilder::new()).finish_trimmed();
                    out.push((line, current_offset));
                    current_offset = input_consumed + i;
                }
                let style = style_index.style_at(input_consumed + i);
                current.push_char(ch, style);
            }
            input_consumed += word_width;
            continue;
        }

        if current.width + word_width > width {
            let line = std::mem::replace(&mut current, LineBuilder::new()).finish_trimmed();
            out.push((line, current_offset));
            current_offset = input_consumed;
        }
        // Preserve per-char styles in case a span boundary lands inside a word
        // (e.g. partial emphasis).
        for (i, ch) in word.chars().enumerate() {
            let style = style_index.style_at(input_consumed + i);
            current.push_char(ch, style);
        }
        input_consumed += word_width;
    }

    if !current.is_empty() {
        out.push((current.finish_trimmed(), current_offset));
    } else if out.is_empty() {
        out.push((LineBuilder::new().finish(), 0));
    }

    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRange {
    pub start: usize,
    pub end: usize,
}

impl PageRange {
    pub fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(self) -> bool {
        self.end <= self.start
    }
}

pub fn page_ranges(line_count: usize, page_height: usize) -> Vec<PageRange> {
    let height = page_height.max(1);
    if line_count == 0 {
        return vec![PageRange { start: 0, end: 0 }];
    }
    let mut pages: Vec<PageRange> = Vec::new();
    let mut start = 0usize;
    while start < line_count {
        let end = (start + height).min(line_count);
        pages.push(PageRange { start, end });
        start = end;
    }
    if pages.is_empty() {
        pages.push(PageRange { start: 0, end: 0 });
    }
    pages
}

pub fn page_index_for_line(line_idx: usize, page_height: usize) -> usize {
    let h = page_height.max(1);
    line_idx / h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paragraphs_from_text(text: &str) -> Vec<StyledLine> {
        text.split('\n').map(StyledLine::from_plain).collect()
    }

    fn plain_lines(out: &LaidOut) -> Vec<String> {
        out.lines.iter().map(StyledLine::plain_text).collect()
    }

    #[test]
    fn lay_out_wraps_long_lines_to_width() {
        let l = lay_out(&paragraphs_from_text("hello world goodbye sky"), 11);
        assert_eq!(plain_lines(&l), vec!["hello world", "goodbye sky"]);
        assert_eq!(l.line_offset(0), 0);
        assert_eq!(l.line_offset(1), 12);
    }

    #[test]
    fn lay_out_preserves_blank_lines() {
        let l = lay_out(&paragraphs_from_text("a\n\nb"), 10);
        assert_eq!(plain_lines(&l), vec!["a", "", "b"]);
        assert_eq!(l.offsets, vec![0, 2, 3]);
    }

    #[test]
    fn lay_out_handles_very_long_words_by_splitting() {
        let l = lay_out(&paragraphs_from_text("aaaaaaaaaa"), 4);
        assert_eq!(plain_lines(&l), vec!["aaaa", "aaaa", "aa"]);
    }

    #[test]
    fn page_ranges_chunks_lines() {
        let pages = page_ranges(7, 3);
        assert_eq!(
            pages,
            vec![
                PageRange { start: 0, end: 3 },
                PageRange { start: 3, end: 6 },
                PageRange { start: 6, end: 7 },
            ]
        );
    }

    #[test]
    fn page_ranges_handles_empty_content() {
        let pages = page_ranges(0, 5);
        assert_eq!(pages, vec![PageRange { start: 0, end: 0 }]);
    }

    #[test]
    fn page_index_for_line_groups_by_height() {
        assert_eq!(page_index_for_line(0, 3), 0);
        assert_eq!(page_index_for_line(2, 3), 0);
        assert_eq!(page_index_for_line(3, 3), 1);
        assert_eq!(page_index_for_line(6, 3), 2);
    }

    #[test]
    fn line_for_offset_finds_nearest_floor() {
        let l = lay_out(&paragraphs_from_text("hello world goodbye sky"), 11);
        assert_eq!(l.line_for_offset(0), 0);
        assert_eq!(l.line_for_offset(5), 0);
        assert_eq!(l.line_for_offset(12), 1);
        assert_eq!(l.line_for_offset(20), 1);
    }

    #[test]
    fn lay_out_preserves_span_styles_across_wrap() {
        let bold = TextStyle {
            bold: true,
            ..TextStyle::PLAIN
        };
        let paragraph = StyledLine {
            spans: vec![
                StyledSpan::plain("hello "),
                StyledSpan::styled("brave new", bold),
                StyledSpan::plain(" world"),
            ],
        };
        // Source: "hello brave new world" (21 chars). Wrap at 11 chars:
        // "hello brave" / "new world"
        let l = lay_out(&[paragraph], 11);
        assert_eq!(l.lines.len(), 2);
        assert_eq!(l.lines[0].plain_text(), "hello brave");
        // The first wrapped line carries both plain and bold spans.
        let bolded: String = l.lines[0]
            .spans
            .iter()
            .filter(|s| s.style.bold)
            .map(|s| s.text.clone())
            .collect();
        assert_eq!(bolded.trim(), "brave");
        // Second line keeps the bold prefix and plain suffix.
        let bolded2: String = l.lines[1]
            .spans
            .iter()
            .filter(|s| s.style.bold)
            .map(|s| s.text.clone())
            .collect();
        assert_eq!(bolded2.trim(), "new");
    }
}
