# Library

The library commands operate on books in the current catalog.

## Importing

```sh
cdx add file1.epub file2.pdf ...
```

Accepts EPUB, PDF, MOBI, AZW3 (and TXT/Markdown for the reader). codex extracts
metadata, stores each file under a sanitized `Author_-_Title.ext`, and skips
content-duplicate imports. Use `--force` to import anyway.

## Listing

```sh
cdx ls
cdx ls --columns id,title,author,rating
cdx ls --all-columns
cdx ls --json
```

Available columns: `id, title, author, tags, series, rating, publisher,
language, published, isbn, format, embed`.

## Inspecting

```sh
cdx inspect <id|title>
```

Accepts a numeric id or an exact (case-insensitive) title. An ambiguous title
returns an error listing the candidate ids.

## Editing metadata

```sh
cdx edit <id>                      # opens $EDITOR with the book's metadata as TOML
cdx tag <id> <tag>...              # add tags
cdx untag <id> <tag>...            # remove tags (--all clears every tag)
cdx rate <id> <0-5>                # set rating (0 clears)
cdx series <id> <name> --index 2   # set series + position (--clear removes it)
```

Any metadata change marks the book `embed_status = pending` so it can be embedded
back into the file later — see [Metadata embedding](./metadata-embedding.md).

## Searching

```sh
cdx search dune                    # substring across title/author/tags
cdx search --author tolkien        # field filters
cdx search hobbit --tag fantasy --rating 4..5
```

Whitespace-separated tokens are AND'd. Filters: `--author`, `--tag` (repeatable,
AND), `--series`, `--rating` (exact `4` or range `3..5`). `--json` emits JSONL.

## Removing

```sh
cdx rm <id|title>          # removes from catalog and deletes the file
cdx rm <id|title> --keep   # moves the file to the cwd instead of deleting
```

## In the TUI

The **Library** screen lists books with the same columns and actions
(inspect / edit / open / push / delete / column selection / embed). `/` filters
by text; `g` switches to [group](./groups.md) browsing.
