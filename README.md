# codex

Terminal-first ebook library and ereader manager. Inspired by Calibre,
built for people who live in the shell.

Everything works over SSH — no GUI. Every read command speaks JSONL (`--json`)
so it composes with `jq`, `fzf`, and shell scripts, and every subcommand has an
equivalent screen in the built-in terminal UI (`cdx tui`). The binary is called
`cdx`.

📖 **Documentation:** <https://danielwsx64.github.io/codex/>

## Install

```sh
curl -fsSL https://codex.daniel.ws/install.sh | sh
```

The script downloads the latest release binary for your platform (Linux x86_64,
macOS Apple silicon / Intel), verifies its checksum, and installs it to
`~/.local/bin`. See the [installation guide](https://danielwsx64.github.io/codex/installation.html)
for manual downloads and options.

## Features

- **Catalogs** — one or more independent libraries, each a plain directory you
  can keep under git.
- **Import** — EPUB, PDF, MOBI, AZW3 (plus TXT/Markdown for the reader), with
  metadata extraction and content-based dedup.
- **Metadata** — edit tags/rating/series/… and embed changes back into the file.
- **Search & groups** — substring search with field filters, plus folder-style
  browsing by author, tag, series, publisher, language, or rating.
- **Kindle sync** — detect USB Kindles and push/pull/sync with a `git add -p`
  style interactive plan.
- **Duplicates** — detect and remove duplicate copies.
- **Reader** — read EPUB, MOBI/AZW3 (DRM-free), PDF, TXT, and Markdown in the
  terminal, with vim-style keys and saved progress.

## Quickstart

```sh
cdx catalog init home ~/books      # create a catalog
cdx add ~/Downloads/*.epub         # import books
cdx ls                             # list them
cdx tui                            # open the terminal UI
```

## Updating

```sh
cdx update            # check + self-install the latest release
cdx update --check    # only report whether a newer version exists
```

## Shell completions

```sh
cdx completions bash   # or zsh, fish, elvish, powershell
```

See the [completions guide](https://danielwsx64.github.io/codex/completions.html)
for where to install the generated script per shell.

## Building from source

Requires a Rust toolchain (MSRV **1.80**):

```sh
cargo build --release   # binary at target/release/cdx
```

## Roadmap

See [ROADMAP.md](./ROADMAP.md). The current release is **v1.0 — Estável**;
everything still open lives in the *Pós-1.0 — Futuro* and *Backlog* sections.

## License

MIT — see [LICENSE](./LICENSE).
