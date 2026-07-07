#!/usr/bin/env bash
set -euo pipefail

echo "=== Installing cheat-engine-mcp for Google Antigravity (Linux) ==="

# Build the release binary
echo "1. Building release binary..."
cargo build --release

# Install binary to ~/.local/bin/
BIN_DIR="$HOME/.local/bin"
mkdir -p "$BIN_DIR"
cp target/release/cheat-engine-mcp "$BIN_DIR/"
echo "installed: $BIN_DIR/cheat-engine-mcp"

# Copy skill definition
SKILL_DIR="$HOME/.gemini/config/skills/cheat-engine-mcp"
echo "2. Copying skill to $SKILL_DIR..."
mkdir -p "$SKILL_DIR"
cp skills/antigravity/SKILL.md "$SKILL_DIR/"
echo "installed: $SKILL_DIR/SKILL.md"

# Configure settings.json
SETTINGS_FILE="$HOME/.gemini/config/settings.json"
echo "3. Configuring $SETTINGS_FILE..."

# Create settings.json if it doesn't exist
if [ ! -f "$SETTINGS_FILE" ]; then
    mkdir -p "$(dirname "$SETTINGS_FILE")"
    echo '{"mcpServers": {}}' > "$SETTINGS_FILE"
fi

# Merge using Python (safer than jq since python is usually installed on Linux)
python3 -c "
import json
import os

path = os.path.expanduser('~/.gemini/config/settings.json')
with open(path, 'r') as f:
    data = json.load(f)

if 'mcpServers' not in data:
    data['mcpServers'] = {}

data['mcpServers']['cheat-engine-mcp'] = {
    'command': os.path.expanduser('~/.local/bin/cheat-engine-mcp')
}

with open(path, 'w') as f:
    json.dump(data, f, indent=2)
"

echo "=== Installation Completed Successfully! ==="
echo "Restart your Antigravity agent and test by typing: 'ping cheat-engine-mcp'"
