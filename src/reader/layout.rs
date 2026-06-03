use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone)]
pub struct LaidOut {
    pub lines: Vec<String>,
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
}

pub fn lay_out(text: &str, width: usize) -> LaidOut {
    let width = width.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut offsets: Vec<usize> = Vec::new();
    let mut char_cursor: usize = 0;

    for raw_line in text.split('\n') {
        let line_start = char_cursor;
        if raw_line.is_empty() {
            lines.push(String::new());
            offsets.push(line_start);
            char_cursor += 1;
            continue;
        }
        let wrapped = wrap_line(raw_line, width);
        if wrapped.is_empty() {
            lines.push(String::new());
            offsets.push(line_start);
        } else {
            for (text_chunk, rel_offset) in wrapped {
                offsets.push(line_start + rel_offset);
                lines.push(text_chunk);
            }
        }
        char_cursor += raw_line.chars().count() + 1; // include the '\n' separator
    }

    if lines.is_empty() {
        lines.push(String::new());
        offsets.push(0);
    }

    LaidOut { lines, offsets }
}

fn wrap_line(line: &str, width: usize) -> Vec<(String, usize)> {
    if line.chars().count() <= width {
        return vec![(line.to_string(), 0)];
    }

    let mut out: Vec<(String, usize)> = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;
    let mut current_offset = 0usize;
    let mut input_consumed = 0usize;

    let flush = |out: &mut Vec<(String, usize)>,
                 current: &mut String,
                 current_width: &mut usize,
                 current_offset: &mut usize,
                 input_consumed: usize| {
        while current.ends_with(' ') {
            current.pop();
        }
        out.push((std::mem::take(current), *current_offset));
        *current_width = 0;
        *current_offset = input_consumed;
    };

    for word in line.split_word_bounds() {
        let word_width = word.chars().count();
        let is_whitespace = word.chars().all(|c| c.is_whitespace());

        if is_whitespace {
            if current_width + word_width > width {
                if !current.is_empty() {
                    flush(
                        &mut out,
                        &mut current,
                        &mut current_width,
                        &mut current_offset,
                        input_consumed + word_width,
                    );
                }
            } else {
                current.push_str(word);
                current_width += word_width;
            }
            input_consumed += word_width;
            continue;
        }

        if word_width > width {
            if !current.is_empty() {
                flush(
                    &mut out,
                    &mut current,
                    &mut current_width,
                    &mut current_offset,
                    input_consumed,
                );
            }
            for (chars_consumed_in_word, ch) in word.chars().enumerate() {
                if current_width + 1 > width {
                    flush(
                        &mut out,
                        &mut current,
                        &mut current_width,
                        &mut current_offset,
                        input_consumed + chars_consumed_in_word,
                    );
                }
                current.push(ch);
                current_width += 1;
            }
            input_consumed += word_width;
            continue;
        }

        if current_width + word_width > width {
            flush(
                &mut out,
                &mut current,
                &mut current_width,
                &mut current_offset,
                input_consumed,
            );
        }
        current.push_str(word);
        current_width += word_width;
        input_consumed += word_width;
    }

    if !current.is_empty() {
        while current.ends_with(' ') {
            current.pop();
        }
        out.push((current, current_offset));
    } else if out.is_empty() {
        out.push((String::new(), 0));
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

    #[test]
    fn lay_out_wraps_long_lines_to_width() {
        let l = lay_out("hello world goodbye sky", 11);
        assert_eq!(l.lines, vec!["hello world", "goodbye sky"]);
        assert_eq!(l.line_offset(0), 0);
        assert_eq!(l.line_offset(1), 12);
    }

    #[test]
    fn lay_out_preserves_blank_lines() {
        let l = lay_out("a\n\nb", 10);
        assert_eq!(l.lines, vec!["a", "", "b"]);
        assert_eq!(l.offsets, vec![0, 2, 3]);
    }

    #[test]
    fn lay_out_handles_very_long_words_by_splitting() {
        let l = lay_out("aaaaaaaaaa", 4);
        assert_eq!(l.lines, vec!["aaaa", "aaaa", "aa"]);
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
        let l = lay_out("hello world goodbye sky", 11);
        // line 0 starts at 0, line 1 starts at 12.
        assert_eq!(l.line_for_offset(0), 0);
        assert_eq!(l.line_for_offset(5), 0);
        assert_eq!(l.line_for_offset(12), 1);
        assert_eq!(l.line_for_offset(20), 1);
    }
}
