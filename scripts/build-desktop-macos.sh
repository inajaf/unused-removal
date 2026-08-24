#!/usr/bin/env bash
# Build the macOS desktop app (.app bundle + optional .dmg)
# Usage: ./scripts/build-desktop-macos.sh [--dmg]
set -euo pipefail

APP_NAME="unused-removal"
BUNDLE_ID="com.inajaf.unused-removal"
VERSION="1.0.0"

echo "▸ Building release binary (feature: desktop)..."
cargo build --release --features desktop

BIN="target/release/${APP_NAME}"
[ -f "$BIN" ] || { echo "binary not found: $BIN"; exit 1; }

APP="target/release/${APP_NAME}.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>              <string>Unused Removal</string>
    <key>CFBundleDisplayName</key>       <string>Unused Removal</string>
    <key>CFBundleIdentifier</key>        <string>${BUNDLE_ID}</string>
    <key>CFBundleVersion</key>           <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key><string>${VERSION}</string>
    <key>CFBundleExecutable</key>        <string>${APP_NAME}</string>
    <key>CFBundlePackageType</key>       <string>APPL</string>
    <key>LSMinimumSystemVersion</key>    <string>10.15</string>
    <key>NSHighResolutionCapable</key>   <true/>
    <key>LSApplicationCategoryType</key> <string>public.app-category.utilities</string>
</dict>
</plist>
PLIST

cp "$BIN" "$APP/Contents/MacOS/${APP_NAME}"

# Optional custom icon: put a 1024x1024 icon.png next to this script
if [ -f "scripts/icon.png" ]; then
    sips -s format icns scripts/icon.png --out "$APP/Contents/Resources/app.icns" >/dev/null
    /usr/libexec/PlistBuddy -c "Add :CFBundleIconFile string app.icns" "$APP/Contents/Info.plist"
fi

# Minimal code signing so Gatekeeper allows local run
codesign --force --sign - "$APP" >/dev/null 2>&1 || true

echo "✔ Built $APP"
[ "${1:-}" = "--dmg" ] && {
    hdiutil create -volname "Unused Removal" -srcfolder "$APP" -ov -format UDZO \
        "target/release/${APP_NAME}-${VERSION}.dmg" >/dev/null
    echo "✔ Built target/release/${APP_NAME}-${VERSION}.dmg"
}
