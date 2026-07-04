#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFIX="${PREFIX:-$HOME/.local}"
BIN_DIR="$PREFIX/bin"
BIN="$BIN_DIR/cheat-engine-mcp"

need() { command -v "$1" >/dev/null 2>&1 || { echo "missing: $1" >&2; exit 1; }; }
need cargo
need scanmem

cd "$ROOT"
cargo build --release
mkdir -p "$BIN_DIR"
install -m 0755 "$ROOT/target/release/cheat-engine-mcp" "$BIN"

cat <<EOF
installed: $BIN

MCP config example:
{
  "mcpServers": {
    "scanmem": {
      "command": "$BIN"
    }
  }
}
EOF
