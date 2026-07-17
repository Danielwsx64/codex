# codex

**codex** is a terminal-first ebook library and ereader manager, inspired by
Calibre but built for people who live in the shell. The binary is called `cdx`.

Everything works over SSH — there is no GUI. Every read command speaks JSONL
(`--json`) so it composes cleanly with `jq`, `fzf`, and shell scripts, and every
subcommand has an equivalent screen in the built-in terminal UI (`cdx tui`).

## What it does

- **Catalogs** — keep one or more independent libraries, each a plain directory
  (a `catalog.db` SQLite file plus a `books/` tree) that you can put under git.
- **Import** — add EPUB, PDF, MOBI, and AZW3 files; codex extracts metadata and
  stores each book under a sanitized `Author_-_Title.ext` name.
- **Metadata** — edit title/author/tags/rating/series and embed the changes back
  into the book file.
- **Search & groups** — substring search with field filters, and a folder-style
  browse mode grouping by author, tag, series, publisher, language, or rating.
- **Kindle sync** — detect USB-mounted Kindles and push/pull/sync books, with a
  `git add -p`-style interactive plan.
- **Duplicates** — detect duplicate copies (by content hash or normalized
  title+author) and assist removal.
- **Reader** — read EPUB, MOBI/AZW3 (non-DRM), PDF, TXT, and Markdown right in
  the terminal, with vim-style navigation and saved reading progress.

## Where to start

- New here? Read [Installation](./installation.md), then
  [Getting started](./getting-started.md).
- Already installed? Jump to the [CLI reference](./cli-reference.md).
