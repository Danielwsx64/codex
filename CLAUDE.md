# codex

Terminal-first ebook library and ereader manager. Inspired by Calibre,
built for people who live in the shell.

## Naming

- **Project / crate**: `codex`
- **Binary**: `cdx` (defined via `[[bin]]` in `Cargo.toml`)

When referring to commands in docs and code, use `cdx` (e.g. `cdx ls`,
`cdx add`). The longer `codex` name is reserved for the project itself.

## Language policy

- **In PT-BR:** `ROADMAP.md`.
- **In English:** everything else — code (modules, functions, variables, columns), comments, log lines, CLI strings (`--help`, error messages), commit messages, PR descriptions, this file. No exceptions for domain terms.

## Core direction

- **Terminal-first**: no GUI. All flows must work over SSH.
- **Multi-catalog registry**: cdx keeps a TOML registry at
  `$XDG_CONFIG_HOME/cdx/config.toml` listing the registered catalogs
  (name, path, description) plus a `current` pointer. Each catalog is
  a directory the user chose (so it can be a git repo): a `catalog.db`
  SQLite file plus a `books/<id>/` tree for the binary files. cdx does
  **not** depend on Calibre's `metadata.db`. Importing from a Calibre
  library is a planned interop feature, not a base requirement.
- **Composable**: `--json` is a first-class flag on every read command and emits **JSONL** — one JSON object per line, no surrounding array. This streams through `jq -c`, `head`, and `fzf --preview` without buffering.

## Where things live

See [ROADMAP.md](./ROADMAP.md) for the milestone breakdown. Current
work targets `v0.1 — MVP catálogo`.

## Workflow

- After any change, run `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`. Resolve everything before declaring the work done.
- MSRV is pinned in `Cargo.toml` (`rust-version = "1.80"`). Don't reach for features from a newer release without bumping the MSRV in the same PR.
- `Cargo.lock` is committed (binary crate). Update deps with `cargo update -p <crate>` consciously; never bulk-update.
- Ship a `ROADMAP.md` feature → flip its checkbox (`- [ ]` → `- [x]`) in the same commit/PR. Don't let the roadmap drift.

## Crate layout

- `src/main.rs` is a thin entry that calls into the library. **All logic lives in `src/lib.rs` and submodules** so it stays testable as a library and reusable from `tests/`.
- Module files use the 2018-edition layout: `foo.rs` plus a `foo/` directory for its submodules. **No `mod.rs`** in `src/`. The one exception is `tests/common/mod.rs` — that's how Cargo finds shared helpers between integration test files.
- Integration tests in `tests/<feature>.rs`. Unit tests in `#[cfg(test)] mod tests { … }` at the bottom of the file under test.

## Error handling

- **Library code** (`src/lib.rs` and submodules): typed errors via `thiserror`. Define one `Error` enum per layer with distinct failure modes (e.g. `catalog::Error`, `import::Error`). Variants carry context — `#[from]` for upstream errors, named fields for caller-supplied data. Never return `String`-typed errors or `Box<dyn Error>`.
- **Binary code** (`src/main.rs` and the command-dispatch layer): `anyhow::Result<T>` with `?`. Use `.with_context(|| format!("…"))` to add the operational layer (which book, which path) the typed error doesn't carry.
- **`unwrap`/`expect` are forbidden outside tests** and proven invariants. When `expect` is unavoidable, write it in the "expect-because" style — describe the invariant, not the failure: `expect("catalog dir is created during `cdx init` before any other command runs")`.
- **`panic!` only for programmer bugs.** Any user-supplied input (CLI args, file contents, DB rows) that violates expectations becomes an `Err`, never a panic.
- `?` over `match`/`if let Err(_)` whenever the error just bubbles up.

## CLI (`clap`)

