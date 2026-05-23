CREATE TABLE books (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  title       TEXT NOT NULL,
  author      TEXT,
  format      TEXT NOT NULL,
  file_path   TEXT NOT NULL,
  added_at    TEXT NOT NULL DEFAULT (datetime('now'))
) STRICT;

CREATE INDEX books_title_idx  ON books(title);
CREATE INDEX books_author_idx ON books(author);
