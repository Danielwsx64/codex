use std::collections::HashMap;

use crate::catalog::books::{Book, EmbedStatus};
use crate::matching::normalize_key;

// Why a particular copy was elected as the one to remove. The strongest signal
// wins: a byte/content-identical copy is the safest to drop, so a hash-linked
// group always reports `IdenticalHash`; otherwise the elected copy lost on
// metadata completeness, and failing that on age.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionReason {
    IdenticalHash,
    FewerMetadata,
    Older,
}

impl SuggestionReason {
    pub fn label(self) -> &'static str {
        match self {
            SuggestionReason::IdenticalHash => "identical hash",
            SuggestionReason::FewerMetadata => "fewer metadata",
            SuggestionReason::Older => "older",
        }
    }

    pub fn json_slug(self) -> &'static str {
        match self {
            SuggestionReason::IdenticalHash => "identical_hash",
            SuggestionReason::FewerMetadata => "fewer_metadata",
            SuggestionReason::Older => "older",
        }
    }
}

// Which detection signals are combined into the grouping (mirrors `--by`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectBy {
    Hash,
    Meta,
    All,
}

impl DetectBy {
    fn uses_hash(self) -> bool {
        matches!(self, DetectBy::Hash | DetectBy::All)
    }

    fn uses_meta(self) -> bool {
        matches!(self, DetectBy::Meta | DetectBy::All)
    }
}

// A connected component of two or more books judged to be duplicates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateGroup {
    pub members: Vec<i64>,
    pub suggested: i64,
    pub reason: SuggestionReason,
    pub linked_by_hash: bool,
    pub linked_by_meta: bool,
}

// Counts the curated metadata fields a book has filled in. Mandatory fields
// (title/format/file_path) don't count; empty or whitespace-only strings and
// empty tag sets read as absent.
pub fn completeness_score(book: &Book) -> u32 {
    let mut score = 0;
    let present = |opt: &Option<String>| {
        if opt.as_deref().map(str::trim).is_some_and(|s| !s.is_empty()) {
            1
        } else {
            0
        }
    };
    score += present(&book.author);
    score += present(&book.description);
    score += present(&book.isbn);
    score += present(&book.publisher);
    score += present(&book.language);
    score += present(&book.published_date);
    score += present(&book.series_name);
    if !book.tags.is_empty() {
        score += 1;
    }
    if book.rating.is_some() {
        score += 1;
    }
    score
}

// Weakest embed state first, so a `pending` copy loses the age tiebreak to a
// `synced` one when score and `added_at` are equal.
fn embed_ordinal(status: EmbedStatus) -> u8 {
    match status {
        EmbedStatus::Pending => 0,
        EmbedStatus::Unsupported => 1,
        EmbedStatus::Synced => 2,
    }
}

struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        UnionFind {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        let mut root = x;
        while self.parent[root] != root {
            root = self.parent[root];
        }
        // Path compression: point every node on the way to the root.
        let mut cur = x;
        while self.parent[cur] != root {
            let next = self.parent[cur];
            self.parent[cur] = root;
            cur = next;
        }
        root
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return;
        }
        match self.rank[ra].cmp(&self.rank[rb]) {
            std::cmp::Ordering::Less => self.parent[ra] = rb,
            std::cmp::Ordering::Greater => self.parent[rb] = ra,
            std::cmp::Ordering::Equal => {
                self.parent[rb] = ra;
                self.rank[ra] += 1;
            }
        }
    }
}

