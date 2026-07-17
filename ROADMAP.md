# Roadmap

Format: versioned milestones. Each release is a closed scope; once every
check is ticked, the version ships and the next one opens.

The milestones marked `[x]` (v0.1–v1.0) are delivered and make up the
feature set of **v1.0 — Stable**. Everything still open has been moved to
the end (**Post-1.0 — Future** and **Backlog**).

## Principle: CLI ↔ TUI parity

Every user-facing feature gets **two interfaces in the same milestone**:
the CLI subcommand (`cdx <verb>`) and the equivalent screen inside the TUI
(`cdx tui`). Both paths consume the same domain module — the divergence is
confined to the presentation layer. So when the items below list only the
CLI verb, they also imply the corresponding TUI screen.

Exception (rare, always justified):

- Book reader (v0.9) — only makes sense in the TUI.

## TUI: global navigation

The TUI's opening screen (the same welcome reused from the shared module)
is the entry point and lists the **top-level sections** as navigable links
(↑/↓ move through the list, Enter enters). Each section maps to a coherent
set of CLI verbs:

1. **Library** — list/view/remove books (`cdx ls`, `cdx show`, `cdx rm`)
   [v0.1]
2. **Search** — full-text search + filters (`cdx search`) [v0.3]
3. **Catalogs** — catalog registry (`cdx catalog ls`/`use`/`rm` + the
   `init`/`add` wizard) [v0.1]
4. **Devices** — sync with ereaders (`cdx device ls`, `device books`,
   `device alias`, `push`, `pull`, `sync`) [v0.4]

Sections of future milestones appear in the list with a "(v0.X)" suffix and
stay disabled (Enter over them does not navigate) until they ship in the
corresponding milestone.

**Global shortcut — command palette via `:`**: from any screen, `:` opens an
input in the footer (vim-style). Available commands: `:library`,
`:catalogs`, `:search`, `:devices`, plus `:quit` (alias of `q`, vim
convention — not a rebind of exit, just an alternative form). Tab completes
by the shortest unique prefix (`:l`, `:c`, `:s`, `:d`, `:q`). Enter runs;
Esc cancels and returns focus to the active screen.

Constraints:

- The reserved keys (`q`, `Ctrl+C` to exit; `Esc`, `Enter` for in-screen
  navigation) still hold. The palette only captures text while it is open —
  outside it, `q` remains the immediate exit.
- The palette **does not replace** the contextual help `?`, which stays
  per-screen documenting the active screen's shortcuts.

## v0.1 — Catalog MVP

Independent local catalog, no device sync, no Calibre. The user chooses
where each catalog lives (any path — it can be a git repo); cdx keeps a
multi-catalog registry at `$XDG_CONFIG_HOME/cdx/config.toml` with the
"current" catalog.

- [x] Define the initial catalog schema (SQLite — `books` table)
- [x] `cdx catalog init <name> <path>` — create DB + `books/` at the path
      and register the catalog
- [x] `cdx catalog add <name> <path>` — register an existing catalog at the
      given path
- [x] `cdx catalog ls` — list registered catalogs (mark the current one and
      `(missing)` when the path disappeared from disk)
- [x] `cdx catalog use <name>` — switch the current catalog
- [x] `cdx catalog rm <name>` — remove from the registry (`--purge` flag to
      delete the files)
- [x] TUI: "Catalogs" screen — list catalogs (current one marked,
      `(missing)` when the path disappeared), allow `use` (Enter) and `rm`
      (with confirmation + option to purge). The welcome is always the home;
      the Catalogs screen is reached via the menu or `:catalogs`.
- [x] TUI: "New catalog" wizard — a single flow covering `init` (creates
      DB + `books/`) and `add` (registers an existing path), with name,
      path, and optional description
- [x] TUI: extend the welcome with a menu of the 4 top-level sections
      (Library and Catalogs active; Search "(v0.3)" and Devices "(v0.4)"
      disabled until their milestones)
