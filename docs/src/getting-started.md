# Getting started

## 1. Create a catalog

A catalog is a directory you choose — it can live anywhere, including inside a
git repo. codex tracks the set of catalogs in a small registry at
`$XDG_CONFIG_HOME/cdx/config.toml` and remembers which one is current.

```sh
cdx catalog init home ~/books
```

This creates `~/books/catalog.db` and `~/books/books/`, registers it under the
name `home`, and makes it the current catalog.

## 2. Add some books

```sh
cdx add ~/Downloads/*.epub ~/Downloads/dune.pdf
```

codex extracts basic metadata, stores each file under
`books/<id>/Author_-_Title.ext`, and skips exact duplicates (override with
`--force`). Supported formats: EPUB, PDF, MOBI, AZW3 (plus TXT/Markdown for the
reader).

## 3. List and inspect

```sh
cdx ls
cdx inspect 1          # by id
cdx inspect "Dune"     # or exact title (case-insensitive)
```

Add `--json` to any read command for JSONL output:

```sh
cdx ls --json | jq -c '{id, title, author}'
```

## 4. Open the terminal UI

```sh
cdx tui
```

The TUI mirrors every command. Press `?` on any screen for its key bindings,
`:` for the command palette, and `q` (or `Ctrl+C`) to quit.

## Global flags

These work on every subcommand:

| Flag              | Meaning                                                        |
| ----------------- | ------------------------------------------------------------- |
| `--json`          | Emit machine-readable JSONL on stdout                          |
| `-v` / `-vv` / `-vvv` | Increase log verbosity (info / debug / trace)             |
| `--catalog <name>`| Use a registered catalog other than the current one           |
| `--data-dir <path>` | Override the config dir (mostly for tests)                  |

Next: learn about [Catalogs](./catalogs.md) and the [Library](./library.md).
