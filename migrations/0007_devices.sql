CREATE TABLE devices (
  serial       TEXT PRIMARY KEY,
  alias        TEXT,
  last_seen_at TEXT NOT NULL DEFAULT (datetime('now'))
) STRICT;

-- mtime is epoch seconds: a fingerprint of a foreign file compared against
-- fs metadata in the sync fast-path. FAT stores mtime with 2s granularity,
-- so the sync layer must not expect sub-second precision.
CREATE TABLE device_books (
  device_serial TEXT    NOT NULL REFERENCES devices(serial) ON DELETE CASCADE,
  book_id       INTEGER NOT NULL REFERENCES books(id)       ON DELETE CASCADE,
  device_path   TEXT    NOT NULL,
  hash          TEXT    NOT NULL,
  size          INTEGER NOT NULL,
  mtime         INTEGER NOT NULL,
  synced_at     TEXT    NOT NULL DEFAULT (datetime('now')),
  PRIMARY KEY (device_serial, book_id)
) STRICT;

CREATE INDEX device_books_book_idx ON device_books(book_id);
