# Installation

codex ships as a single prebuilt binary. There is no `cargo install` / crates.io
package — you download the binary for your platform.

## Install script (recommended)

```sh
curl -fsSL https://codex.daniel.ws/install.sh | sh
```

The script detects your OS and architecture, downloads the matching binary from
the latest [GitHub release](https://github.com/Danielwsx64/codex/releases),
verifies its SHA-256 checksum, and installs it to `~/.local/bin`.

Environment variables:

| Variable          | Default            | Meaning                          |
| ----------------- | ------------------ | -------------------------------- |
| `CDX_INSTALL_DIR` | `$HOME/.local/bin` | Where to install the `cdx` binary |
| `CDX_VERSION`     | latest release     | Install a specific tag (e.g. `v1.0.0`) |

If `~/.local/bin` is not on your `PATH`, the script prints a reminder. Add it:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

## Supported platforms

| Platform            | Release asset                    |
| ------------------- | -------------------------------- |
| Linux x86_64        | `cdx-x86_64-unknown-linux-musl` (static) |
| macOS (Apple silicon) | `cdx-aarch64-apple-darwin`     |
| macOS (Intel)       | `cdx-x86_64-apple-darwin`        |

The Linux binary is statically linked against musl, so it runs on both glibc and
musl distributions with no system dependencies. USB device sync is Linux-only;
the rest of codex works on every supported platform.

## Manual download

Grab the asset for your platform from the
[releases page](https://github.com/Danielwsx64/codex/releases), verify it, and
move it onto your `PATH`:

```sh
target="x86_64-unknown-linux-musl"   # pick yours
tag="v1.0.0"
base="https://github.com/Danielwsx64/codex/releases/download/$tag"
curl -fsSLO "$base/cdx-$target"
curl -fsSLO "$base/cdx-$target.sha256"
sha256sum -c "cdx-$target.sha256"    # macOS: shasum -a 256 -c
chmod +x "cdx-$target"
mv "cdx-$target" ~/.local/bin/cdx
```

## Keeping it up to date

Once installed, codex can update itself — see [Updating cdx](./updating.md):

```sh
cdx update
```

## Building from source

You need a Rust toolchain (MSRV **1.80**):

```sh
git clone https://github.com/Danielwsx64/codex
cd codex
cargo build --release
# binary at target/release/cdx
```