- `clap` v4 with the `derive` feature. The subcommand surface is a `Command` enum — one variant per `cdx <verb>` in the roadmap. Each variant is a struct with `#[arg]` fields; no hand-parsed `Vec<String>`.
- Global flags on the root parser: `--json` (machine output), `-v/--verbose` (one extra tracing level per occurrence), `--data-dir <path>` (override the resolved XDG path; mostly for tests).
- `#[command(version, about)]` on the root so `cdx --version` and `cdx --help` work out of the box.
- All help text and error strings are in English (Language policy).

## TUI (`ratatui`)

- Entry point is `cdx tui`. The TUI mirrors every CLI subcommand (CLI ↔ TUI parity is declared in `ROADMAP.md`). Both surfaces consume the same domain modules — only the rendering layer differs.
- Backend is `crossterm`. Always restore the terminal (disable raw mode, leave alternate screen, show cursor) on every exit path, including panics. Use a RAII guard so a `?` early-return can't leave a broken terminal behind.
- **Reserved keys — do not rebind:**
  - `q` and `Ctrl+C` are the **only** exit keys for the whole TUI.
  - `Esc` and `Enter` are reserved for in-screen navigation (back / confirm / drill-in) and must **never** trigger exit. A screen that doesn't use them yet still leaves them unbound, never aliased to quit.
- Per-screen key bindings beyond these reserved ones are documented in `?` (the contextual help overlay).
- Shared widget content (welcome screen art, labels) lives in plain modules under `src/` and is consumed by both the CLI renderer and the ratatui renderer.

## Output

- Human output goes to **stdout**; logs and errors go to **stderr**. Never mix.
- Machine output (`--json`) emits **JSONL** — one JSON object per line, no surrounding array. Per-record `serde_json::to_writer` + `writeln!`. A command that returns zero records prints nothing in `--json` mode (no `[]`, no blank line).
- Human tables use `tabwriter` (lightweight, plain text). Reserve fancier table libs for the day they're actually needed.

## SQLite (`rusqlite`)

- `rusqlite` with the `bundled` feature so the build doesn't depend on a system SQLite. No `sqlx` — we have no async story and the compile-time SQL check isn't worth a `DATABASE_URL` at build time.
- **Connections are passed explicitly**, never via a global/`OnceLock`. Every function that touches the DB takes `conn: &Connection` (or `&mut Connection` for transactions). This keeps tests trivial — each test opens a fresh DB in `tempfile::tempdir()`.
- Use `conn.prepare_cached(...)` for hot reads inside loops.
- **Transactions are explicit**: any write that touches more than one row or table opens `conn.transaction()?` and commits at the end. Single-statement writes can run directly on the connection.
- Migrations via `rusqlite_migration`. SQL files live under `migrations/`, embedded with `include_str!`, and the migrator runs on `cdx catalog init` and on every command startup against the active catalog (idempotent).

## Paths

- Use the `directories` crate (`ProjectDirs::from("", "", "cdx")`) to resolve the cdx **config dir** (`$XDG_CONFIG_HOME/cdx` on Linux). That dir holds the catalog registry (`config.toml`) — *not* the catalogs themselves. Don't read `$XDG_CONFIG_HOME` by hand — the crate already handles macOS/Windows fallbacks.
- Each registered catalog lives at a user-chosen path (resolved from the registry). The on-disk layout is `<catalog>/catalog.db` plus `<catalog>/books/<book-id>/<file>`.
- File paths are `Path` / `PathBuf` everywhere. **Never `String` for paths.** At OS boundaries, accept `&Path` and use `.display()` only for logging.
- The `--data-dir` flag overrides the resolved **config dir** (where the registry lives), not the catalog dir. Integration tests pass a `tempdir` here so they never touch `$XDG_CONFIG_HOME`.

## Logging (`tracing`)