- [x] TUI: command palette `:` — footer overlay with input + tab-complete;
      registers `:library` (stub if the Library screen is not ready yet),
      `:catalogs`, `:quit`/`:q`; the other sections register in their
      milestones
- [x] `cdx add <file>...` — import EPUB/PDF/MOBI/AZW3, extract basic
      metadata, and rename the stored file to a sanitized
      `Author_-_Title.ext`; formats outside the list are refused with a
      clear message
- [x] `cdx ls` — list books (id, title, author, format)
- [x] `cdx inspect <id|title>` — show detailed metadata; accepts a numeric
      id or an exact (case-insensitive) title; an ambiguous title returns an
      error listing the candidate ids
    - [ ] dynamic name/id autocomplete in the shell — deferred (see
          "Deferred extras" under Post-1.0; `clap_complete` `unstable-dynamic`)
- [x] `cdx rm <id|title>` — remove from the catalog and delete the file;
      `--keep` moves the file to the cwd instead of deleting (suffixes `.1`,
      `.2` on collision)
- [x] Logging configurable via `RUST_LOG` (`tracing-subscriber` reads
      `RUST_LOG`; `-v/-vv/-vvv` adjusts the default without needing to
      export it)
- [x] Welcome screen in a shared module, shown when `cdx` runs with no
      subcommand (same content is reused by the TUI)
- [x] `cdx tui` — ratatui skeleton + welcome screen reusing the shared
      module (proves the CLI↔TUI cycle; the other screens land alongside
      their respective commands in the following milestones)

## v0.2 — Metadata editing

Embed cycle: any edit (`cdx edit` or TUI `e`) marks the book as
`embed_status = 'pending'`; the sync (`cdx embed sync`, TUI `w` or
`Ctrl+W`) embeds it into the file and marks `synced` (EPUB/PDF) or
`unsupported` (MOBI/AZW3, not retryable).

- [x] `cdx edit <id>` — open `$EDITOR` with the metadata as TOML; validate
      on parse and reuse `handle_update` (which resets `embed_status` to
      `pending`); the tempfile is preserved on error
- [x] `cdx tag <id> <tag>...` / `cdx untag <id> <tag>... [--all]` — a
      "Tags" field in the TUI edit modal (multi, comma-separated); a "tags"
      column in `cdx ls` human and JSON; `embed_status` returns to
      `pending` only when the set changes; `--all` on `untag` clears all
      tags
- [x] `cdx rate <id> <0-5>` — TUI: a "Rating" field in the modal (validated
      0–5); the CLI accepts 0–5 and treats `0` as "clear"
- [x] `cdx series <id> <name> [--index N]` — TUI: "Series" + "Index" fields
      in the modal; the CLI has `--clear` to remove it (without `<name>`)
- [x] `cdx embed sync` — embed metadata into every `pending` book; print
      line-by-line progress + a final summary
- [x] TUI: embed metadata into the file (EPUB/PDF) via the `w` key on
      Inspect — MOBI/AZW3 returns status "embed not supported"
- [x] Migration `0002_metadata.sql` — `description`, `series_name`,
      `series_index`, `rating`, `isbn`, `publisher`, `language`,
      `published_date` columns on `books`; `tags` + `book_tags` tables
- [x] Migration `0004_embed_state.sql` — `embed_status` +
      `embed_synced_at` columns on `books`
- [x] Migration `0005_content_hash.sql` + dedup in `cdx add` — a SHA-256
      fingerprint per book (`book_hashes` table); EPUB gets a stable content
      hash (ignoring the OPF rewritten by embedding), other formats use the
      file hash + an accumulated post-embed hash; a duplicate is skipped
      with a warning, `--force` re-imports; best-effort backfill of existing
      books
- [x] Extended extraction in `cdx add` (EPUB/MOBI/PDF) to populate the new
      fields when available in the file

## v0.3 — Search and filters

- [x] `cdx search <query>` — case-insensitive substring on
      title/author/tags (whitespace = AND tokens; reuses the `ls` renderer
      for human and JSONL)
