# 🤖 Setting Up cheat-engine-mcp with Claude Code

This guide explains how to install and configure `cheat-engine-mcp` with **Claude Code** (the CLI tool).

---

## 🚀 1. Install Using the One-Click Installer

Run the automatic installer script to build the release binary, register it with Claude Code, and copy the skill definitions:

### Linux
```bash
./install-claude-code.sh
```

### Windows (PowerShell)
```powershell
.\install-claude-code.ps1
```

---

## ✍️ 2. Manual Installation & Integration

### Step 1: Build the Binary
```bash
cargo build --release
```

### Step 2: Register MCP Server with Claude Code

You can add the MCP server using Claude Code CLI command:
```bash
claude mcp add cheat-engine-mcp /absolute/path/to/target/release/cheat-engine-mcp
```

Alternatively, manually edit your Claude Desktop or Claude Code settings file (`.claude/settings.json` or global config):
```json
{
  "mcpServers": {
    "cheat-engine-mcp": {
      "command": "/absolute/path/to/cheat-engine-mcp"
    }
  }
}
```

### Step 3: Install Claude Code Skills & Rules

Claude Code utilizes project-specific skills and `CLAUDE.md` rules.

1. In the target workspace where you want to use the cheat tools, copy the skill definition folder:
   ```bash
   mkdir -p YOUR_TARGET_REPO/.claude/skills/
   cp -r .claude/skills/cheat-engine-mcp/ YOUR_TARGET_REPO/.claude/skills/cheat-engine-mcp/
   ```
2. Copy `CLAUDE.md` containing the project instructions to your target repo:
   ```bash
   cp CLAUDE.md YOUR_TARGET_REPO/CLAUDE.md
   ```

---

## 🔍 Verification

1. Run `claude` inside the target workspace.
2. Ask:
   > "ping cheat-engine-mcp"
3. Claude should respond with `pong` and be able to list all 72 memory hacking and IL2CPP reverse tools.
