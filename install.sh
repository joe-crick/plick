#!/usr/bin/env bash
# Install Plick locally with a GNOME desktop entry.
# Usage: ./install.sh

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")" && pwd)"
APP_ID="com.github.plick"
BINARY="$PROJECT_ROOT/target/release/plick"
ICON_SRC="$PROJECT_ROOT/data/${APP_ID}.svg"
ICON_DEST="$HOME/.local/share/icons/hicolor/scalable/apps/${APP_ID}.svg"
DESKTOP_DEST="$HOME/.local/share/applications/${APP_ID}.desktop"

echo "==> Building release binary..."
cargo build --release --manifest-path "$PROJECT_ROOT/Cargo.toml"

echo "==> Installing icon..."
install -Dm644 "$ICON_SRC" "$ICON_DEST"

echo "==> Installing desktop entry..."
cat > "$DESKTOP_DEST" <<EOF
[Desktop Entry]
Name=Plick
Comment=Minimal GIF screen recorder for Wayland
Exec=$BINARY
Icon=$APP_ID
Terminal=false
Type=Application
Categories=Utility;Video;GTK;
Keywords=screencast;gif;record;screen;wayland;
StartupNotify=true
EOF

echo "==> Updating caches..."
update-desktop-database "$HOME/.local/share/applications/" 2>/dev/null || true
gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor/" 2>/dev/null || true

echo "==> Done. Plick should appear in your app grid."
