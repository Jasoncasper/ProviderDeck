#!/usr/bin/env bash
set -euo pipefail

CODEX_BIN="${CODEX_BIN:-$(command -v codex)}"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

"$CODEX_BIN" app-server generate-json-schema --out "$TMP_DIR" >/dev/null
REQUEST_SCHEMA="$TMP_DIR/ClientRequest.json"
RESUME_SCHEMA="$TMP_DIR/v2/ThreadResumeResponse.json"

for required in 'thread/resume' 'modelProvider' 'config'; do
  if ! grep -q "$required" "$REQUEST_SCHEMA"; then
    echo "missing required app-server request field: $required" >&2
    exit 1
  fi
done

for required in 'modelProvider' 'model' 'thread'; do
  if ! grep -q "\"$required\"" "$RESUME_SCHEMA"; then
    echo "missing required thread/resume response field: $required" >&2
    exit 1
  fi
done

echo "app-server schema supports ProviderDeck thread/provider rebinding"
