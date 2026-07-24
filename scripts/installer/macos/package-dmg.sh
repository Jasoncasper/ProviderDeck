#!/usr/bin/env bash
set -euo pipefail

ARCH="${2:-$(uname -m)}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
VERSION="${1:-$(awk -F '"' '/^version = / { print $2; exit }' "$ROOT/Cargo.toml")}"
if [[ -z "$VERSION" ]]; then
  echo "Unable to resolve package version" >&2
  exit 1
fi
DIST="$ROOT/dist/macos"
STAGE="$DIST/stage"
BINARY_DIR="${BINARY_DIR:-$ROOT/target/release}"
DMG="$DIST/ProviderDeck-${VERSION}-macos-${ARCH}.dmg"
ICON_SOURCE="$ROOT/apps/providerdeck-manager/src-tauri/icons/icon.png"
ICON_NAME="providerdeck.icns"
ICON_ICNS="$DIST/$ICON_NAME"

rm -rf "$STAGE"
mkdir -p "$STAGE"

prepare_icon() {
  local iconset="$DIST/providerdeck.iconset"
  rm -rf "$iconset"
  mkdir -p "$iconset"

  sips -z 16 16 "$ICON_SOURCE" --out "$iconset/icon_16x16.png" >/dev/null
  sips -z 32 32 "$ICON_SOURCE" --out "$iconset/icon_16x16@2x.png" >/dev/null
  sips -z 32 32 "$ICON_SOURCE" --out "$iconset/icon_32x32.png" >/dev/null
  sips -z 64 64 "$ICON_SOURCE" --out "$iconset/icon_32x32@2x.png" >/dev/null
  sips -z 128 128 "$ICON_SOURCE" --out "$iconset/icon_128x128.png" >/dev/null
  sips -z 256 256 "$ICON_SOURCE" --out "$iconset/icon_128x128@2x.png" >/dev/null
  sips -z 256 256 "$ICON_SOURCE" --out "$iconset/icon_256x256.png" >/dev/null
  sips -z 512 512 "$ICON_SOURCE" --out "$iconset/icon_256x256@2x.png" >/dev/null
  sips -z 512 512 "$ICON_SOURCE" --out "$iconset/icon_512x512.png" >/dev/null
  sips -z 1024 1024 "$ICON_SOURCE" --out "$iconset/icon_512x512@2x.png" >/dev/null

  iconutil -c icns "$iconset" -o "$ICON_ICNS" || cp "$ROOT/apps/providerdeck-manager/src-tauri/icons/icon.icns" "$ICON_ICNS"
}

create_app() {
  local app_name="$1"
  local executable_name="$2"
  local binary_path="$3"
  local bundle_id="$4"
  local lsui_element="${5:-false}"
  local app_dir="$STAGE/$app_name.app"

  mkdir -p "$app_dir/Contents/MacOS" "$app_dir/Contents/Resources"
  cp "$binary_path" "$app_dir/Contents/MacOS/$executable_name"
  cp "$ICON_ICNS" "$app_dir/Contents/Resources/$ICON_NAME"
  chmod +x "$app_dir/Contents/MacOS/$executable_name"
  cat > "$app_dir/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>$app_name</string>
  <key>CFBundleDisplayName</key>
  <string>$app_name</string>
  <key>CFBundleIdentifier</key>
  <string>$bundle_id</string>
  <key>CFBundleVersion</key>
  <string>$VERSION</string>
  <key>CFBundleShortVersionString</key>
  <string>$VERSION</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleExecutable</key>
  <string>$executable_name</string>
  <key>CFBundleIconFile</key>
  <string>$ICON_NAME</string>
  <key>LSMinimumSystemVersion</key>
  <string>12.0</string>
  <key>LSUIElement</key>
  <$lsui_element/>
</dict>
</plist>
PLIST
}

copy_helper_binary() {
  local app_dir="$1"
  local executable_name="$2"
  local binary_path="$3"

  cp "$binary_path" "$app_dir/Contents/MacOS/$executable_name"
  chmod +x "$app_dir/Contents/MacOS/$executable_name"
}

sign_app() {
  local app_dir="$1"
  codesign --force --deep --sign - "$app_dir"
}

prepare_icon
create_app "ProviderDeck" "providerdeck-manager" "$BINARY_DIR/providerdeck-manager" "com.jasoncasper.providerdeck" "false"
copy_helper_binary "$STAGE/ProviderDeck.app" "providerdeck" "$BINARY_DIR/providerdeck"
cp -R "$ROOT/apps/providerdeck-manager/dist" "$STAGE/ProviderDeck.app/Contents/dist"
ln -s /Applications "$STAGE/Applications"

sign_app "$STAGE/ProviderDeck.app"

hdiutil create -volname "ProviderDeck" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
echo "$DMG"
