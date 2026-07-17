# Devices (Kindle sync)

codex syncs with USB-mounted Kindles on Linux, with support for several devices
connected at once. Each device is identified by the stable **serial** read from
the USB descriptor; you can give it an **alias** and use that day-to-day.

> Device sync is Linux-only. On other platforms the commands compile but detect
> no devices.

## Selecting a device

Every device command takes `--device <serial|alias>`. With exactly one device
connected it is the implicit default; with two or more and no flag you get an
error listing the candidates. The last device you used explicitly becomes the
current device (per catalog), so it stays the default across runs.

## Commands

| Command                              | What it does                                          |
| ------------------------------------ | ----------------------------------------------------- |
| `cdx device ls`                      | List detected and known devices (alias, serial, free space, book count) |
| `cdx device alias <target> <alias>`  | Set or rename a device alias                           |
| `cdx device books [--device <a>]`    | List books on a device with catalog presence          |
| `cdx device clean [--device <a>]`    | Remove books from a device (never touches the catalog) |
| `cdx push <id|title> [--device <a>]` | Copy a catalog book to the device                      |
| `cdx pull <path> [--device <a>]`     | Import a book from the device into the catalog         |
| `cdx sync [--device <a>]`            | Bidirectional diff + interactive apply                 |

`push` and `pull` open an interactive picker when you omit the target/path.
`clean` supports `--all` and `--yes`; `pull` supports `--force`.

## How identity works

- **Exact:** the `device_books` sync-state table records every book codex sent or
  pulled (book id ↔ device path + SHA-256 + size/mtime). No guessing for these.
- **By metadata:** books that arrived on the device by other means are matched on
  normalized title + author (casefold, NFKD without diacritics, collapsed
  punctuation/whitespace) — never the filename, since formats differ between
  ends. A genuinely ambiguous match becomes a conflict for you to resolve.

## Sync

```sh
cdx sync                 # interactive, git-add-p style
cdx sync --dry-run       # print the plan only
cdx sync --yes           # accept everything (scripts)
cdx sync --verify        # full SHA-256 instead of the size+mtime fast path
```

The plan lists what is missing on each end, `modified`, `missing`, and match
conflicts. You confirm item by item (`y` apply / `n` skip / `a` accept the rest /
`q` abort). **Sync never deletes** on either end — removal is always manual (use
`cdx device clean`).

## In the TUI

The **Devices** screen lists devices (`r` renames, `Enter` opens the device's
books). Presence indicators (`both` / `local only` / `device only` / `modified`)
appear in the Library and device views. `p` in the Library pushes the selected
book to the current device.
