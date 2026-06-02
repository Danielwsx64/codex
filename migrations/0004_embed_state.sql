ALTER TABLE books ADD COLUMN embed_status TEXT NOT NULL DEFAULT 'pending'
  CHECK (embed_status IN ('pending','synced','unsupported'));
ALTER TABLE books ADD COLUMN embed_synced_at TEXT;
