#!/usr/bin/env bash
# Print the absolute path of a working hkferry binary.
set -euo pipefail

if [ -n "${HKFERRY_BIN:-}" ] && [ -x "$HKFERRY_BIN" ]; then
  printf '%s\n' "$HKFERRY_BIN"
  exit 0
fi

if command -v hkferry >/dev/null 2>&1; then
  command -v hkferry
  exit 0
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ -f "$ROOT/Cargo.toml" ] && command -v cargo >/dev/null 2>&1; then
  (cd "$ROOT" && cargo build --release --locked >/dev/null)
  BIN="$ROOT/target/release/hkferry"
  if [ -x "$BIN" ]; then
    printf '%s\n' "$BIN"
    exit 0
  fi
fi

echo "hkferry: no binary found and no Cargo build available" >&2
echo "Set HKFERRY_BIN, put hkferry on PATH, or build with: cargo build --release --locked" >&2
exit 1
