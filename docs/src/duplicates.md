# Duplicates

`cdx dedup` finds duplicate books in the current catalog and suggests which copy
to remove. Signals are combined by **union** — if any method flags a group, it
becomes a candidate.

1. **Content hash** — SHA-256 fingerprints catch byte-identical copies and the
   same EPUB before/after embedding.
2. **Normalized title + author** — casefold + NFKD, collapsed
   punctuation/whitespace; catches the same book in different formats or editions
   where the hash won't match.

For each group codex suggests deleting the "worst" copy: the one with the fewest
filled-in metadata fields, breaking ties toward the older / weaker-embed copy.
The decision is always yours — codex only suggests.

## Usage

```sh
cdx dedup                    # list duplicate groups + the suggested removal
cdx dedup --by hash          # restrict the detection signal
cdx dedup --by meta
cdx dedup --by all           # default: union of both
cdx dedup --json             # JSONL, one object per group
```

Assisted removal:

```sh
cdx dedup --rm               # interactive picker (accepts the suggestion by default)
cdx dedup --rm --keep        # move removed files to the cwd instead of deleting
cdx dedup --yes              # accept every suggestion (scripts)
```

Removal reuses the `cdx rm` path and never deletes without confirmation (unless
`--yes`).

## In the TUI

The **Duplicates** action in the Library section lists the groups, highlights the
suggested copy, and removes with confirmation.
