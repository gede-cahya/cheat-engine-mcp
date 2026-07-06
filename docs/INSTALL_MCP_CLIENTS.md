# Install cheat-engine-mcp ke MCP Client

`cheat-engine-mcp` adalah MCP server stdio. Semua client memakai binary yang sama; bedanya hanya lokasi file konfigurasi.

## 1. Install binary

### Linux

```bash
sudo apt update && sudo apt install -y git cargo scanmem gdb
# Arch: sudo pacman -S git rust scanmem gdb

git clone https://github.com/gede-cahya/cheat-engine-mcp.git
cd cheat-engine-mcp
./install.sh
~/.local/bin/cheat-engine-mcp --help
```

### Windows

```powershell
git clone https://github.com/gede-cahya/cheat-engine-mcp.git
cd cheat-engine-mcp
cargo build --release
New-Item -ItemType Directory -Force C:\Tools | Out-Null
Copy-Item .\target\release\cheat-engine-mcp.exe C:\Tools\cheat-engine-mcp.exe
C:\Tools\cheat-engine-mcp.exe --help
```

Windows mode mendukung tool portable seperti `ping`, cheat table, workspace, IL2CPP artifact search, dan report. Tool `/proc`, `scanmem`, dan GDB attach adalah Linux-only.

## 2. Tambahkan config MCP

### Google Antigravity

Linux/macOS: `~/.gemini/config/settings.json`

Windows: `%USERPROFILE%\.gemini\config\settings.json`

```json
{
  "mcpServers": {
    "cheat-engine-mcp": {
      "command": "/home/USER/.local/bin/cheat-engine-mcp"
    }
  }
}
```

Windows:

```json
{
  "mcpServers": {
    "cheat-engine-mcp": {
      "command": "C:\\Tools\\cheat-engine-mcp.exe"
    }
  }
}
```

### Claude Code

Per repo: `.claude/settings.json`

```json
{
  "mcpServers": {
    "cheat-engine-mcp": {
      "command": "/home/USER/.local/bin/cheat-engine-mcp"
    }
  }
}
```

Install skill Claude Code:

```bash
mkdir -p YOUR_REPO/.claude/skills
cp -r .claude/skills/cheat-engine-mcp YOUR_REPO/.claude/skills/
cp CLAUDE.md YOUR_REPO/CLAUDE.md
```

### Claude Desktop

Linux: `~/.config/Claude/claude_desktop_config.json`

macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`

Windows: `%APPDATA%\Claude\claude_desktop_config.json`

Gunakan format `mcpServers` yang sama seperti di atas.

### Cursor / Windsurf / client MCP lain

Letakkan konfigurasi di file MCP client masing-masing, biasanya `.cursor/mcp.json`, `.windsurf/mcp_config.json`, atau config global client. Isi tetap:

```json
{
  "mcpServers": {
    "cheat-engine-mcp": {
      "command": "/absolute/path/to/cheat-engine-mcp"
    }
  }
}
```

## 3. Restart dan test

Restart client AI lalu tanya:

```text
Use cheat-engine-mcp and run the ping tool. Then list available reverse-engineering workflows safely without attaching to a real process.
```
