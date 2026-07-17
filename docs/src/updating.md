# Updating cdx

codex can update itself in place from the latest GitHub release.

## Check for a newer version

```sh
cdx update --check
```

This reports whether a newer release exists and installs nothing. Add `--json`
for machine-readable output:

```sh
cdx update --check --json
# {"current":"1.0.0","latest":"1.1.0","newer_available":true,"html_url":"..."}
```

## Install the update

```sh
cdx update
```

codex downloads the release binary for your platform, verifies its SHA-256
checksum, and atomically replaces the running executable. You are asked to
confirm first; pass `--yes` to skip the prompt (for scripts):

```sh
cdx update --yes
```

## How it works

- The target platform is chosen automatically (Linux x86_64 → the static musl
  binary; macOS → the matching Apple-silicon or Intel binary).
- The download is checksum-verified before anything is replaced; a mismatch
  aborts without touching your installed binary.
- The new binary is written next to the current one and swapped in with an atomic
  rename, so an interrupted update never leaves you with a half-written `cdx`.

If your platform has no prebuilt release, `cdx update` tells you to build from
source (see [Installation](./installation.md)). You can also re-run the install
script at any time.
