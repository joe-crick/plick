#!/usr/bin/env bash
# Build Plick release binary and install data files.
# Usage: ./dist/build.sh [--prefix /usr/local]

set -euo pipefail

PREFIX="${1:-/usr/local}"
BINDIR="$PREFIX/bin"
DATADIR="$PREFIX/share"
DESTDIR="${DESTDIR:-}"

echo "Building Plick (release)..."
cargo build --release

echo "Installing..."
install -Dm755 target/release/plick "${DESTDIR}${BINDIR}/plick"
install -Dm644 data/com.github.plick.desktop "${DESTDIR}${DATADIR}/applications/com.github.plick.desktop"
install -Dm644 data/com.github.plick.metainfo.xml "${DESTDIR}${DATADIR}/metainfo/com.github.plick.metainfo.xml"
install -Dm644 data/com.github.plick.svg "${DESTDIR}${DATADIR}/icons/hicolor/scalable/apps/com.github.plick.svg"

echo "Installed to ${DESTDIR}${PREFIX}"
