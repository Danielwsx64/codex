# Groups

Group browsing lets you navigate the catalog like a folder tree: pick a metadata
field as the "grouper" and each distinct value becomes a folder. It is a **mode
of the Library screen**, not a separate section — inside a folder you get the same
columns and actions as the normal listing.

A folder's scope is **exact equality** (the "Jane Austen" folder contains only
`author = 'Jane Austen'`), unlike search, which is substring. `author` is a
single column (a book falls in one folder); `tags` is many-to-many (a book can
appear in several folders).

## CLI

```sh
cdx groups --by author
cdx groups --by tag
cdx groups --by rating
cdx groups --by publisher
cdx groups --by language
cdx groups --by series
cdx groups --by format
cdx groups --by author --json
```

Each row is a value plus its book count. In `--json` (JSONL, one object per
group), the catch-all group — no author / no tags / no rating — has `value:
null`. An empty catalog prints nothing in `--json`.

## In the TUI

Press `g` in the Library to open the grouper selector (Author / Tags / Rating /
Publisher / Language / Series / Format / Off). The folder level lists value +
count; `Enter` drills in. A breadcrumb shows the current group. `Esc` steps back
one layer at a time: clear the filter → back to folders → leave grouping → back
to the welcome screen. `/` inside a folder filters that folder's books in memory
without widening its exact scope.
