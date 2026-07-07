# 🖥️ Setting Up cheat-engine-mcp with Claude Desktop

This guide explains how to connect `cheat-engine-mcp` to the **Claude Desktop App** (Windows and macOS/Linux).

---

## 🛠️ Configuration

To connect this MCP server, you need to add it to the `claude_desktop_config.json` file.

### 1. Locate the Configuration File

Depending on your Operating System, the configuration file is located at:

* **macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
* **Windows**: `%APPDATA%\Claude\claude_desktop_config.json` (or `C:\Users\YOUR_USER\AppData\Roaming\Claude\claude_desktop_config.json`)
* **Linux**: `~/.config/Claude/claude_desktop_config.json`

If the file does not exist, you can create it.

---

## 🚀 2. Edit Configuration

Open the `claude_desktop_config.json` and add the `cheat-engine-mcp` definition under the `mcpServers` object:

### Linux / macOS Configuration
```json
{
  "mcpServers": {
    "cheat-engine-mcp": {
      "command": "/home/YOUR_USERNAME/.local/bin/cheat-engine-mcp"
    }
  }
}
```

### Windows Configuration
```json
{
  "mcpServers": {
    "cheat-engine-mcp": {
      "command": "C:\\Tools\\cheat-engine-mcp.exe"
    }
  }
}
```

---

## 🔍 Verification

1. Fully restart the **Claude Desktop App** (close from the system tray as well).
2. Look for the "plug" icon 🔌 in the chat UI. Clicking it should list `cheat-engine-mcp` or show its tools.
3. Test by asking:
   > "ping cheat-engine-mcp" or "list active processes"
