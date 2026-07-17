#!/usr/bin/env bash
#
# seed.sh — populate a throwaway cdx catalog with fake books for local
# TUI/GUI testing. Generates real .md files (so the reader works) plus a wide
# spread of metadata that lights up every TUI screen: Library, Groups/folders,
# Duplicates, Devices, and reading progress.
#
# Everything lands in an isolated data dir (default ./.seed), passed to cdx via
# the global --data-dir flag, so it never touches your real ~/.config/cdx
# registry. Wipe it any time with `make seed-clean`.
#
# Usage:
#   ./scripts/seed.sh                 # 24 books into ./.seed
#   SEED_DATA_DIR=/tmp/cdx ./scripts/seed.sh
#   BOOK_COUNT=40 ./scripts/seed.sh
#
set -euo pipefail

SEED_DATA_DIR="${SEED_DATA_DIR:-./.seed}"
BOOK_COUNT="${BOOK_COUNT:-24}"
CATALOG_NAME="${CATALOG_NAME:-seed-lib}"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CDX="${CDX:-$REPO_ROOT/target/debug/cdx}"
LIB_DIR="$SEED_DATA_DIR/lib"
DB="$LIB_DIR/catalog.db"

die() { echo "seed: $*" >&2; exit 1; }

# Execute a SQL script file against a SQLite DB. Prefers the sqlite3 CLI, falls
# back to python3's stdlib sqlite3 module (no system package needed).
run_sql() {
  # run_sql <db> <sqlfile>
  local db="$1" sqlfile="$2"
  if command -v sqlite3 >/dev/null 2>&1; then
    sqlite3 "$db" <"$sqlfile"
  else
    python3 - "$db" "$sqlfile" <<'PY'
import sqlite3, sys
db, sqlfile = sys.argv[1], sys.argv[2]
con = sqlite3.connect(db)
con.execute("PRAGMA foreign_keys = ON")
with open(sqlfile) as fh:
    con.executescript(fh.read())
con.commit()
con.close()
PY
  fi
}

# --- preconditions -----------------------------------------------------------

if ! command -v sqlite3 >/dev/null 2>&1 && ! command -v python3 >/dev/null 2>&1; then
  die "need either the 'sqlite3' CLI or python3 (with the stdlib sqlite3 module) to write seed data"
fi

if [[ ! -x "$CDX" ]]; then
  echo "seed: building cdx (cargo build)..." >&2
  (cd "$REPO_ROOT" && cargo build) || die "cargo build failed"
fi

# --- fresh catalog -----------------------------------------------------------

echo "seed: resetting $SEED_DATA_DIR" >&2
rm -rf "$SEED_DATA_DIR"

# init creates catalog.db (migrated to latest), books/, registers the catalog,
# and sets it current — exactly like a real `cdx catalog init`.
"$CDX" --data-dir "$SEED_DATA_DIR" catalog init "$CATALOG_NAME" "$LIB_DIR" \
  --description "Seeded catalog for local TUI testing" >/dev/null

# --- categorical pools -------------------------------------------------------

AUTHORS=("Brandon Sanderson" "Ursula K. Le Guin" "Terry Pratchett" "N. K. Jemisin" "Andy Weir" "Octavia Butler")
PUBLISHERS=("Tor Books" "Orbit" "Gollancz" "Del Rey")
LANGUAGES=("en" "pt" "es" "fr")
SERIES_NAMES=("The Stormlight Archive" "Earthsea Cycle" "Discworld")
STUB_FORMATS=("epub" "pdf" "mobi" "azw3" "epub" "pdf")
EMBED_STATES=("pending" "synced" "unsupported")
TAG_NAMES=("fantasy" "sci-fi" "classic" "programming" "history" "poetry" "unread" "favorite")

# --- markdown generator ------------------------------------------------------

write_md() {
  # write_md <id> <title>
  local id="$1" title="$2"
  local dir="$LIB_DIR/books/$id"
  mkdir -p "$dir"
  cat >"$dir/book_$id.md" <<EOF
# $title

## Chapter 1 — The Beginning

Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor
incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis
nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.

## Chapter 2 — The Middle

Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu
fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in
culpa qui officia deserunt mollit anim id est laborum.

## Chapter 3 — The End

Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium
doloremque laudantium, totam rem aperiam, eaque ipsa quae ab illo inventore.
EOF
}

# --- build the SQL batch -----------------------------------------------------

SQL="$(mktemp)"
trap 'rm -f "$SQL"' EXIT

