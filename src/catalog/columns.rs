use ratatui::layout::Constraint;

use crate::catalog::books::Book;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryColumn {
    Id,
    Title,
    Author,
    Tags,
    Series,
    Rating,
    Publisher,
    Language,
    Published,
    Isbn,
    Format,
    Embed,
}

impl LibraryColumn {
    pub const ALL: &'static [LibraryColumn] = &[
        LibraryColumn::Id,
        LibraryColumn::Title,
        LibraryColumn::Author,
        LibraryColumn::Tags,
        LibraryColumn::Series,
        LibraryColumn::Rating,
        LibraryColumn::Publisher,
        LibraryColumn::Language,
        LibraryColumn::Published,
        LibraryColumn::Isbn,
        LibraryColumn::Format,
        LibraryColumn::Embed,
    ];

    pub const DEFAULT: &'static [LibraryColumn] = &[
        LibraryColumn::Id,
        LibraryColumn::Title,
        LibraryColumn::Author,
        LibraryColumn::Tags,
        LibraryColumn::Format,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            LibraryColumn::Id => "id",
            LibraryColumn::Title => "title",
            LibraryColumn::Author => "author",
            LibraryColumn::Tags => "tags",
            LibraryColumn::Series => "series",
            LibraryColumn::Rating => "rating",
            LibraryColumn::Publisher => "publisher",
            LibraryColumn::Language => "language",
            LibraryColumn::Published => "published",
            LibraryColumn::Isbn => "isbn",
            LibraryColumn::Format => "format",
            LibraryColumn::Embed => "embed",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|c| c.slug() == s)
    }

    pub fn header(self) -> &'static str {
        self.slug()
    }

    pub fn width(self) -> Constraint {
        match self {
            LibraryColumn::Id => Constraint::Length(5),
            LibraryColumn::Title => Constraint::Min(20),
            LibraryColumn::Author => Constraint::Length(20),
            LibraryColumn::Tags => Constraint::Length(20),
            LibraryColumn::Series => Constraint::Length(20),
            LibraryColumn::Rating => Constraint::Length(14),
            LibraryColumn::Publisher => Constraint::Length(16),
            LibraryColumn::Language => Constraint::Length(8),
            LibraryColumn::Published => Constraint::Length(12),
            LibraryColumn::Isbn => Constraint::Length(15),
            LibraryColumn::Format => Constraint::Length(6),
            LibraryColumn::Embed => Constraint::Length(12),
        }
    }

    pub fn render(self, book: &Book) -> String {
        match self {
            LibraryColumn::Id => book.id.to_string(),
            LibraryColumn::Title => book.title.clone(),
            LibraryColumn::Author => book
                .author
                .clone()
                .unwrap_or_else(|| "(unknown)".to_string()),
            LibraryColumn::Tags => book.tags.join(", "),
            LibraryColumn::Format => book.format.clone(),
            LibraryColumn::Series => match (&book.series_name, book.series_index) {
                (Some(s), Some(i)) => format!("{s} #{}", format_index(i)),
                (Some(s), None) => s.clone(),
                _ => String::new(),
            },
            LibraryColumn::Rating => match book.rating {
                Some(0) | None => "☆☆☆☆☆".to_string(),
                Some(r) => {
                    let v = r.min(5) as usize;
                    let mut s = String::new();
                    for i in 0..5 {
                        s.push(if i < v { '★' } else { '☆' });
                    }
                    format!("{s} {r}/5")
                }
            },
            LibraryColumn::Publisher => book.publisher.clone().unwrap_or_default(),
            LibraryColumn::Language => book.language.clone().unwrap_or_default(),
            LibraryColumn::Published => book.published_date.clone().unwrap_or_default(),
            LibraryColumn::Isbn => book.isbn.clone().unwrap_or_default(),
            LibraryColumn::Embed => book.embed_status.as_str().to_string(),
        }
    }

    pub fn json_value(self, book: &Book) -> serde_json::Value {
        use serde_json::Value;
        match self {
            LibraryColumn::Id => Value::Number(book.id.into()),
            LibraryColumn::Title => Value::String(book.title.clone()),
            LibraryColumn::Author => book
                .author
                .as_ref()
                .map(|s| Value::String(s.clone()))
                .unwrap_or(Value::Null),
            LibraryColumn::Tags => {
                Value::Array(book.tags.iter().map(|t| Value::String(t.clone())).collect())
            }
            LibraryColumn::Format => Value::String(book.format.clone()),
            LibraryColumn::Series => match (&book.series_name, book.series_index) {
                (Some(s), Some(i)) => Value::String(format!("{s} #{}", format_index(i))),
                (Some(s), None) => Value::String(s.clone()),
                _ => Value::Null,
            },
            LibraryColumn::Rating => match book.rating {
                Some(r) => Value::Number(r.into()),
                None => Value::Null,
            },
            LibraryColumn::Publisher => book
                .publisher
                .as_ref()
                .map(|s| Value::String(s.clone()))
                .unwrap_or(Value::Null),
            LibraryColumn::Language => book
                .language
                .as_ref()
                .map(|s| Value::String(s.clone()))
                .unwrap_or(Value::Null),
            LibraryColumn::Published => book
                .published_date
                .as_ref()
                .map(|s| Value::String(s.clone()))
                .unwrap_or(Value::Null),
            LibraryColumn::Isbn => book
                .isbn
                .as_ref()
                .map(|s| Value::String(s.clone()))
                .unwrap_or(Value::Null),
            LibraryColumn::Embed => Value::String(book.embed_status.as_str().to_string()),
        }
    }
}

fn format_index(idx: f64) -> String {
    if idx.fract() == 0.0 {
        format!("{}", idx as i64)
    } else {
        format!("{idx}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_roundtrip_covers_every_variant() {
        for c in LibraryColumn::ALL.iter().copied() {
            let s = c.slug();
            let parsed = LibraryColumn::from_slug(s);
            assert_eq!(parsed, Some(c), "round-trip failed for {s}");
        }
    }

    #[test]
    fn from_slug_unknown_is_none() {
        assert!(LibraryColumn::from_slug("nope").is_none());
        assert!(LibraryColumn::from_slug("").is_none());
    }

    #[test]
    fn default_columns_subset_of_all() {
        for c in LibraryColumn::DEFAULT {
            assert!(LibraryColumn::ALL.contains(c));
        }
    }
}
