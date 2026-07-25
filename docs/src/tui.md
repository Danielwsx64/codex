# Terminal UI

```sh
cdx tui
```

The TUI mirrors every CLI subcommand — both surfaces share the same domain
modules, only the rendering differs. The welcome screen is the home and lists the
top-level sections as navigable links (`↑`/`↓` to move, `Enter` to enter):

1. **Library** — list / view / edit / remove books, groups, duplicates
2. **Search** — the Library's filtered mode
3. **Catalogs** — the catalog registry + new-catalog wizard
4. **Devices** — ereader sync

## Command palette

Press `:` from any screen to open a vim-style input at the bottom. Commands:
`:library`, `:catalogs`, `:search`, `:devices`, and `:quit` (`:q`). `Tab`
completes the shortest unique prefix (`:l`, `:c`, `:s`, `:d`, `:q`). `Enter` runs
it; `Esc` cancels.

## Reserved keys

These are consistent across the whole TUI:

| Key            | Meaning                                              |
| -------------- | --------------------------------------------------- |
| `q`, `Ctrl+C`  | Quit (the only exit keys)                            |
| `Esc`          | Back / cancel within a screen — never quits          |
| `Enter`        | Confirm / drill in                                   |
| `?`            | Contextual help for the active screen                |

Per-screen bindings beyond these are documented in each screen's `?` overlay.

## Devices screen

| Key       | Meaning                                                        |
| --------- | -------------------------------------------------------------- |
| `r`       | Rename the selected device's alias                              |
| `R`, `F5` | Rescan — pick up devices connected since the screen opened      |
| `s`       | Open the sync plan for the selected device                      |

`R`/`F5` also works in the device's books view, where it re-reads the files on
the device.
