#!/usr/bin/env bash
set -euo pipefail

echo "=== Installing cheat-engine-mcp for Claude Code (Linux) ==="

# Build the release binary
echo "1. Building release binary..."
cargo build --release

# Install binary to ~/.local/bin/
BIN_DIR="$HOME/.local/bin"
mkdir -p "$BIN_DIR"
cp target/release/cheat-engine-mcp "$BIN_DIR/"
echo "installed: $BIN_DIR/cheat-engine-mcp"

# Register with Claude Code CLI
echo "2. Registering with Claude Code..."
if command -v claude >/dev/null 2>&1; then
    claude mcp add cheat-engine-mcp "$BIN_DIR/cheat-engine-mcp"
    echo "Registered with Claude Code successfully!"
else
    echo "WARNING: 'claude' command not found. You will need to manually add it:"
    echo "claude mcp add cheat-engine-mcp $BIN_DIR/cheat-engine-mcp"
fi

# Print instructions for workspace integration
echo "=== Installation Completed! ==="
echo "To copy Claude Code skills to your active workspace, run:"
echo "mkdir -p /path/to/target/.claude/skills/"
echo "cp -r .claude/skills/cheat-engine-mcp/ /path/to/target/.claude/skills/cheat-engine-mcp/"
echo "cp CLAUDE.md /path/to/target/CLAUDE.md"