- [x] `--author`, `--tag`, `--series`, `--rating` flags
- [x] `--json` output to compose with `jq`/scripts
- [x] TUI: register `:search` in the command palette + activate the
      "Search" link on the welcome — becomes the "filtered mode" of the
      Library screen: `/` filters by text (AND tokens, as in the CLI) and
      `:search` opens the wizard with text/author/tag/series/rating fields;
      Esc clears the filter

## v0.4 — Kindle sync (USB)

Sync with ereaders mounted over USB, supporting multiple simultaneous
devices. Each device is identified by its stable serial (read from the USB
descriptor via sysfs) and can get an **alias** — the alias is what the
commands use day-to-day.

A book's identity between catalog and device has two layers:

1. **Exact** — the sync-state table (`device_books`) records every book cdx
   sent/pulled (book_id ↔ path on the device + SHA-256 checksum +
   size/mtime). For these, there is no guessing.
2. **By metadata** — for files that arrived on the device by other means,
   the match is **normalized title + author** (casefold, NFKD without
   diacritics, punctuation and whitespace collapsed), never the file: the
   format may vary between ends (local EPUB vs AZW3 on the device) and a
   hash does not survive conversion. Small variations ("Café" vs "Cafe")
   match; no fuzzy matching — a genuine ambiguity (two candidates for the
   same match) never resolves itself, it becomes a conflict for manual
   decision.

- [x] Detect Kindles mounted via USB mass storage (Linux), supporting
      multiple simultaneous devices; the stable identity of each device is
      the **serial** read from the USB descriptor in sysfs (`idVendor` 1949
      = Amazon/Lab126 is the gate; `documents/` + `system/` on the mount are
      just a sanity check). On other OSes detection compiles and returns an
      empty list
- [x] Migration `0007_devices.sql` — `devices` table (`serial` PK, `alias`,
      `last_seen_at`) + `device_books` table (sync state: `device_serial`,
      `book_id`, `device_path`, `hash`, `size`, `mtime`, `synced_at`);
      devices live in catalog.db, so the alias is per catalog
- [x] `cdx device ls` — list detected and known devices (alias, serial,
      mount path when connected, free space, book count); human + JSONL
- [x] `cdx device alias <serial|alias> <new-alias>` — set/rename the alias;
      on the first detection of a device without an alias, the serial is
      used as a fallback in listings
- [x] `cdx device books [--device <alias>]` — list a device's books by
      reading file metadata (not just the filename), with the presence
      column ("both" / "device only") via sync state + normalized match;
      human + JSONL
- [x] Device selection: `--device <alias>` flag on `device books`, `push`,
      `pull`, and `sync`. One connected device → implicit default; two or
      more without the flag → a clear error listing the candidates (never
      choosing on its own)
- [x] `cdx push <id|title> [--device <alias>]` — copy a file from the
      catalog to the device and record the sync state (hash/size/mtime);
      without `<id|title>` it opens an interactive picker (arrows/`j``k` +
      Enter) listing the catalog's books
- [x] `cdx pull <path> [--device <alias>]` — import a book from the device
      reusing the `cdx add` pipeline (including hash dedup) and record the
      sync state; without `<path>` it opens an interactive picker
      (arrows/`j``k` + Enter) listing the device's books
- [x] Sync verification: the diff checks each sync-state entry via the
      size + mtime fast path; a divergence marks the book as `modified`
      (a re-push is offered in the plan). `--verify` forces a full SHA-256
      (USB is slow — full hash only on demand). An entry whose file vanished
      from the device becomes `missing`
- [x] `cdx sync [--device <alias>]` — **iterative** bidirectional diff:
      compute the plan (missing on each end, `modified`, `missing`, match
      conflicts) and confirm item by item, `git add -p` style (`y` apply /
      `n` skip / `a` accept the rest / `q` abort). `--dry-run` only prints
      the plan; `--yes` accepts everything (for scripts). Sync **never
      deletes** on either end — it only copies; removal is always manual
