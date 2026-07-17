# Metadata embedding

codex stores metadata in the catalog database. Embedding writes that metadata
back into the book *file itself*, so the values travel with the book when you
copy it to a device or another app.

## The embed cycle

1. Any edit (`cdx edit`, `cdx tag`, `cdx rate`, `cdx series`, or the TUI edit
   modal) sets the book's `embed_status` to `pending`.
2. Running the sync embeds the metadata into every pending book and marks it
   `synced` (EPUB/PDF) or `unsupported` (MOBI/AZW3, which are not rewritten).

```sh
cdx embed sync
```

This walks every book with status `pending`, embeds the metadata, and prints
per-book progress plus a final summary.

## Format support

| Format     | Embedding                                   |
| ---------- | ------------------------------------------- |
| EPUB, PDF  | Supported — metadata is written into the file |
| MOBI, AZW3 | Not supported — marked `unsupported`         |

The `embed` column in `cdx ls` (and the Inspect screen in the TUI) shows the
current status. In the TUI, press `w` on the Inspect screen to embed a single
book.

> Note: for EPUB, codex keeps a stable content fingerprint that ignores the
> rewritten OPF, so re-embedding does not make a book look like a duplicate.
