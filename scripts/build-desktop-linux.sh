#!/usr/bin/env bash
# Build Linux desktop artifacts: binary + .desktop entry + optional AppImage
# Requirements: webkit2gtk-4.1 dev packages (Debian/Ubuntu:
#   sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file)
set -euo pipefail

APP_NAME="unused-removal"
VERSION="1.0.0"

echo "▸ Building release binary (feature: desktop)..."
cargo build --release --features desktop

BIN="target/release/${APP_NAME}"
[ -f "$BIN" ] || { echo "binary not found: $BIN"; exit 1; }

# Portable tar.gz with launcher
STAGE="target/release/${APP_NAME}-linux"
rm -rf "$STAGE"
mkdir -p "$STAGE/usr/bin" "$STAGE/usr/share/applications"

cp "$BIN" "$STAGE/usr/bin/"
cat > "$STAGE/usr/share/applications/${APP_NAME}.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Unused Removal
Comment=Fast disk cleanup — find and remove junk files safely
Exec=/usr/bin/${APP_NAME}
Icon=${APP_NAME}
Categories=Utility;System;
Terminal=false
DESKTOP

tar -czf "target/release/${APP_NAME}-${VERSION}-linux.tar.gz" -C "$STAGE" usr
echo "✔ Built target/release/${APP_NAME}-${VERSION}-linux.tar.gz"

# Optional: AppImage if appimagetool is available
if command -v appimagetool >/dev/null 2>&1; then
    APPDIR="target/release/AppDir"
    rm -rf "$APPDIR"
    mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications"
    cp "$BIN" "$APPDIR/usr/bin/"
    cp "$STAGE/usr/share/applications/${APP_NAME}.desktop" "$APPDIR/"
    touch "$APPDIR/${APP_NAME}.png" # replace with a real 256x256 icon if available
    appimagetool "$APPDIR" "target/release/${APP_NAME}-${VERSION}-x86_64.AppImage" >/dev/null
    echo "✔ Built target/release/${APP_NAME}-${VERSION}-x86_64.AppImage"
else
    echo "ℹ appimagetool not found — skipped AppImage (tar.gz is ready)"
fi
