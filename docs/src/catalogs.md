# Catalogs

codex is multi-catalog. Each catalog is a directory you pick; the registry at
`$XDG_CONFIG_HOME/cdx/config.toml` lists them and points at the current one.
On-disk layout of a catalog:

```
<catalog>/
  catalog.db          # SQLite metadata
  books/<id>/<file>   # the stored book files
```

## Commands

| Command                              | What it does                                        |
| ------------------------------------ | --------------------------------------------------- |
| `cdx catalog init <name> <path>`     | Create a new catalog at `<path>` and register it    |
| `cdx catalog add <name> <path>`      | Register an existing catalog directory               |
| `cdx catalog ls`                     | List registered catalogs (current + `(missing)`)    |
| `cdx catalog use <name>`             | Switch the current catalog                           |
| `cdx catalog rm <name>`              | Unregister a catalog (`--purge` deletes its files)   |

`init` and `add` accept `--description <text>` and `--no-switch` (register
without making it current).

```sh
cdx catalog init fiction ~/lib/fiction --description "Novels"
cdx catalog add archive /mnt/nas/ebooks
cdx catalog ls
cdx catalog use archive
```

A catalog whose directory has disappeared from disk is shown as `(missing)` in
`cdx catalog ls` rather than causing an error.

## In the TUI

The **Catalogs** screen lists every catalog (current one marked), lets you
switch with `Enter` and remove with confirmation. The **New catalog** wizard
covers both `init` and `add`. Reach it from the welcome menu or the `:catalogs`
command-palette entry.
