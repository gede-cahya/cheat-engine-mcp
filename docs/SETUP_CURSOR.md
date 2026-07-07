# 🖱️ Setting Up cheat-engine-mcp with Cursor AI

This guide explains how to integrate `cheat-engine-mcp` with the **Cursor** editor.

---

## 🛠️ Configuration

You can configure the MCP server directly inside Cursor's Graphical Interface:

### Step 1: Open Settings
1. Open Cursor.
2. Go to **Settings** (Gear icon in top right, or `Ctrl + ,` / `Cmd + ,`).
3. Navigate to **Features** > **MCP**.

### Step 2: Add New MCP Server
1. Click on **+ Add New MCP Server**.
2. Fill in the details:
   * **Name**: `cheat-engine-mcp`
   * **Type**: `command`
   * **Command**:
     * **Linux**: `/home/YOUR_USERNAME/.local/bin/cheat-engine-mcp`
     * **Windows**: `C:\Tools\cheat-engine-mcp.exe`
3. Click **Save**.

---

## ✍️ Project-specific config (Alternative)

If you prefer repository-level configuration, create a file named `.cursor/mcp.json` in your workspace root:

```json
{
  "mcpServers": {
    "cheat-engine-mcp": {
      "command": "/home/YOUR_USERNAME/.local/bin/cheat-engine-mcp"
    }
  }
}
```

---

## 🔍 Verification

1. Once added, Cursor will show the status of the MCP server as a green dot `● Active`.
2. Open the Chat sidebar in Cursor (`Ctrl + L` or `Cmd + L`).
3. Test by typing:
   > "ping cheat-engine-mcp" or "list my running processes"
