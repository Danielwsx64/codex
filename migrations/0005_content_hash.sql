CREATE TABLE book_hashes (
  book_id INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
  kind    TEXT NOT NULL CHECK (kind IN ('full','content')),
  hash    TEXT NOT NULL,
  PRIMARY KEY (book_id, kind, hash)
) STRICT;

CREATE INDEX book_hashes_hash_idx ON book_hashes(hash);