- [x] `cdx device clean [--device <alias>]` — remove books from the device.
      Without a target it opens an interactive picker (arrows/`j``k`,
      multi-select, Enter confirms) listing the device's books; `--all`
      clears everything. Deletes the file on the device and removes the
      corresponding `device_books` entry (sync state). **Never touches the
      local catalog** — the removal is only on the device end, materializing
      this milestone's "removal is always manual". Always confirms before
      deleting; `--yes` skips the confirmation (scripts). `--json`
      summarizes what was removed (path + bytes freed)
- [x] TUI: clean action in the device view — Space marks books, confirms and
      deletes (mirrors `cdx device clean`); navigation already resolves the
      device choice without a flag
- [x] TUI: "Devices" screen — list devices (alias, connected or not); `r`
      renames the alias; Enter opens the book view of the selected device
      (navigation resolves the device choice without a flag)
- [x] Current device: a per-catalog pointer (key in `settings`) that becomes
      the implicit `--device` target. It becomes current when there is only
      one connected device and when a device is chosen explicitly (`--device`
      on the CLI or a selection in the TUI), so the "last used" persists
      across runs even with several connected. `cdx device ls` (human +
      JSONL) and the TUI device list mark the current one; `resolve_target`
      uses the current one before falling into the ambiguous case
- [x] TUI: presence indicators in the device view and the Library (when a
      device is connected): each row marks "both" / "local only" / "device
      only" / "modified" via sync state + normalized match, showing each
      end's format when they differ
- [x] TUI: sync flow mirroring the iterative CLI — the plan becomes a list
      with a checkbox per item (Space toggles, `a` all), highlighted
      conflicts require an explicit choice, Enter applies only what is
      checked, line-by-line progress
- [x] TUI: register `:devices` in the command palette + activate the
      "Devices" link on the welcome
- [x] TUI: push from the Library — `p` on the table (and a "Push to device"
      item in the actions menu) copies the selected book to the current
      device after confirmation, reusing `cdx push`; navigation resolves the
      device choice without a flag and the header shows the current connected
      device (alias + ●)

## v0.5 — Curation: duplicates

Detect duplicate books in the current catalog and suggest which copy to
remove. Duplicate signals are combined by **union**: if *any* method flags
a suspicion, the group becomes a candidate.

1. **Content hash** — `book_hashes` (SHA-256 `full`/`content`): catches
   byte-identical copies and the same EPUB before/after embedding.
2. **Normalized title + author** — casefold + NFKD, punctuation/whitespace
   collapsed (the same normalization as the device match): catches the same
   book in different formats/editions (EPUB vs PDF), where the hash does not
   match.

For each group, cdx **suggests deleting** the "worst" copy: fewest
metadata fields filled in (a score over the presence of
author/description/isbn/publisher/language/published_date/series/tags/
rating) and, as a tiebreaker, the most "stale" one (oldest by `added_at` /
weakest embed_status). The final decision is always the user's — cdx only
suggests.

- [x] `cdx dedup` — list the detected duplicate groups (any method),
      marking in each group the copy suggested for removal and the reason
      ("identical hash" / "less metadata" / "older"); human + `--json`
      (JSONL, one object per group)
- [x] `--by hash|meta|all` flag (default `all` = union of the signals) to
      restrict the detection method
- [x] Metadata completeness score — a pure function over `Book` that scores
      the presence of the fields; elects the suggested copy and appears in
      `--json`. A fingerprint backfill ensures the hash method works on old
      books
- [x] Assisted removal — a picker (arrows/`j``k` + Enter, or accept the
      suggestion) that reuses the `cdx rm` path (deletes from the catalog +
      file; `--keep` moves to the cwd); `--yes` accepts all suggestions
      (scripts). Never deletes without confirmation
- [x] TUI: "Duplicates" screen/action in the Library section — list the
      groups, highlight the suggestion and delete with confirmation (mirrors
      `cdx dedup`)