{
  echo "PRAGMA foreign_keys = ON;"
  echo "BEGIN;"

  # tags: explicit ids 1..N so book_tags can reference them.
  for i in "${!TAG_NAMES[@]}"; do
    printf "INSERT INTO tags(id, name) VALUES (%d, '%s');\n" "$((i + 1))" "${TAG_NAMES[$i]}"
  done

  # books
  n_md=$((BOOK_COUNT * 3 / 4)) # ~75% real .md files, rest are format stubs
  for ((id = 1; id <= BOOK_COUNT; id++)); do
    idx=$((id - 1))

    # -- duplicate special cases -------------------------------------------
    # ids 5 & 6: identical title+author  -> dedup --by meta
    # ids 8 & 9: identical content hash   -> dedup --by hash (added below)
    if [[ $id -eq 6 ]]; then
      title="Elantris"
      author="Brandon Sanderson"
    elif [[ $id -eq 5 ]]; then
      title="Elantris"
      author="Brandon Sanderson"
    else
      title="Seed Book $(printf '%02d' "$id")"
      # leave a couple of books without an author (catch-all group)
      if [[ $((id % 7)) -eq 0 ]]; then
        author=""
      else
        author="${AUTHORS[$((idx % ${#AUTHORS[@]}))]}"
      fi
    fi

    # -- format + file -----------------------------------------------------
    if [[ $id -le $n_md ]]; then
      format="md"
      write_md "$id" "$title"
      file_path="books/$id/book_$id.md"
    else
      format="${STUB_FORMATS[$(((id - n_md - 1) % ${#STUB_FORMATS[@]}))]}"
      file_path="" # stub row: browses fine, FileMissing on open
    fi

    # -- varied metadata (NULLs sprinkled for catch-all folders) ----------
    publisher="${PUBLISHERS[$((idx % ${#PUBLISHERS[@]}))]}"
    language="${LANGUAGES[$((idx % ${#LANGUAGES[@]}))]}"
    embed="${EMBED_STATES[$((idx % ${#EMBED_STATES[@]}))]}"

    if [[ $((id % 4)) -eq 0 ]]; then
      rating="NULL"
    else
      rating=$(((id % 5) + 1))
    fi

    if [[ $((id % 3)) -eq 0 ]]; then
      series_name="NULL"
      series_index="NULL"
    else
      series_name="'${SERIES_NAMES[$((idx % ${#SERIES_NAMES[@]}))]}'"
      series_index="$(((id % 4) + 1)).0"
    fi

    author_sql="NULL"
    [[ -n "$author" ]] && author_sql="'$author'"

    year=$((2000 + (id % 24)))
    isbn="978-0-$(printf '%09d' "$id")"

    printf "INSERT INTO books(id, title, author, format, file_path, description, series_name, series_index, rating, isbn, publisher, language, published_date, embed_status) VALUES (%d, '%s', %s, '%s', '%s', '%s', %s, %s, %s, '%s', '%s', '%s', '%s', '%s');\n" \
      "$id" "$title" "$author_sql" "$format" "$file_path" \
      "A seeded description for $title." \
      "$series_name" "$series_index" "$rating" \
      "$isbn" \
      "$publisher" "$language" "${year}-01-15" \
      "$embed"

    # -- tags: attach 0..3, many-to-many so some books span tag folders ----
    ntags=$((id % 4))
    for ((t = 0; t < ntags; t++)); do
      tag_id=$((((id + t) % ${#TAG_NAMES[@]}) + 1))
      printf "INSERT OR IGNORE INTO book_tags(book_id, tag_id) VALUES (%d, %d);\n" "$id" "$tag_id"
    done
  done

  # -- duplicate signals ---------------------------------------------------
  # hash duplicate: two distinct books share the same content hash.
  echo "INSERT INTO book_hashes(book_id, kind, hash) VALUES (8, 'content', 'seed-dup-content-hash');"
  echo "INSERT INTO book_hashes(book_id, kind, hash) VALUES (9, 'content', 'seed-dup-content-hash');"
  # give every book a unique 'full' hash too, so they otherwise look distinct.
  for ((id = 1; id <= BOOK_COUNT; id++)); do
    printf "INSERT INTO book_hashes(book_id, kind, hash) VALUES (%d, 'full', 'seed-full-%d');\n" "$id" "$id"
  done

  # -- devices + current-device pointer ------------------------------------
  echo "INSERT INTO devices(serial, alias) VALUES ('KINDLE-SEED-0001', 'Kindle');"
  echo "INSERT INTO devices(serial, alias) VALUES ('KOBO-SEED-0002', 'Kobo');"
  # link the first few books to the Kindle (epoch mtime, 2s FAT granularity).
  for id in 1 2 3 4; do
    printf "INSERT INTO device_books(device_serial, book_id, device_path, hash, size, mtime) VALUES ('KINDLE-SEED-0001', %d, 'documents/book_%d.md', 'seed-full-%d', %d, 1700000000);\n" \
      "$id" "$id" "$id" "$((10000 + id * 137))"
  done
  echo "INSERT INTO settings(key, value) VALUES ('device.current', 'KINDLE-SEED-0001');"

  # -- reading progress on a few md books ----------------------------------
  echo "UPDATE books SET last_chapter = 1, last_offset = 420, last_read_at = datetime('now', '-2 days') WHERE id = 1;"
  echo "UPDATE books SET last_chapter = 2, last_offset = 88,  last_read_at = datetime('now', '-1 day')  WHERE id = 2;"
  echo "UPDATE books SET last_chapter = 0, last_offset = 12,  last_read_at = datetime('now')            WHERE id = 4;"

  echo "COMMIT;"
} >"$SQL"

run_sql "$DB" "$SQL"

# --- summary -----------------------------------------------------------------

echo "seed: done — $BOOK_COUNT books in $LIB_DIR" >&2
echo "seed:   md files : $n_md (reader works) | stubs: $((BOOK_COUNT - n_md)) (browse only)" >&2
echo "seed:   dupes    : ids 5&6 (meta), ids 8&9 (hash)" >&2
echo "seed:   devices  : Kindle (current, 4 books), Kobo" >&2
echo "" >&2
echo "Launch the TUI against the seeded catalog:" >&2
echo "  $CDX --data-dir $SEED_DATA_DIR tui" >&2
echo "Or inspect from the CLI, e.g.:" >&2
echo "  $CDX --data-dir $SEED_DATA_DIR ls" >&2
echo "  $CDX --data-dir $SEED_DATA_DIR groups --by author" >&2
echo "  $CDX --data-dir $SEED_DATA_DIR dedup --by all" >&2
