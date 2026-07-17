# CLI reference

`cdx <command> [args]`. Run `cdx --help` or `cdx <command> --help` for the
authoritative, always-current text. Global flags (`--json`, `-v/-vv/-vvv`,
`--catalog <name>`, `--data-dir <path>`) work on every subcommand.

## Catalogs

| Command | Description |
| --- | --- |
| `cdx catalog init <name> <path>` | Create + register a catalog (`--description`, `--no-switch`) |
| `cdx catalog add <name> <path>` | Register an existing catalog (`--description`, `--no-switch`) |
| `cdx catalog ls` | List catalogs |
| `cdx catalog use <name>` | Switch the current catalog |
| `cdx catalog rm <name>` | Unregister (`--purge` deletes files) |

## Library

| Command | Description |
| --- | --- |
| `cdx add <file>...` | Import books (`--force`) |
| `cdx ls` | List books (`--columns`, `--all-columns`) |
| `cdx inspect <id\|title>` | Show detailed metadata |
| `cdx search <query>` | Search (`--author`, `--tag`, `--series`, `--rating`) |
| `cdx edit <id>` | Edit metadata in `$EDITOR` |
| `cdx tag <id> <tag>...` | Add tags |
| `cdx untag <id> <tag>...` | Remove tags (`--all`) |
| `cdx rate <id> <0-5>` | Set/clear rating |
| `cdx series <id> <name>` | Set series (`--index`, `--clear`) |
| `cdx rm <id\|title>` | Remove book (`--keep`) |
| `cdx groups --by <field>` | Group by author/tag/rating/publisher/language/series/format |
| `cdx dedup` | Find duplicates (`--by`, `--rm`, `--yes`, `--keep`) |

## Metadata embedding

| Command | Description |
| --- | --- |
| `cdx embed sync` | Embed metadata into all `pending` books |

## Devices

| Command | Description |
| --- | --- |
| `cdx device ls` | List devices |
| `cdx device alias <target> <alias>` | Set/rename alias |
| `cdx device books` | List device books (`--device`) |
| `cdx device clean` | Remove books from a device (`--device`, `--all`, `--yes`) |
| `cdx push <id\|title>` | Copy to device (`--device`) |
| `cdx pull <path>` | Import from device (`--device`, `--force`) |
| `cdx sync` | Bidirectional sync (`--device`, `--dry-run`, `--yes`, `--verify`) |

## Maintenance

| Command | Description |
| --- | --- |
| `cdx tui` | Open the terminal UI |
| `cdx update` | Self-update (`--check`, `--yes`) — see [Updating cdx](./updating.md) |
| `cdx completions <shell>` | Print a completion script — see [Shell completions](./completions.md) |

## JSON output

Every read command accepts `--json` and emits **JSONL** — one JSON object per
line, no surrounding array — so it streams through `jq -c`, `head`, and
`fzf --preview`. A command with zero records prints nothing.

```sh
cdx ls --json | jq -c 'select(.rating >= 4) | {title, rating}'
cdx search --author tolkien --json | jq -r '.title'
```
