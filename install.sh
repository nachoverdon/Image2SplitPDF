#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
BIN_NAME="image2splitpdf"
DESKTOP_NAME="image2splitpdf.desktop"
ICON_NAME="image2splitpdf.svg"

BIN_DEST="$HOME/.local/bin"
DESKTOP_DEST="$HOME/.local/share/applications"
ICON_DEST="$HOME/.local/share/icons/hicolor/scalable/apps"

echo "==> Building Image2SplitPDF in release mode..."
cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml"

echo "==> Cleaning up any legacy Image2SplittedPDF files..."
rm -f "$BIN_DEST/image2splittedpdf" "$DESKTOP_DEST/image2splittedpdf.desktop" "$ICON_DEST/image2splittedpdf.svg"

echo "==> Creating destination directories..."
mkdir -p "$BIN_DEST" "$DESKTOP_DEST" "$ICON_DEST"

echo "==> Installing binary to $BIN_DEST/$BIN_NAME..."
cp "$SCRIPT_DIR/target/release/Image2SplitPDF" "$BIN_DEST/$BIN_NAME"
chmod +x "$BIN_DEST/$BIN_NAME"

echo "==> Installing icon to $ICON_DEST/$ICON_NAME..."
cp "$SCRIPT_DIR/assets/$ICON_NAME" "$ICON_DEST/$ICON_NAME"

echo "==> Installing desktop entry to $DESKTOP_DEST/$DESKTOP_NAME..."
cp "$SCRIPT_DIR/assets/$DESKTOP_NAME" "$DESKTOP_DEST/$DESKTOP_NAME"

echo "==> Updating desktop and icon caches..."
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$DESKTOP_DEST"
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true
fi

echo ""
echo "✅ Installation complete!"
echo "Image2SplitPDF is now installed and accessible in Omarchy:"
echo "  • Open launcher: Press SUPER + ALT + SPACE (Apps menu) or SUPER + SPACE (Omarchy menu)"
echo "  • Type: Image2SplitPDF"
echo "  • CLI command: $BIN_NAME [optional_image_path]"
