#!/usr/bin/env bash
set -euo pipefail

BIN_DEST="$HOME/.local/bin/image2splitpdf"
DESKTOP_DEST="$HOME/.local/share/applications/image2splitpdf.desktop"
ICON_DEST="$HOME/.local/share/icons/hicolor/scalable/apps/image2splitpdf.svg"

echo "==> Removing installed files..."
rm -f "$BIN_DEST" "$DESKTOP_DEST" "$ICON_DEST"
# Also clean up legacy files if any
rm -f "$HOME/.local/bin/image2splittedpdf" "$HOME/.local/share/applications/image2splittedpdf.desktop" "$HOME/.local/share/icons/hicolor/scalable/apps/image2splittedpdf.svg"

echo "==> Refreshing desktop and icon databases..."
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$HOME/.local/share/applications"
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true
fi

echo "✅ Image2SplitPDF has been uninstalled."
