ALTER TABLE books ADD COLUMN description    TEXT;
ALTER TABLE books ADD COLUMN series_name    TEXT;
ALTER TABLE books ADD COLUMN series_index   REAL;
ALTER TABLE books ADD COLUMN rating         INTEGER;
ALTER TABLE books ADD COLUMN isbn           TEXT;
ALTER TABLE books ADD COLUMN publisher      TEXT;
ALTER TABLE books ADD COLUMN language       TEXT;
ALTER TABLE books ADD COLUMN published_date TEXT;

CREATE INDEX books_series_idx ON books(series_name);

CREATE TABLE tags (
  id   INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL UNIQUE COLLATE NOCASE
) STRICT;

CREATE TABLE book_tags (
  book_id INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
  tag_id  INTEGER NOT NULL REFERENCES tags(id)  ON DELETE CASCADE,
  PRIMARY KEY (book_id, tag_id)
) STRICT;

CREATE INDEX book_tags_tag_idx ON book_tags(tag_id);
