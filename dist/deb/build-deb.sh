#!/usr/bin/env bash
# Build a .deb package for Plick.
# Run from the project root: ./dist/deb/build-deb.sh
#
# Prerequisites (Debian/Ubuntu):
#   apt install build-essential cargo libgtk-4-dev

set -euo pipefail

VERSION="0.1.0"
PKG="plick_${VERSION}_amd64"
ROOT="$(pwd)/dist/deb/$PKG"

echo "Building Plick release binary..."
cargo build --release

echo "Assembling .deb structure..."
rm -rf "$ROOT"
mkdir -p "$ROOT/DEBIAN"
mkdir -p "$ROOT/usr/bin"
mkdir -p "$ROOT/usr/share/applications"
mkdir -p "$ROOT/usr/share/metainfo"
mkdir -p "$ROOT/usr/share/icons/hicolor/scalable/apps"

# Control file
cat > "$ROOT/DEBIAN/control" <<EOF
Package: plick
Version: $VERSION
Section: video
Priority: optional
Architecture: amd64
Depends: libgtk-4-1, ffmpeg, xdg-desktop-portal, pipewire
Maintainer: Plick Maintainer <plick@example.com>
Description: Minimal GIF screen recorder for Wayland
 Plick is a fast, minimal Peek-like GIF screen recorder for Wayland.
 It uses xdg-desktop-portal for screen selection and FFmpeg for
 recording and GIF conversion.
EOF

# Install files
cp target/release/plick "$ROOT/usr/bin/plick"
cp data/com.github.plick.desktop "$ROOT/usr/share/applications/"
cp data/com.github.plick.metainfo.xml "$ROOT/usr/share/metainfo/"
cp data/com.github.plick.svg "$ROOT/usr/share/icons/hicolor/scalable/apps/"

# Build the .deb
dpkg-deb --build "$ROOT"
echo "Package created: dist/deb/${PKG}.deb"
