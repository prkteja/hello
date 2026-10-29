#!/bin/sh
# Installs Hello for the current user (or system-wide with PREFIX=/usr/local and sudo).
# Works from a release tarball or from a source checkout after `cargo build --release`.
set -eu
cd "$(dirname "$0")"
PREFIX="${PREFIX:-$HOME/.local}"

if [ -f hello ]; then BIN=hello; else BIN=target/release/hello; fi
[ -f "$BIN" ] || { echo "hello binary not found; run 'cargo build --release' first" >&2; exit 1; }

install -Dm755 "$BIN" "$PREFIX/bin/hello"
# Point the launcher entry at the installed binary: desktop sessions often
# don't have ~/.local/bin on their PATH, so a bare "Exec=hello" would fail.
DESKTOP="$PREFIX/share/applications/io.github.hello.desktop"
mkdir -p "$(dirname "$DESKTOP")"
sed "s|^Exec=hello\$|Exec=\"$PREFIX/bin/hello\"|" data/io.github.hello.desktop > "$DESKTOP"
chmod 644 "$DESKTOP"
command -v update-desktop-database >/dev/null && update-desktop-database -q "$(dirname "$DESKTOP")" || true
echo "Installed to $PREFIX/bin/hello"