// Detects duplicate groups by union of the requested signals. `books` is the
// catalog set; `hash_rows` is every `(book_id, hash)` pair from `book_hashes`.
// A group is a connected component of size >= 2; output is deterministic
// (members sorted ascending, groups ordered by smallest member id).
pub fn find_duplicate_groups(
    books: &[Book],
    hash_rows: &[(i64, String)],
    by: DetectBy,
) -> Vec<DuplicateGroup> {
    if books.len() < 2 {
        return Vec::new();
    }

    let index_of: HashMap<i64, usize> = books.iter().enumerate().map(|(i, b)| (b.id, i)).collect();

    let mut uf = UnionFind::new(books.len());

    // Hash edges: every set of books sharing a hash value forms a clique.
    let mut hash_buckets: HashMap<&str, Vec<usize>> = HashMap::new();
    if by.uses_hash() {
        for (book_id, hash) in hash_rows {
            if let Some(&idx) = index_of.get(book_id) {
                hash_buckets.entry(hash.as_str()).or_default().push(idx);
            }
        }
        for members in hash_buckets.values() {
            for pair in members.windows(2) {
                uf.union(pair[0], pair[1]);
            }
        }
    }

    // Metadata edges: books sharing a normalized title+author key.
    let mut meta_buckets: HashMap<String, Vec<usize>> = HashMap::new();
    if by.uses_meta() {
        for (i, b) in books.iter().enumerate() {
            let key = normalize_key(&b.title, b.author.as_deref());
            meta_buckets.entry(key).or_default().push(i);
        }
        for members in meta_buckets.values() {
            for pair in members.windows(2) {
                uf.union(pair[0], pair[1]);
            }
        }
    }

    // Gather components keyed by representative root.
    let mut components: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..books.len() {
        let root = uf.find(i);
        components.entry(root).or_default().push(i);
    }

    let mut groups: Vec<DuplicateGroup> = Vec::new();
    for indices in components.values() {
        if indices.len() < 2 {
            continue;
        }
        let members_set: std::collections::HashSet<usize> = indices.iter().copied().collect();

        // A component is hash-linked if some hash bucket has >= 2 of its members.
        let linked_by_hash = hash_buckets
            .values()
            .any(|bucket| bucket.iter().filter(|i| members_set.contains(i)).count() >= 2);
        // ...and meta-linked if some normalized-key bucket does.
        let linked_by_meta = meta_buckets
            .values()
            .any(|bucket| bucket.iter().filter(|i| members_set.contains(i)).count() >= 2);

        let group_books: Vec<&Book> = indices.iter().map(|&i| &books[i]).collect();
        let (suggested, reason) = elect_suggestion(&group_books, linked_by_hash);

        let mut member_ids: Vec<i64> = group_books.iter().map(|b| b.id).collect();
        member_ids.sort_unstable();

        groups.push(DuplicateGroup {
            members: member_ids,
            suggested,
            reason,
            linked_by_hash,
            linked_by_meta,
        });
    }

    groups.sort_by_key(|g| g.members.first().copied().unwrap_or(i64::MAX));
    groups
}

