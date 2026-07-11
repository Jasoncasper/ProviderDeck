#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:?Usage: $0 <version> [repo] [body]}"
REPO="${2:-${GITHUB_REPOSITORY:-YOUR_GITHUB_USERNAME/ProviderDeck}}"
BODY="${3:-}"
if [[ "$VERSION" == v* ]]; then TAG="$VERSION"; else TAG="v${VERSION}"; fi

DMG_AARCH64="ProviderDeck-${VERSION}-macos-aarch64.dmg"
DMG_X86_64="ProviderDeck-${VERSION}-macos-x86_64.dmg"

DOWNLOAD_BASE="https://github.com/${REPO}/releases/download/${TAG}"

cat <<EOF
{
  "version": "${VERSION}",
  "tag_name": "${TAG}",
  "url": "https://github.com/${REPO}/releases/tag/${TAG}",
  "body": $(printf '%s' "$BODY" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))'),
  "assets": [
    {
      "name": "${DMG_AARCH64}",
      "url": "${DOWNLOAD_BASE}/${DMG_AARCH64}",
      "browser_download_url": "${DOWNLOAD_BASE}/${DMG_AARCH64}"
    },
    {
      "name": "${DMG_X86_64}",
      "url": "${DOWNLOAD_BASE}/${DMG_X86_64}",
      "browser_download_url": "${DOWNLOAD_BASE}/${DMG_X86_64}"
    }
  ]
}
EOF
