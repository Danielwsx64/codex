use std::fmt;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub code: bool,
    /// `Some(level)` (1..=6) marks the span as belonging to a heading line.
    /// Renderers apply a level-dependent style on top of any other flags.
    pub heading_level: Option<u8>,
    pub link: bool,
}

impl TextStyle {
    pub const PLAIN: TextStyle = TextStyle {
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        code: false,
        heading_level: None,
        link: false,
    };

    pub fn is_plain(self) -> bool {
        self == TextStyle::PLAIN
    }

    pub fn with_heading(mut self, level: u8) -> Self {
        self.heading_level = Some(level);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledSpan {
    pub text: String,
    pub style: TextStyle,
}

impl StyledSpan {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: TextStyle::PLAIN,
        }
    }

    pub fn styled(text: impl Into<String>, style: TextStyle) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }

    pub fn char_count(&self) -> usize {
        self.text.chars().count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StyledLine {
    pub spans: Vec<StyledSpan>,
}

impl StyledLine {
    pub fn empty() -> Self {
        Self { spans: Vec::new() }
    }

    pub fn from_plain(text: impl Into<String>) -> Self {
        let text = text.into();
        if text.is_empty() {
            Self::empty()
        } else {
            Self {
                spans: vec![StyledSpan::plain(text)],
            }
        }
    }

    pub fn char_count(&self) -> usize {
        self.spans.iter().map(StyledSpan::char_count).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.spans.iter().all(|s| s.text.is_empty())
    }

    /// Concatenated plain text content, without any styling.
    pub fn plain_text(&self) -> String {
        let mut buf = String::with_capacity(self.char_count());
        for span in &self.spans {
            buf.push_str(&span.text);
        }
        buf
    }

    /// Heading level shared by every non-empty span on the line, if any.
    pub fn heading_level(&self) -> Option<u8> {
        self.spans
            .iter()
            .find_map(|s| s.style.heading_level)
            .filter(|_| self.spans.iter().all(|s| s.style.heading_level.is_some()))
    }
}

impl fmt::Display for StyledLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for span in &self.spans {
            f.write_str(&span.text)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_concatenates_spans() {
        let line = StyledLine {
            spans: vec![
                StyledSpan::plain("hello "),
                StyledSpan::styled(
                    "world",
                    TextStyle {
                        bold: true,
                        ..TextStyle::PLAIN
                    },
                ),
            ],
        };
        assert_eq!(line.plain_text(), "hello world");
        assert_eq!(line.char_count(), 11);
    }

    #[test]
    fn empty_line_reports_zero_chars() {
        assert!(StyledLine::empty().is_empty());
        assert_eq!(StyledLine::empty().char_count(), 0);
    }

    #[test]
    fn heading_level_set_when_every_span_agrees() {
        let line = StyledLine {
            spans: vec![
                StyledSpan::styled("hello ", TextStyle::PLAIN.with_heading(2)),
                StyledSpan::styled("world", TextStyle::PLAIN.with_heading(2)),
            ],
        };
        assert_eq!(line.heading_level(), Some(2));
    }

    #[test]
    fn heading_level_none_when_some_spans_plain() {
        let line = StyledLine {
            spans: vec![
                StyledSpan::styled("hello ", TextStyle::PLAIN.with_heading(2)),
                StyledSpan::plain("world"),
            ],
        };
        assert_eq!(line.heading_level(), None);
    }
}
