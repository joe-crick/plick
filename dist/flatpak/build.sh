#!/usr/bin/env bash
# Build and install the Plick Flatpak from a clean state.
# Usage: ./dist/flatpak/build.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
MANIFEST="$SCRIPT_DIR/com.github.plick.yml"
BUILD_DIR="$PROJECT_ROOT/build-dir"
REPO_DIR="$PROJECT_ROOT/repo"
BUNDLE="$PROJECT_ROOT/plick.flatpak"

echo "==> Clearing flatpak-builder cache and build directory..."
rm -rf "$PROJECT_ROOT/.flatpak-builder" "$BUILD_DIR"

echo "==> Building flatpak..."
flatpak-builder --force-clean "$BUILD_DIR" "$MANIFEST"

echo "==> Exporting to local repo..."
flatpak-builder --export-only --repo="$REPO_DIR" "$BUILD_DIR" "$MANIFEST"

echo "==> Bundling to $BUNDLE..."
flatpak build-bundle "$REPO_DIR" "$BUNDLE" com.github.plick

echo "==> Installing (user)..."
flatpak install --user --reinstall -y "$BUNDLE"

echo "==> Done. Run with: flatpak run com.github.plick"
