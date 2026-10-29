#!/bin/sh
# Installs Hello for the current user (or system-wide with PREFIX=/usr/local and sudo).
# Works from a release tarball or from a source checkout after `cargo build --release`.
set -eu
cd "$(dirname "$0")"
PREFIX="${PREFIX:-$HOME/.local}"

if [ -f hello ]; then BIN=hello; else BIN=target/release/hello; fi
[ -f "$BIN" ] || { echo "hello binary not found; run 'cargo build --release' first" >&2; exit 1; }

install -Dm755 "$BIN" "$PREFIX/bin/hello"
install -Dm644 data/io.github.hello.desktop "$PREFIX/share/applications/io.github.hello.desktop"
echo "Installed to $PREFIX/bin/hello"
