# Devices (Kindle sync)

codex syncs with USB-connected Kindles on Linux, with support for several devices
connected at once. Each device is identified by the stable **serial** read from
the USB descriptor; you can give it an **alias** and use that day-to-day.

> Device sync is Linux-only. On other platforms the commands compile but detect
> no devices.

## Mass storage and MTP

Kindles connect one of two ways, and codex handles both:

- **USB mass storage** (Paperwhite 11 and older, Oasis, Voyage): the device shows
  up as a normal disk and codex finds it in `/proc/mounts`.
- **MTP** (Colorsoft, Scribe, Paperwhite 12): no disk appears at all. codex looks
  instead at what **gvfs** has mounted under `/run/user/<uid>/gvfs/mtp:host=…`
  and ties it back to the Amazon USB device by serial.

codex only *discovers* MTP mounts — it never mounts or unmounts anything. If
`cdx device ls` doesn't show an MTP Kindle, gvfs hasn't mounted it yet: open it
once in a file manager, or run

```sh
gio mount mtp://<host>/     # `gio mount -l` lists the available hosts
```

Writing to an MTP device also needs the `gio` command (`libglib2.0-bin` on
Debian/Ubuntu, `glib2` elsewhere). This is not a choice — gvfs cannot create a
file on an MTP mount through ordinary filesystem calls, because the protocol
needs the object size before the transfer begins. `cdx push` uses `gio copy` for
that one step and reports a clear error if the command is missing. Every other
operation (listing, reading, `pull`, `clean`) goes through the filesystem as
usual.

MTP devices are also **exclusive**: only one program can hold one at a time. If
Calibre or another MTP client has claimed the Kindle, gvfs won't have it mounted
and codex won't see it.

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
books, `R` or `F5` rescans). The list is a snapshot taken when the screen opened,
so rescan after plugging a device in — an MTP Kindle in particular only appears
once gvfs has finished mounting it. In the books view `R`/`F5` re-reads the
device's files. Presence indicators (`both` / `local only` / `device only` / `modified`)
appear in the Library and device views. `p` in the Library pushes the selected
book to the current device.