## v0.9 — TUI reader (EPUB + TXT/Markdown)

Reading books directly in the terminal — the only TUI-only feature of the
roadmap (cf. the exception declared in the parity principle). The other TUI
screens are distributed across the earlier milestones, alongside their CLI
commands. No equivalent CLI command: the reader is TUI-only by design.

- [x] EPUB rendering — spine extraction via the `src/epub` module (extends
      what already existed in `src/import/epub.rs`) + HTML→text via
      `html2text`. Reflow recomputed on resize.
- [x] TXT/Markdown rendering — `cdx add` accepts `.txt` and `.md`; Markdown
      via `pulldown-cmark`; TXT by direct reading.
- [x] Pagination by viewport height — `:N` jumps to the book's absolute
      page; `:cN` jumps to chapter N. The footer shows `ch X/Y · pg A/B`.
- [x] Vim-style visual cursor (`h j k l w b e 0 $ gg G`), pagination
      (`Space`, `Ctrl+f`, `Ctrl+b`, `Ctrl+d`, `Ctrl+u`), chapter switching
      (`]`, `[`). `Esc` returns to the Library.
- [x] Persist reading progress — migration `0006_reading_progress` stores
      `last_chapter`, `last_offset`, `last_read_at` on `books`. Saved when
      switching chapters, paging, and on leaving the reader.
- [x] Chapter navigation — `[`/`]` between chapters; `:cN` jumps directly.
      The EPUB TOC (NCX or nav.xhtml) is used to name chapters when
      available.
- [x] `?` opens contextual help with the active screen's keyboard
      shortcuts.

Out of scope for this delivery (deferred):

- Visual selection (`v`), search (`/`, `n`, `N`), bookmarks.
- A navigable TOC modal (the current list stays embedded in the
  footer/help).
- Inline images (Kitty/Sixel) — depends on terminal detection.
- Showing `last_read_at` in `cdx ls` / `cdx inspect`.

## v0.9.1 — Reader: Kindle (MOBI/AZW3)

Extends the reader to the Kindle ecosystem. `cdx add` already accepts
MOBI/AZW3; only the read path in the reader is missing.

- [x] Reader for MOBI via the `mobi` crate (`content_as_string()`, with a
      lossy fallback for CP1252 books); reuses the `html2text` → `layout`
      pipeline from v0.9.
- [x] Reader for AZW3 (KF8) — the container carries two streams (legacy
      MOBI KF7 + KF8). The `mobi` crate **does not** parse KF8: dual-stream
      AZW3 is read via the legacy stream; KF8-only (the typical Calibre
      output) fails with a clear message suggesting conversion to EPUB.
- [x] Detect DRM (Amazon Topaz / KFX / protected AZW) with a clear message
      — **cdx does not remove DRM**. Only sideloaded, DRM-free books work.
- [x] Chapters for MOBI/AZW3 — the crate does not expose the index (INDX),
      so the split is on the MOBI6 `<mbp:pagebreak/>` markers
      (deterministic, titles "Chapter N"); without markers the book becomes
      a single chapter.

Validated sub-formats (limitations of the `mobi` 0.8 crate):

- MOBI6 PalmDOC/uncompressed → reads normally.
- HUFF/CDIC → refused with a clear message (the crate's decoder is not
  reliable; we prefer to refuse rather than render a blank book).
- AZW3 KF8-only → refused with a clear message ("convert it to EPUB").
- Topaz / KFX → detected by magic bytes before the parse, refused.
- Malformed/truncated file → the crate's parser may panic; the reader
  catches it via `catch_unwind` and returns a normal error instead of
  taking down the TUI session.

## v0.9.2 — Reader: PDF

PDF is fixed-layout, fundamentally hostile to terminal reflow.

- [x] Reader for single-column PDF via `pdf-extract` (sequential text
      reused by `layout::lay_out`). Acceptable for most fiction books
      exported as PDF. Each PDF page becomes a chapter ("Page N"), so `:cN`
      jumps by the document's real page. An encrypted PDF is refused with a
      clear message.
