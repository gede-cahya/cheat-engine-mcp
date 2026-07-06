# Prompt installer skill + MCP cheat-engine-mcp

Copy prompt ini ke Antigravity, Claude Code, Claude Desktop, Cursor, Windsurf, atau agent lain yang bisa mengubah file lokal.

## Linux installer prompt

```text
Kamu adalah installer agent. Install cheat-engine-mcp MCP server dan skill-nya dengan aman.

Target repo: https://github.com/gede-cahya/cheat-engine-mcp
OS: Linux

Langkah wajib:
1. Cek dependency: git, cargo, scanmem, gdb. Jika apt tersedia jalankan:
   sudo apt update && sudo apt install -y git cargo scanmem gdb
   Jika pacman tersedia jalankan:
   sudo pacman -S --needed git rust scanmem gdb
2. Clone atau update repo:
   git clone https://github.com/gede-cahya/cheat-engine-mcp.git ~/cheat-engine-mcp
   jika folder sudah ada: cd ~/cheat-engine-mcp && git pull
3. Build/install:
   cd ~/cheat-engine-mcp && ./install.sh
4. Tambahkan MCP server bernama "cheat-engine-mcp" ke config client saat ini dengan command:
   ~/.local/bin/cheat-engine-mcp
5. Jika client mendukung skill, install skill dari:
   ~/cheat-engine-mcp/.claude/skills/cheat-engine-mcp/SKILL.md
6. Restart client dan test tool ping/list tools. Jangan attach ke proses game, jangan write memory, jangan hook GDB saat test.
```

## Windows installer prompt

```text
Kamu adalah installer agent. Install cheat-engine-mcp MCP server dan skill-nya dengan aman.

Target repo: https://github.com/gede-cahya/cheat-engine-mcp
OS: Windows

Langkah wajib:
1. Pastikan Git dan Rust/Cargo tersedia. Jika winget tersedia:
   winget install --id Git.Git -e
   winget install --id Rustlang.Rustup -e
2. Clone atau update repo:
   git clone https://github.com/gede-cahya/cheat-engine-mcp.git $env:USERPROFILE\cheat-engine-mcp
   jika folder sudah ada: cd $env:USERPROFILE\cheat-engine-mcp; git pull
3. Build dan copy binary:
   cd $env:USERPROFILE\cheat-engine-mcp
   cargo build --release
   New-Item -ItemType Directory -Force C:\Tools | Out-Null
   Copy-Item .\target\release\cheat-engine-mcp.exe C:\Tools\cheat-engine-mcp.exe
4. Tambahkan MCP server bernama "cheat-engine-mcp" ke config client saat ini dengan command:
   C:\Tools\cheat-engine-mcp.exe
5. Jika client mendukung skill, install/copy skill dari:
   $env:USERPROFILE\cheat-engine-mcp\.claude\skills\cheat-engine-mcp\SKILL.md
6. Restart client dan test tool ping/list tools. Ingat: Windows hanya portable mode; scanmem, /proc memory scan, dan GDB attach adalah Linux-only.
```

## Prompt pemakaian setelah install

```text
Gunakan skill cheat-engine-mcp. Mulai dengan read-only: ping server, cari proses target, tampilkan workspace/report/cheat-table yang sudah ada, lalu buat rencana. Jangan write memory atau hook GDB sebelum preview dan konfirmasi eksplisit.
```
