# codex

Terminal-first ebook library and ereader manager. Inspired by Calibre,
built for people who live in the shell.

## Naming

- **Project / crate**: `codex`
- **Binary**: `cdx` (defined via `[[bin]]` in `Cargo.toml`)

When referring to commands in docs and code, use `cdx` (e.g. `cdx ls`,
`cdx add`). The longer `codex` name is reserved for the project itself.

## Core direction

- **Terminal-first**: no GUI. All flows must work over SSH.
- **Independent catalog**: cdx maintains its own SQLite catalog under
  `$XDG_DATA_HOME/cdx` (typically `~/.local/share/cdx`). It does **not**
  depend on Calibre's `metadata.db`. Importing from a Calibre library is
  a planned interop feature, not a base requirement.
- **Composable**: prefer machine-readable output (`--json`) so that
  flows can be scripted with `jq`, `fzf`, etc.

## Where things live

See [ROADMAP.md](./ROADMAP.md) for the milestone breakdown. Current
work targets `v0.1 — MVP catálogo`.