// Picks the "worst" copy to remove: lowest completeness score, then oldest
// `added_at`, then weakest embed status, then smallest id for determinism.
fn elect_suggestion(group: &[&Book], hash_linked: bool) -> (i64, SuggestionReason) {
    let scores: HashMap<i64, u32> = group
        .iter()
        .map(|b| (b.id, completeness_score(b)))
        .collect();

    let worst = group
        .iter()
        .min_by(|a, b| {
            scores[&a.id]
                .cmp(&scores[&b.id])
                .then_with(|| a.added_at.cmp(&b.added_at))
                .then_with(|| embed_ordinal(a.embed_status).cmp(&embed_ordinal(b.embed_status)))
                .then_with(|| a.id.cmp(&b.id))
        })
        .expect("group is non-empty: find_duplicate_groups only elects on components of size >= 2");

    let max_score = scores.values().copied().max().unwrap_or(0);
    let reason = if hash_linked {
        SuggestionReason::IdenticalHash
    } else if scores[&worst.id] < max_score {
        SuggestionReason::FewerMetadata
    } else {
        SuggestionReason::Older
    };

    (worst.id, reason)
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
            file_path: format!("books/{id}/file.epub"),
            added_at: format!("2024-01-{id:02}T00:00:00Z"),
            description: None,
            series_name: None,
            series_index: None,
            rating: None,
            isbn: None,
            publisher: None,
            language: None,
            published_date: None,
            tags: Vec::new(),
            embed_status: EmbedStatus::Pending,
            embed_synced_at: None,
        }
    }

    #[test]
    fn score_is_zero_for_a_bare_book() {
        assert_eq!(completeness_score(&book(1, "Dune", None)), 0);
    }

    #[test]
    fn score_counts_each_present_curated_field() {
        let mut b = book(1, "Dune", Some("Frank Herbert"));
        b.description = Some("desc".into());
        b.isbn = Some("123".into());
        b.publisher = Some("Ace".into());
        b.language = Some("en".into());
        b.published_date = Some("1965".into());
        b.series_name = Some("Dune".into());
        b.tags = vec!["sci-fi".into()];
        b.rating = Some(5);
        assert_eq!(completeness_score(&b), 9);
    }

    #[test]
    fn blank_strings_and_empty_tags_do_not_count() {
        let mut b = book(1, "Dune", Some("   "));
        b.description = Some("".into());
        b.tags = Vec::new();
        assert_eq!(completeness_score(&b), 0);
    }

    #[test]
    fn singletons_are_not_groups() {
        let books = vec![
            book(1, "Dune", Some("Frank Herbert")),
            book(2, "Solaris", Some("Stanislaw Lem")),
        ];
        assert!(find_duplicate_groups(&books, &[], DetectBy::All).is_empty());
    }

    #[test]
    fn meta_match_groups_different_formats() {
        let mut pdf = book(2, "Dune", Some("Frank Herbert"));
        pdf.format = "pdf".into();
        let books = vec![book(1, "Dune", Some("Frank Herbert")), pdf];

        let groups = find_duplicate_groups(&books, &[], DetectBy::All);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members, vec![1, 2]);
        assert!(groups[0].linked_by_meta);
        assert!(!groups[0].linked_by_hash);
    }

    #[test]
    fn by_hash_ignores_meta_only_links() {
        let books = vec![
            book(1, "Dune", Some("Frank Herbert")),
            book(2, "Dune", Some("Frank Herbert")),
        ];
        assert!(find_duplicate_groups(&books, &[], DetectBy::Hash).is_empty());
    }

    #[test]
    fn by_meta_ignores_hash_only_links() {
        let books = vec![
            book(1, "Dune", Some("Frank Herbert")),
            book(2, "Different", Some("Author")),
        ];
        let hashes = vec![(1, "abc".to_string()), (2, "abc".to_string())];
        assert!(find_duplicate_groups(&books, &hashes, DetectBy::Meta).is_empty());

        let groups = find_duplicate_groups(&books, &hashes, DetectBy::Hash);
        assert_eq!(groups.len(), 1);
        assert!(groups[0].linked_by_hash);
    }

    #[test]
    fn transitive_merge_across_both_signals() {
        // 1~2 by hash, 2~3 by meta => one group of three under `All`.
        let mut b3 = book(3, "Title B", Some("Author B"));
        b3.format = "pdf".into();
        let books = vec![
            book(1, "Title A", Some("Author A")),
            book(2, "Title B", Some("Author B")),
            b3,
        ];
        let hashes = vec![(1, "h".to_string()), (2, "h".to_string())];

        let groups = find_duplicate_groups(&books, &hashes, DetectBy::All);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members, vec![1, 2, 3]);
        assert!(groups[0].linked_by_hash);
        assert!(groups[0].linked_by_meta);
    }

    #[test]
    fn both_hash_kinds_still_collapse_into_one_group() {
        // A book carrying full+content hashes that each match a different book
        // must end up in a single component.
        let books = vec![
            book(1, "A", Some("x")),
            book(2, "B", Some("y")),
            book(3, "C", Some("z")),
        ];
        let hashes = vec![
            (1, "full".to_string()),
            (2, "full".to_string()),
            (1, "content".to_string()),
            (3, "content".to_string()),
        ];
        let groups = find_duplicate_groups(&books, &hashes, DetectBy::Hash);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members, vec![1, 2, 3]);
    }

    #[test]
    fn hash_linked_group_reports_identical_hash() {
        let books = vec![book(1, "Dune", Some("FH")), book(2, "Dune", Some("FH"))];
        let hashes = vec![(1, "h".to_string()), (2, "h".to_string())];
        let groups = find_duplicate_groups(&books, &hashes, DetectBy::All);
        assert_eq!(groups[0].reason, SuggestionReason::IdenticalHash);
    }

    #[test]
    fn fewer_metadata_copy_is_elected() {
        let mut rich = book(1, "Dune", Some("Frank Herbert"));
        rich.isbn = Some("123".into());
        rich.publisher = Some("Ace".into());
        let poor = book(2, "Dune", Some("Frank Herbert"));
        let books = vec![rich, poor];

        let groups = find_duplicate_groups(&books, &[], DetectBy::Meta);
        assert_eq!(groups[0].suggested, 2);
        assert_eq!(groups[0].reason, SuggestionReason::FewerMetadata);
    }

    #[test]
    fn equal_score_breaks_on_older_added_at() {
        // Same score; book 1 is older (added_at 2024-01-01 < 2024-01-02).
        let books = vec![book(1, "Dune", Some("FH")), book(2, "Dune", Some("FH"))];
        let groups = find_duplicate_groups(&books, &[], DetectBy::Meta);
        assert_eq!(groups[0].suggested, 1);
        assert_eq!(groups[0].reason, SuggestionReason::Older);
    }

    #[test]
    fn equal_score_and_age_breaks_on_weaker_embed_status() {
        let mut pending = book(1, "Dune", Some("FH"));
        pending.added_at = "2024-01-01T00:00:00Z".into();
        pending.embed_status = EmbedStatus::Pending;
        let mut synced = book(2, "Dune", Some("FH"));
        synced.added_at = "2024-01-01T00:00:00Z".into();
        synced.embed_status = EmbedStatus::Synced;
        let books = vec![synced, pending];

        let groups = find_duplicate_groups(&books, &[], DetectBy::Meta);
        assert_eq!(groups[0].suggested, 1);
    }

    #[test]
    fn output_is_deterministic() {
        let mut b3 = book(3, "B", Some("y"));
        b3.format = "pdf".into();
        let books = vec![
            book(1, "A", Some("x")),
            book(10, "A", Some("x")),
            book(2, "B", Some("y")),
            b3,
        ];
        let groups = find_duplicate_groups(&books, &[], DetectBy::Meta);
        assert_eq!(groups.len(), 2);
        // Groups ordered by smallest member id; members sorted ascending.
        assert_eq!(groups[0].members, vec![1, 10]);
        assert_eq!(groups[1].members, vec![2, 3]);
    }
}
