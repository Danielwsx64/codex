use std::collections::HashMap;

use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

use crate::catalog::books::Book;

// Normalize a title/author pair into a comparison key for matching catalog
// books against files on a device. The match is intentionally exact-after-
// normalization (no fuzzy matching): casefold + NFKD + drop combining marks so
// "Café" == "Cafe", then collapse every run of non-alphanumerics to a single
// space. Title and author are joined with a unit separator so a title-only file
// never collides with a same-titled book that also carries an author.
pub fn normalize_key(title: &str, author: Option<&str>) -> String {
    let title = fold(title);
    let author = author.map(fold).unwrap_or_default();
    format!("{title}\u{1f}{author}")
}

fn fold(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pending_space = false;
    for ch in input.nfkd() {
        // Drop the combining marks NFKD split off (the diacritic itself) without
        // treating them as a word break — "café" must fold to "cafe", not "caf e".
        if is_combining_mark(ch) {
            continue;
        }
        if ch.is_alphanumeric() {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.extend(ch.to_lowercase());
        } else {
            pending_space = true;
        }
    }
    out
}

// All catalog books indexed by their normalized title+author key. A key can map
// to several books (e.g. two editions sharing title and author); that real
// ambiguity is surfaced to the caller rather than resolved here.
pub struct MatchIndex {
    by_key: HashMap<String, Vec<i64>>,
}

impl MatchIndex {
    pub fn build(books: &[Book]) -> Self {
        let mut by_key: HashMap<String, Vec<i64>> = HashMap::new();
        for book in books {
            let key = normalize_key(&book.title, book.author.as_deref());
            by_key.entry(key).or_default().push(book.id);
        }
        MatchIndex { by_key }
    }

    pub fn lookup(&self, key: &str) -> &[i64] {
        self.by_key.get(key).map(Vec::as_slice).unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(id: i64, title: &str, author: Option<&str>) -> Book {
        Book {
            id,
            title: title.to_string(),
            author: author.map(str::to_string),
            format: "epub".to_string(),
            file_path: String::new(),
            added_at: String::new(),
            description: None,
            series_name: None,
            series_index: None,
            rating: None,
            isbn: None,
            publisher: None,
            language: None,
            published_date: None,
            tags: Vec::new(),
            embed_status: crate::catalog::books::EmbedStatus::Pending,
            embed_synced_at: None,
        }
    }

    #[test]
    fn diacritics_are_folded_away() {
        assert_eq!(
            normalize_key("Café", Some("Áuthor")),
            normalize_key("Cafe", Some("Author"))
        );
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert_eq!(
            normalize_key("The Hobbit", Some("J.R.R. Tolkien")),
            normalize_key("THE HOBBIT", Some("j.r.r. tolkien"))
        );
    }

    #[test]
    fn punctuation_and_whitespace_collapse() {
        // Leading/trailing/repeated separators vanish; interior runs become one space.
        assert_eq!(normalize_key("  War  &  Peace! ", None), "war peace\u{1f}");
    }

    #[test]
    fn title_only_does_not_collide_with_authored_book() {
        assert_ne!(
            normalize_key("Dune", None),
            normalize_key("Dune", Some("Frank Herbert"))
        );
    }

    #[test]
    fn lookup_returns_zero_one_or_many_ids() {
        let books = vec![
            book(1, "Dune", Some("Frank Herbert")),
            book(2, "Dune", Some("Frank Herbert")),
            book(3, "Solaris", Some("Stanislaw Lém")),
        ];
        let index = MatchIndex::build(&books);

        assert!(index.lookup(&normalize_key("Nonexistent", None)).is_empty());

        // Fold applies on lookup too: the decomposable "é" matches plain "e".
        assert_eq!(
            index.lookup(&normalize_key("Solaris", Some("Stanislaw Lem"))),
            &[3]
        );

        let mut dune = index
            .lookup(&normalize_key("dune", Some("FRANK HERBERT")))
            .to_vec();
        dune.sort_unstable();
        assert_eq!(dune, vec![1, 2]);
    }
}