- [x] Heuristic to detect multi-column (vertical gaps in separate columns)
      — in multi-column text `pdf-extract` mixes lines between columns.
      Flag it as "best-effort: layout not preserved" and proceed anyway, or
      ask for conversion to EPUB. The "proceed anyway" branch is
      implemented: an italic warning line at the top of each affected page.
- [x] Tables, math formulas, vector images — degrade. Document as a
      limitation. Documented in the reader itself: the best-effort warning
      and the error messages (encrypted, no extractable text) carry the
      limitation to the user.
- [x] **Do not use `pdfium-render`**: it requires a C++ Pdfium runtime,
      which breaks cdx's "single binary" portability. `lopdf` (already a
      dep) is only for metadata; for text, `pdf-extract` is the way.

## v0.9.3 — Reader: conversion cache and async open

Conversion (PDF mostly) is expensive; reopening a book should not pay that
cost again, nor freeze the TUI the first time.

- [x] On-disk cache of the conversion result (PDF/EPUB/MOBI/AZW3) in the
      XDG cache dir (`~/.cache/cdx/<catalog-hash>/<id>.json`); invalidated by
      the source file's mtime + size + schema version. A cache failure never
      breaks the open — silent fallback to conversion. TXT/MD are left out
      (parsing is as fast as reading the cache).
- [x] Conversion runs on a background thread with an animated loading
      screen ("Opening <title>…"); the TUI stays responsive and `Esc`
      cancels the open, returning to the library.

## v0.11 — Group navigation (browse)

Navigate the catalog as if it were a folder tree: one metadata field
becomes the "grouper" and each distinct value becomes a folder. It is a
**mode of the Library screen** (not a new section) — inside a folder the
same columns and the same actions of the normal listing apply.

A folder's scope is **exact equality** (the "Jane Austen" folder contains
only `author = 'Jane Austen'`), unlike the search filter, which is
substring. `author` is a single column (a book falls in one folder); `tags`
is many-to-many (a book appears in several folders).

- [x] `cdx groups --by author|tag|rating` — list the current catalog's
      groups (value + book count), human and `--json` (JSONL, one object per
      group; `value: null` in the catch-all group — no author / no tags / no
      rating). An empty catalog prints nothing in `--json`.
- [x] TUI: grouped mode in the Library — `g` opens the grouper selector
      (Author / Tags / Rating / Off); the "folders" level lists value +
      count (`↑↓`/`jk` navigate, Enter enters). The domain module
      (`catalog::groups`) is shared with the CLI.
- [x] TUI: inside a folder the table reuses the columns and actions of the
      listing (inspect/edit/open/push/delete/columns/embed); a breadcrumb in
      the header shows the current group and the count. `Esc` steps down one
      layer at a time: clear the filter → back to folders → leave grouping →
      back to the welcome.
- [x] TUI: `/` inside a folder filters the group's books in memory, without
      widening the folder's exact scope.
- [x] Group also by `publisher`/`language`/`series`/`format` — reuses the
      exact `books_in_group` path (does not depend on `SearchFilters`); the
      CLI (`cdx groups --by …`) and the TUI's `g` selector gain the new
      options.

## v1.0 — Stable

First stable version: freezes the feature set above and focuses on
**distribution and discovery**. No crates.io package — distribution is via
prebuilt binary (install script + self-update). The milestones above
(v0.1–v0.11) document what already goes into 1.0.

- [x] CI on GitHub Actions: `cargo fmt --check` + `cargo clippy
      --all-targets -- -D warnings` + `cargo test`, on the stable toolchain
      and on MSRV 1.80.
- [x] Release workflow: build the binaries per `v*` tag
      (`x86_64-unknown-linux-musl` static + `aarch64-apple-darwin` +
      `x86_64-apple-darwin`) and automatically create the GitHub Release
      with notes generated from the commits/PRs.
