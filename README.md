# 🎮 cheat-engine-mcp

[![CI](https://github.com/gede-cahya/cheat-engine-mcp/actions/workflows/ci.yml/badge.svg)](https://github.com/gede-cahya/cheat-engine-mcp/actions/workflows/ci.yml)
[![Release](https://github.com/gede-cahya/cheat-engine-mcp/actions/workflows/release.yml/badge.svg)](https://github.com/gede-cahya/cheat-engine-mcp/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

**AI-powered game memory scanner and reverse engineering toolkit** — an MCP (Model Context Protocol) server that gives AI coding assistants direct access to `scanmem`, `gdb`, and IL2CPP reverse engineering tools.

> Turn your AI assistant into a game hacking partner. Scan memory, hook functions, search IL2CPP metadata, and modify game values — all through natural language.

---

## ✨ Features

| Category | Tools | Description |
|---|---|---|
| 🔍 **Memory Scanning** | `scanmem_scan_*`, `session_*` | Exact, range, type-based, increased/decreased/changed value scanning |
| ✏️ **Memory Writing** | `scanmem_write_*`, `scanmem_freeze_*` | Safe guarded writes with preview, dry-run, confirmation, and persistent freeze |
| 🔬 **GDB Hooks** | `gdb_hook_*`, `gdb_probe_*` | Dynamic function hooking, breakpoint probes, disassembly preview |
| 📖 **Memory Reading** | `memory_read_*` | Read bytes, ints, floats, and strings from process memory |
| 🧬 **IL2CPP Reverse** | `il2cpp_*` | Search classes, methods, fields, strings, and RVA in Unity IL2CPP dumps |
| 📊 **Cheat Tables** | `table_*` | Save/load/resolve/validate cheat entries with module+RVA tracking |
| 📋 **Reports** | `reverse_report_*` | Create and manage local reverse engineering reports per game |
| 🎯 **Process Utils** | `process_*`, `rva_*` | Process search, module listing, RVA/address conversion, memory maps |

**72 MCP tools** in total — all accessible via natural language through any MCP-compatible AI assistant.

---

## 🚀 Quick Start

### 1. Install Dependencies

```bash
# Arch Linux
sudo pacman -S scanmem gdb rust

# Debian / Ubuntu
sudo apt update && sudo apt install -y scanmem gdb cargo

# macOS (Homebrew) — limited support
brew install rust
```

### 2. Build & Install

```bash
git clone https://github.com/gede-cahya/cheat-engine-mcp.git
cd cheat-engine-mcp
./install.sh
```

This builds the release binary and installs it to `~/.local/bin/cheat-engine-mcp`.

### 3. Connect to Your AI Assistant

Choose your AI assistant and follow [Install cheat-engine-mcp ke MCP Client](docs/INSTALL_MCP_CLIENTS.md).

| AI Assistant | Config | Skill support |
|---|---|---|
| **Google Antigravity** | `~/.gemini/config/settings.json` | Copy `SKILL.md` into Antigravity/Gemini skills folder if enabled |
| **Claude Code** | `.claude/settings.json` per repo | `.claude/skills/cheat-engine-mcp/SKILL.md` |
| **Claude Desktop** | `claude_desktop_config.json` | Use project instructions / prompt installer |
| **Cursor / Windsurf / Generic MCP** | Client MCP config file | Use the prompt installer or custom rules |

**Linux one-liner:**

```bash
git clone https://github.com/gede-cahya/cheat-engine-mcp.git
cd cheat-engine-mcp && ./install.sh
```

**Windows build:**

```powershell
git clone https://github.com/gede-cahya/cheat-engine-mcp.git
cd cheat-engine-mcp; cargo build --release
```

Need an agent to install it for you? Copy [docs/SKILL_INSTALL_PROMPT.md](docs/SKILL_INSTALL_PROMPT.md) into Antigravity, Claude Code, or another coding agent.

---

## 🔧 MCP Client Configuration

All MCP-compatible clients use the same server binary. The only difference is where the config file lives.

### Config Format

```json
{
  "mcpServers": {
    "cheat-engine-mcp": {
      "command": "/home/USER/.local/bin/cheat-engine-mcp"
    }
  }
}
```

### Config File Locations

| Client | Linux | macOS | Windows |
|---|---|---|---|
| **Antigravity** | `~/.gemini/config/settings.json` | `~/.gemini/config/settings.json` | `%USERPROFILE%\.gemini\config\settings.json` |
| **Claude Code** | `.claude/settings.json` (per-repo) | Same | Same |
| **Claude Desktop** | `~/.config/Claude/claude_desktop_config.json` | `~/Library/Application Support/Claude/claude_desktop_config.json` | `%APPDATA%\Claude\claude_desktop_config.json` |
| **Cursor** | `.cursor/mcp.json` (per-project) | Same | Same |

### Windows Note

Windows binary supports **portable tools only**: `ping`, cheat tables, workspaces, IL2CPP artifact search, and reports. Memory scanning (`scanmem`), process memory, `/proc`, and GDB tools are **Linux-only**.

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

## 🧠 Skill / Plugin Installation

For the best experience, install the **skill definition** so your AI assistant knows *how* to use the tools effectively.

### Google Antigravity

```bash
mkdir -p ~/.gemini/config/skills/cheat-engine-mcp
cp .claude/skills/cheat-engine-mcp/SKILL.md ~/.gemini/config/skills/cheat-engine-mcp/SKILL.md
```

### Claude Code

```bash
mkdir -p YOUR_REPO/.claude/skills
cp -r .claude/skills/cheat-engine-mcp/ YOUR_REPO/.claude/skills/cheat-engine-mcp/
cp CLAUDE.md YOUR_REPO/CLAUDE.md
```

---

## 🎯 Usage Examples

Once configured, just talk to your AI assistant naturally:

### Find and modify a game value

```
"Scan game TaskbarHero for health value 100, then set it to 999"
```

The AI will:
1. `process_search` → find the game PID
2. `session_create` → create a scan session
3. `scanmem_scan_exact` → scan for value 100
4. Refine with `scanmem_scan_decreased` / `scanmem_scan_unchanged`
5. `scanmem_preview_write` → preview the write
6. `scanmem_write_selected` → apply with safety guards

### Reverse engineer a Unity/IL2CPP game

```
"Search for Hero class methods in TaskbarHero's IL2CPP dump"
```

The AI will:
1. `workspace_set_active` → set game workspace
2. `il2cpp_class_search` → find Hero class
3. `il2cpp_method_search` → find relevant methods
4. `il2cpp_field_search` → find class fields and offsets
5. `il2cpp_find_by_rva` → map RVA addresses

### Hook a game function with GDB

```
"Hook the damage function at RVA 0x958ADC in GameAssembly.dll and multiply damage by 1000"
```

The AI will:
1. `rva_disassemble_preview` → inspect the function
2. `gdb_hook_preview` → preview the hook script
3. `gdb_hook_start` → attach and hook (with confirmation)
4. Monitor via log output

---

## 🛠️ Tool Reference

### Process & Memory

| Tool | Description |
|---|---|
| `process_search` | Search running processes by name |
| `process_info` | Get detailed process information |
| `process_suggest_target` | AI-friendly target suggestion |
| `process_list_modules` | List loaded modules/DLLs |
| `process_read_maps` | Read `/proc/PID/maps` |
| `process_module_base` | Get module base address |
| `rva_to_address` | Convert RVA to absolute address |
| `address_to_rva` | Convert absolute address to RVA |
| `memory_read_bytes` | Read raw bytes from memory |
| `memory_read_int` | Read integer value |
| `memory_read_float` | Read float value |
| `memory_read_string` | Read string from memory |

### Scanning

| Tool | Description |
|---|---|
| `session_create` | Create scan session for a PID |
| `session_status` | Check session state |
| `session_close` | Close session and cleanup |
| `scanmem_scan_exact` | Scan for exact value |
| `scanmem_scan_increased` | Filter: value increased |
| `scanmem_scan_decreased` | Filter: value decreased |
| `scanmem_scan_changed` | Filter: value changed |
| `scanmem_scan_unchanged` | Filter: value unchanged |
| `scanmem_scan_unknown` | Initial unknown value scan |
| `scanmem_scan_range` | Scan value range |
| `scanmem_scan_by_type` | Typed scan (int32/float/string/etc.) |

### Writing & Freezing

| Tool | Description |
|---|---|
| `scanmem_preview_write` | Preview write operation (safe) |
| `scanmem_write_selected` | Write to matched addresses |
| `scanmem_freeze_value` | Freeze value (one-shot or persistent) |
| `scanmem_unfreeze_value` | Stop freezing |

### GDB Hooks

| Tool | Description |
|---|---|
| `gdb_hook_preview` | Preview single hook script |
| `gdb_hook_start` | Start single GDB hook |
| `gdb_hook_stop` | Stop hook and detach |
| `gdb_hook_group_preview` | Preview multi-breakpoint hook |
| `gdb_hook_group_start` | Start hook group |
| `gdb_hook_group_stop` | Stop hook group |
| `gdb_probe_preview` | Preview read-only probe |
| `gdb_probe_start` | Start probe (auto-stops after N hits) |
| `gdb_probe_stop` | Stop probe |
| `rva_disassemble_preview` | Disassemble at RVA |
| `gdb_disassemble_address` | Disassemble at absolute address |
| `gdb_breakpoint_probe_preview` | Preview breakpoint probe |

### IL2CPP Reverse Engineering

| Tool | Description |
|---|---|
| `il2cpp_artifacts_status` | Check dump.cs availability |
| `il2cpp_class_search` | Search classes by name |
| `il2cpp_method_search` | Search methods by name |
| `il2cpp_field_search` | Search fields by name |
| `il2cpp_string_search` | Search string literals |
| `il2cpp_script_search` | Search MonoBehaviour scripts |
| `il2cpp_method_detail` | Get method details |
| `il2cpp_find_by_rva` | Find method by RVA |
| `il2cpp_related_methods` | Find related methods |

### Workspace & Reports

| Tool | Description |
|---|---|
| `workspace_list` | List all game workspaces |
| `workspace_status` | Current workspace status |
| `workspace_set_active` | Set active workspace |
| `workspace_clear_active` | Clear active workspace |
| `reverse_report_create` | Create reverse report |
| `reverse_report_add_finding` | Add finding to report |
| `reverse_report_list` | List reports for a game |

### Cheat Tables

| Tool | Description |
|---|---|
| `table_create` | Create new cheat table |
| `table_add_entry` | Add entry to table |
| `table_list_entries` | List table entries |
| `table_resolve_entries` | Resolve RVAs to addresses |
| `table_validate_entries` | Validate entries against live process |
| `table_load` | Load table from file |
| `table_save` | Save table to file |

---

## 🔒 Safety Design

This tool is built with **defense-in-depth safety**:

1. **Preview First** — All destructive operations have a preview/dry-run mode
2. **Explicit Confirmation** — Writes require `confirm_write: true`, hooks require `confirm_hook: true`
3. **Write Limits** — `max_writes` caps the number of addresses modified
4. **Dry Run** — `dry_run: true` simulates without touching memory
5. **GDB Command Whitelist** — Only `set`, `printf`, `if/else/end` allowed in hook scripts
6. **Session Timeout** — Scan sessions auto-expire after 30 minutes
7. **Live PID Check** — Validates process is alive before any operation

---

## 🏗️ Building from Source

### Linux

```bash
git clone https://github.com/gede-cahya/cheat-engine-mcp.git
cd cheat-engine-mcp
cargo build --release
# Binary: ./target/release/cheat-engine-mcp
```

### Windows

```powershell
git clone https://github.com/gede-cahya/cheat-engine-mcp.git
cd cheat-engine-mcp
cargo build --release
# Binary: .\target\release\cheat-engine-mcp.exe
```

### Install to PATH

```bash
./install.sh
# Installs to ~/.local/bin/cheat-engine-mcp

# Custom prefix:
PREFIX=/usr/local sudo -E ./install.sh
```

---

## 🧪 Testing

```bash
cargo fmt --check
cargo test
cargo check
cargo build --release
(cd examples/dummy-target && cargo check)
```

### Manual MCP Test

```bash
# Initialize
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | cargo run -q

# List tools
printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | cargo run -q

# Ping
printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ping","arguments":{}}}' | cargo run -q
```

### Dummy Target

```bash
# Terminal 1: Start dummy target
cd examples/dummy-target && cargo run

# Terminal 2: Scan with PID from terminal 1
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"scanmem_scan_exact","arguments":{"pid":PID,"value":"100"}}}' | cargo run -q
```

---

## 🐧 Linux Troubleshooting

### ptrace Permission

```bash
# Check current setting
cat /proc/sys/kernel/yama/ptrace_scope

# Allow ptrace (for development)
sudo sysctl kernel.yama.ptrace_scope=0

# Restore default (more secure)
sudo sysctl kernel.yama.ptrace_scope=1
```

### Common Issues

| Problem | Solution |
|---|---|
| `scanmem` not found | Install: `sudo apt install scanmem` or `sudo pacman -S scanmem` |
| `gdb` not found | Install: `sudo apt install gdb` or `sudo pacman -S gdb` |
| Permission denied | Run MCP and target as same user; check ptrace_scope |
| PID not found | Ensure target process is running; check with `pgrep` |
| Too many matches | Use refine scans (increased/decreased/unchanged) |

---

## 📁 Project Structure

```
cheat-engine-mcp/
├── src/main.rs              # MCP server (single-file, 72 tools)
├── Cargo.toml               # Rust project config
├── install.sh               # Linux install script
├── install-antigravity.sh   # Antigravity one-click installer
├── install-antigravity.ps1  # Antigravity Windows installer
├── install-claude-code.sh   # Claude Code one-click installer
├── install-claude-code.ps1  # Claude Code Windows installer
├── skills/
│   └── antigravity/         # Antigravity skill definition
│       ├── SKILL.md
│       └── AGENTS.md
├── .claude/
│   └── skills/
│       └── cheat-engine-mcp/  # Claude Code skill definition
│           └── SKILL.md
├── docs/
│   ├── SETUP_ANTIGRAVITY.md
│   ├── SETUP_CLAUDE_CODE.md
│   ├── SETUP_CLAUDE_DESKTOP.md
│   ├── SETUP_CURSOR.md
│   ├── SETUP_GENERIC.md
│   ├── RULES_INSTALL.md
│   ├── RELEASE.md
│   └── *.example.json
├── examples/
│   └── dummy-target/        # Test target for scanning
├── reverse/                 # Local reverse artifacts (gitignored)
├── .cheat-tables/           # Local cheat tables (gitignored)
├── .github/workflows/       # CI + Release automation
├── CLAUDE.md                # Claude Code project rules
├── README.md
├── ROADMAP.md
└── LICENSE
```

---

## 📜 License

[MIT License](LICENSE) © gede-cahya

---

## 🤝 Contributing

1. Fork the repo
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Run tests (`cargo fmt --check && cargo test && cargo check`)
4. Commit your changes (`git commit -m 'Add amazing feature'`)
5. Push to the branch (`git push origin feature/amazing-feature`)
6. Open a Pull Request
