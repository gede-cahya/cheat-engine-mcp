# 🌌 Setting Up cheat-engine-mcp with Google Antigravity

This guide explains how to install and integrate `cheat-engine-mcp` with the **Google Antigravity** agentic AI coding assistant.

---

## 🛠️ Prerequisites

* **Linux**: Rust (`cargo`), `scanmem`, and `gdb` must be installed.
* **Windows**: Rust (`cargo`) must be installed (only supports portable tools like cheat tables and workspaces).

---

## 🚀 1. Install Using the One-Click Installer

The easiest way to set up everything is by running the one-click installer script:

### Linux
```bash
./install-antigravity.sh
```

### Windows (PowerShell)
```powershell
.\install-antigravity.ps1
```

The script will automatically:
1. Compile the release binary and install it to the path (`~/.local/bin/` on Linux, `C:\Tools\` on Windows).
2. Configure the MCP server inside your global Antigravity configuration (`settings.json`).
3. Install the **cheat-engine-mcp skill definition** to `~/.gemini/config/skills/cheat-engine-mcp/`.

---

## ✍️ 2. Manual Installation & Integration

If you prefer to set up manually, follow these steps:

### Step 1: Build the Binary
```bash
# Build the project
cargo build --release

# Linux: Install to local bin directory
mkdir -p ~/.local/bin
cp target/release/cheat-engine-mcp ~/.local/bin/

# Windows: Copy to target tools folder
mkdir -p C:\Tools
cp target/release/cheat-engine-mcp.exe C:\Tools\
```

### Step 2: Configure the MCP Server

Add the server configuration to your global Antigravity settings:

* **Linux Config Path**: `~/.gemini/config/settings.json`
* **Windows Config Path**: `%USERPROFILE%\.gemini\config\settings.json`

Add the following block to your `settings.json`:

```json
{
  "mcpServers": {
    "cheat-engine-mcp": {
      "command": "/home/YOUR_USERNAME/.local/bin/cheat-engine-mcp"
    }
  }
}
```
*(On Windows, replace command path with `"C:\\Tools\\cheat-engine-mcp.exe"`).*

### Step 3: Install the Skill Definition

Antigravity uses "Skills" to learn how to operate specific tools.

1. Create the skill directory:
   ```bash
   mkdir -p ~/.gemini/config/skills/cheat-engine-mcp
   ```
2. Copy the skill file:
   ```bash
   cp skills/antigravity/SKILL.md ~/.gemini/config/skills/cheat-engine-mcp/SKILL.md
   ```
3. Copy project agent rules:
   Add the contents of `skills/antigravity/AGENTS.md` to your workspace customization rules file (`.agents/AGENTS.md` or `~/.gemini/config/AGENTS.md` globally).

---

## 🔍 Verification

1. Start/restart your Antigravity agent.
2. Ask the agent:
   > "ping cheat-engine-mcp"
3. If successful, the agent will reply with `pong` and list the available tools.