- `tracing` + `tracing-subscriber`. Filter via `EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into())` — that satisfies the roadmap's "logging configurable via `RUST_LOG`" requirement.
- Initialize the subscriber once at the top of `main`, before parsing args.
- Use structured fields, not interpolated strings: `tracing::info!(book_id, %path, "imported")` — not `info!("imported {} at {}", book_id, path)`.
- Levels: `error` for user-visible failures (after they've bubbled to `main`), `warn` for recoverable oddities, `info` for one-line "what just happened" per command, `debug`/`trace` for internal flow.

## Serialization (`serde`)

- `serde` + `serde_json` for `--json`. `toml` (and `toml_edit` once `cdx edit` lands in v0.2 — it preserves formatting on rewrite).
- Derive `Serialize` / `Deserialize` only on types that genuinely cross a boundary (CLI output, file format, DB row). Don't derive by reflex.
- JSON output stays `snake_case` — the consumer is `jq`, not a JavaScript client. Use `#[serde(rename_all = "snake_case")]` when the Rust field name doesn't match what we want on the wire.

## Tests

- **Every module gets unit-test coverage** for its public functions, success path and main failure path. Unit tests live in `#[cfg(test)] mod tests` at the bottom of the file under test.
- **CLI integration tests** in `tests/<feature>.rs` use `assert_cmd::Command::cargo_bin("cdx")` + `predicates` for stdout/stderr assertions + `tempfile::tempdir()` for the data directory. Always pass `--data-dir <tempdir>` so the test never touches `$XDG_DATA_HOME`.
- **Snapshot tests** for stable textual output (human tables, `--json` schema) via `insta::assert_snapshot!` / `insta::assert_json_snapshot!`. Review with `cargo insta review`; commit `.snap` files.
- **No shared global state** between tests — every test builds its own tempdir + DB. `cargo test` stays parallel-safe by default.
- **No `#[ignore]` on `main`.** A flaky or slow test gets fixed or deleted, not muted.
- Shared helpers between integration test files live in `tests/common/mod.rs`.

## Lints and style

- `#![warn(clippy::all)]` and `#![deny(unsafe_code)]` at the top of `src/lib.rs` and `src/main.rs`. We don't enable `clippy::pedantic` — too noisy for the value it adds. If a pedantic lint catches a real bug, copy that specific lint into the `warn` list.
- CI runs `cargo clippy --all-targets -- -D warnings`. Fix lint hits; never silence them with blanket `#[allow]` at file scope. A localized `#[allow(clippy::...)]` is fine when justified by a comment.
- **Naming follows the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/naming.html)**: `snake_case` for functions/modules/variables, `CamelCase` for types/traits, `SCREAMING_SNAKE_CASE` for constants, `is_*` / `has_*` for predicates.
- **Format strings use captured args**: `format!("{name}")`, `tracing::info!("imported {path:?}")` — not `format!("{}", name)`. Stable since 1.58.
- `let ... else` for early returns. Prefer iterator pipelines over imperative loops when they read more cleanly. Stop nesting `match` / `if let` past two levels — extract a function and pattern-match at the head.

## Documentation in code

- **No doc-comments** (`///`, `//!`) anywhere. Names and types carry the meaning; tests document behavior. The crate doesn't publish to `docs.rs` until v1.0 — revisit then.
- Line comments (`//`) only when the *why* is non-obvious — a hidden invariant, a workaround for a specific bug, behavior that would surprise a reader. Never to describe what the code does.

## Dependencies

- Add a dep only when stdlib + already-chosen crates can't do the job in a reasonable amount of code. Each new line in `[dependencies]` is a build-time, audit-surface, and coupling cost.
- Pin with `^x.y` (cargo default). Never `*` or `latest`.
- Expected v0.1 surface: `clap`, `rusqlite` (bundled), `rusqlite_migration`, `serde`, `serde_json`, `toml`, `tracing`, `tracing-subscriber`, `anyhow`, `thiserror`, `directories`, `tabwriter`. Dev-deps: `assert_cmd`, `predicates`, `tempfile`, `insta`. Anything outside this set needs a one-line note in the PR explaining why.
