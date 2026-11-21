#!/usr/bin/env bash
# Build hkferry from this checkout and install it into ~/.local/bin.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${HKFERRY_BIN_DIR:-$HOME/.local/bin}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required: https://rustup.rs" >&2
  exit 1
fi

cargo build --release --locked --manifest-path "$ROOT/Cargo.toml"
mkdir -p "$DEST"
install -m 0755 "$ROOT/target/release/hkferry" "$DEST/hkferry"
printf 'installed %s\n' "$DEST/hkferry"
case ":$PATH:" in
  *":$DEST:"*) ;;
  *) echo "note: add $DEST to PATH" >&2 ;;
esac
