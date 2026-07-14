#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP="$ROOT/apps/providerdeck-manager"
ICONS="$APP/src-tauri/icons"
SOURCE="$ICONS/providerdeck-source.svg"
TRAY_SOURCE="$ICONS/tray-template.svg"
TEMP_DIR="$(mktemp -d)"

cleanup() {
  rm -rf "$TEMP_DIR"
}
trap cleanup EXIT

(
  cd "$APP"
  npx tauri icon "$SOURCE" --output "$ICONS"
  npx tauri icon --png 44 "$TRAY_SOURCE" --output "$TEMP_DIR"
)

cp "$ICONS/icon.png" "$APP/public/app-icon.png"
cp "$TEMP_DIR/44x44.png" "$ICONS/tray-template.png"