- [x] `install.sh` — detect OS/architecture, download the binary from the
      latest release, verify the SHA-256 checksum, and install to
      `~/.local/bin` (override via `$CDX_INSTALL_DIR`); documented in the
      README and the docs.
- [x] `cdx update` — check the latest release on GitHub and install the new
      version over the current binary (`--check` only reports; `--yes` skips
      the confirmation).
- [x] `cdx completions <shell>` — generate the completion script for
      bash/zsh/fish (and the other `clap_complete` shells).
- [x] Documentation site (mdBook + GitHub Pages) covering installation and
      each feature area.
- [x] README with installation, quickstart, updating, and a link to the
      docs.

## Post-1.0 — Future

Milestones and items that fell outside the 1.0 scope. They land in later
releases according to priority.

### v1.1 — Format conversion

- [ ] `cdx convert <id> --to epub|mobi|azw3` (delegating to Calibre's
      `ebook-convert` if available)
- [ ] Detect the absence of the external dependency with a clear message

### v1.2 — Other ereaders

- [ ] Kobo support (folder structure, local DB)
- [ ] A "device driver" abstraction to ease PocketBook/Boox in the future

### v1.3 — Import / interop

- [ ] `cdx import calibre <path>` — import from an existing Calibre library
      (reads `metadata.db`)
- [ ] Export a cdx catalog in a neutral format (JSON/CSV)

### v1.4 — Annotations and marks

Highlights, notes, and bookmarks as first-class data in the catalog:
imported from the Kindle and/or created in the TUI reader. Picks up the
visual selection (`v`) and the bookmarks that v0.9 left deferred.

- [ ] Migration `0008_annotations.sql` — `annotations` table (`book_id`,
      `kind` highlight|note|bookmark, `chapter`, `offset`, `text` the marked
      excerpt, `note` optional comment, `source` kindle|cdx, `created_at`);
      index by `book_id`.
- [ ] `cdx import clippings <path>` — parse `My Clippings.txt` (records
      delimited by `==========`: title/author, type, location, timestamp,
      text) and import all annotations into the DB, matching each with the
      catalog's book by title/author (unmatched ones become a warning, not
      an error). `source = kindle`. `--json` summarizes what went in. TUI: an
      equivalent import flow.
- [ ] `cdx annotations ls <id|title>` — list a book's annotations (human +
      `--json`); a `--source kindle|cdx` flag filters the origin.
- [ ] TUI reader: create a mark via visual selection (`v` + movement, Enter
      confirms) and a note (a comment input over the selected excerpt) —
      persists with `source = cdx`.
- [ ] TUI reader: navigate annotations — a list/modal of the book's marks
      with a jump to the corresponding excerpt; keys to jump between marks
      documented in `?`.
- [ ] TUI reader: visually highlight the origin — marks imported from the
      Kindle and marks created in codex use distinct styles (via
      `src/reader/style.rs`).
- [ ] Export annotations in a neutral format (Markdown/JSON), grouped by
      book and separating Kindle vs codex origin.

Exploration (best-effort, may slip to the backlog):

- [ ] Try to re-export to the Kindle the annotations created only in codex,
      reusing open-source code (Calibre plugins, `.sdr`/`.pds`/`.mbp`
      sidecar parsers). A proprietary format, tied to the file's
      ASIN/checksum and unstable across firmwares — no round-trip guarantee.
      Document how far it can go.

### Deferred extras

- [ ] Man page (`cdx.1`)
- [ ] **Dynamic** completion of positional arguments (`cdx inspect <TAB>`,
      `cdx rm <TAB>`) querying the catalog via
      `clap_complete::engine::ArgValueCompleter` (`unstable-dynamic`
      feature).
- [ ] Broader integration-test coverage (beyond the CI gate).

## Backlog (no milestone)

- Read-only HTTP server to browse the catalog from another device
- Wi-Fi sync (no cable)
- News download / RSS-to-EPUB (à la Calibre recipes)
- Plugin system
