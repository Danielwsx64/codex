#!/bin/sh
# cdx installer — downloads the latest release binary for this platform,
# verifies its checksum, and installs it.
#
#   curl -fsSL https://codex.daniel.ws/install.sh | sh
#
# Environment:
#   CDX_INSTALL_DIR   install location (default: $HOME/.local/bin)
#   CDX_VERSION       tag to install (default: latest release)
set -eu

REPO="Danielwsx64/codex"
INSTALL_DIR="${CDX_INSTALL_DIR:-$HOME/.local/bin}"

err() {
	echo "error: $*" >&2
	exit 1
}

info() {
	echo "$*" >&2
}

# Prefer curl, fall back to wget. Both write to stdout.
download() {
	# $1 = url, $2 = output path
	if command -v curl >/dev/null 2>&1; then
		curl -fsSL "$1" -o "$2"
	elif command -v wget >/dev/null 2>&1; then
		wget -qO "$2" "$1"
	else
		err "need curl or wget to download files"
	fi
}

download_stdout() {
	if command -v curl >/dev/null 2>&1; then
		curl -fsSL "$1"
	elif command -v wget >/dev/null 2>&1; then
		wget -qO - "$1"
	else
		err "need curl or wget to download files"
	fi
}

# Map uname to the release asset target triple. We ship a single static musl
# binary for Linux (runs on glibc and musl hosts alike).
detect_target() {
	os="$(uname -s)"
	arch="$(uname -m)"
	case "$os" in
	Linux)
		case "$arch" in
		x86_64 | amd64) echo "x86_64-unknown-linux-musl" ;;
		*) err "unsupported Linux architecture: $arch (build from source: https://github.com/$REPO)" ;;
		esac
		;;
	Darwin)
		case "$arch" in
		arm64 | aarch64) echo "aarch64-apple-darwin" ;;
		x86_64) echo "x86_64-apple-darwin" ;;
		*) err "unsupported macOS architecture: $arch" ;;
		esac
		;;
	*)
		err "unsupported OS: $os (build from source: https://github.com/$REPO)"
		;;
	esac
}

# Resolve the download tag: an explicit CDX_VERSION, or the latest release.
resolve_tag() {
	if [ -n "${CDX_VERSION:-}" ]; then
		echo "$CDX_VERSION"
		return
	fi
	# Follow the /releases/latest redirect and read the tag off the final URL.
	api="https://api.github.com/repos/$REPO/releases/latest"
	tag="$(download_stdout "$api" | grep -m1 '"tag_name"' | cut -d'"' -f4)"
	[ -n "$tag" ] || err "could not determine the latest release tag"
	echo "$tag"
}

verify_checksum() {
	# $1 = file, $2 = expected sha256 hex
	if command -v sha256sum >/dev/null 2>&1; then
		actual="$(sha256sum "$1" | cut -d' ' -f1)"
	elif command -v shasum >/dev/null 2>&1; then
		actual="$(shasum -a 256 "$1" | cut -d' ' -f1)"
	else
		info "warning: no sha256 tool found; skipping checksum verification"
		return 0
	fi
	[ "$actual" = "$2" ] || err "checksum mismatch (expected $2, got $actual)"
}

main() {
	target="$(detect_target)"
	tag="$(resolve_tag)"
	asset="cdx-$target"
	base="https://github.com/$REPO/releases/download/$tag"

	info "Installing cdx $tag ($target) to $INSTALL_DIR"

	tmp="$(mktemp -d)"
	trap 'rm -rf "$tmp"' EXIT

	download "$base/$asset" "$tmp/cdx"
	download "$base/$asset.sha256" "$tmp/cdx.sha256"
	expected="$(cut -d' ' -f1 <"$tmp/cdx.sha256")"
	verify_checksum "$tmp/cdx" "$expected"

	chmod +x "$tmp/cdx"
	mkdir -p "$INSTALL_DIR"
	mv "$tmp/cdx" "$INSTALL_DIR/cdx"

	info "Installed cdx to $INSTALL_DIR/cdx"
	case ":$PATH:" in
	*":$INSTALL_DIR:"*) ;;
	*) info "note: $INSTALL_DIR is not on your PATH — add it to use \`cdx\`" ;;
	esac
	"$INSTALL_DIR/cdx" --version 2>/dev/null || true
}

main "$@"
