# cheat-engine-mcp

MCP server ringan berbasis Rust untuk membungkus `scanmem` di Linux: scan value, refine, guarded write, process search, dan cheat table JSON.

## Fitur

- MCP stdio: `initialize`, `tools/list`, `tools/call`.
- Scan flow: exact, increased, decreased, changed, unchanged, unknown, range, typed value.
- Session in-memory per PID.
- Write safety: `confirm_write`, live PID check, preview, dry-run, max writes, backup field.
- Process UX: search/info/suggest target, module base/RVA helper.
- Cheat Table Lite: save/load JSON + module/RVA watchlist di `.cheat-tables/`.
- Reverse report lokal ignored di `reverse/<game>/tools/reports/`.
- Dummy target test di `examples/dummy-target`.

## Dependency OS

Install Rust, `scanmem`, dan `gdb`.

```bash
# Arch
sudo pacman -S scanmem gdb rust

# Debian/Ubuntu
sudo apt update
sudo apt install -y scanmem gdb cargo
```

## Build release

Linux:

```bash
cargo build --release
./target/release/cheat-engine-mcp
```

Windows:

```powershell
cargo build --release
.\target\release\cheat-engine-mcp.exe
```

Catatan Windows: binary MCP bisa jalan untuk tool portable (`ping`, table/report, workspace, IL2CPP artifact search). Tool memory/process/GDB/`scanmem` masih Linux-only dan akan mengembalikan error unsupported.

## Install lokal

```bash
./install.sh
```

Default install ke:

```text
~/.local/bin/cheat-engine-mcp
```

Override prefix:

```bash
PREFIX=/usr/local sudo -E ./install.sh
```

## Config MCP client

Claude Desktop / MCP client Linux:

```json
{
  "mcpServers": {
    "scanmem": {
      "command": "/home/USER/.local/bin/cheat-engine-mcp"
    }
  }
}
```

Windows:

```json
{
  "mcpServers": {
    "scanmem": {
      "command": "C:\\Tools\\cheat-engine-mcp.exe"
    }
  }
}
```

Contoh file tersedia di:

- `docs/claude-desktop.example.json`
- `docs/mcp-client.example.json`

## Test manual MCP

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | cargo run -q
printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | cargo run -q
printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"scanmem_version","arguments":{}}}' | cargo run -q
printf '%s\n' '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"process_search","arguments":{"query":"dummy"}}}' | cargo run -q
```

## Dummy target

Terminal 1:

```bash
cd examples/dummy-target
cargo run
```

Terminal 2 gunakan PID yang dicetak dummy target:

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"scanmem_scan_exact","arguments":{"pid":1234,"value":"100"}}}' | cargo run -q
```

## GDB hook lite

Preview dulu, lalu start hanya dengan `confirm_hook:true`:

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"gdb_hook_preview","arguments":{"pid":1234,"module":"libtarget.so","rva":"0x1234","commands":["printf \"hit\\n\""]}}}' | cargo run -q
```

Stop dengan `gdb_hook_stop` pakai `hook_id` dari `gdb_hook_start`.

Multi-breakpoint hook pakai satu GDB script:

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"gdb_hook_group_preview","arguments":{"pid":1234,"breakpoints":[{"name":"hit-a","module":"libtarget.so","rva":"0x1234","commands":["printf \"hit a\\n\""]},{"name":"hit-b","module":"libtarget.so","rva":"0x5678","commands":["printf \"hit b\\n\""]}]}}}' | cargo run -q
printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"gdb_hook_group_start","arguments":{"pid":1234,"confirm_hook":true,"breakpoints":[{"module":"libtarget.so","rva":"0x1234","commands":["printf \"hit a\\n\""]},{"module":"libtarget.so","rva":"0x5678","commands":["printf \"hit b\\n\""]}]}}}' | cargo run -q
printf '%s\n' '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"gdb_hook_group_stop","arguments":{"group_id":"gdb-hook-group-1234-..."}}}' | cargo run -q
```

Probe read-only auto-stop setelah `max_hits` (default 5):

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"gdb_probe_preview","arguments":{"pid":1234,"module":"libtarget.so","rva":"0x1234","max_hits":3}}}' | cargo run -q
printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"gdb_probe_start","arguments":{"pid":1234,"module":"libtarget.so","rva":"0x1234","max_hits":3,"confirm_probe":true}}}' | cargo run -q
printf '%s\n' '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"gdb_probe_stop","arguments":{"probe_id":"gdb-probe-1234-..."}}}' | cargo run -q
```

Verifikasi alamat/RVA sebelum hook:

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"process_read_maps","arguments":{"pid":1234,"limit":5}}}' | cargo run -q
printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"rva_disassemble_preview","arguments":{"pid":1234,"module":"libtarget.so","rva":"0x1234"}}}' | cargo run -q
printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"memory_read_bytes","arguments":{"pid":1234,"address":"0x7fff1234","count":16}}}' | cargo run -q
printf '%s\n' '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"gdb_breakpoint_probe_preview","arguments":{"pid":1234,"module":"libtarget.so","rva":"0x1234"}}}' | cargo run -q
```

## Local reverse artifacts

Artifact reverse bisa disimpan lokal per game di `reverse/<game>/tools/`. Folder `reverse/` di-ignore, jadi dump tidak ikut GitHub.

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"workspace_list","arguments":{}}}' | cargo run -q
printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"workspace_set_active","arguments":{"workspace":"game"}}}' | cargo run -q
printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"il2cpp_method_search","arguments":{"query":"Health"}}}' | cargo run -q
printf '%s\n' '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"il2cpp_class_search","arguments":{"query":"Hero"}}}' | cargo run -q
printf '%s\n' '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"il2cpp_find_by_rva","arguments":{"rva":"0x1234"}}}' | cargo run -q
```

Bisa juga per-call tanpa active workspace:

```json
{"game":"game","query":"Health"}
```

- `workspace_list`, `workspace_status`, `workspace_set_active`
- `il2cpp_artifacts_status`
- `il2cpp_method_search`, `il2cpp_string_search`, `il2cpp_script_search`
- `il2cpp_class_search`, `il2cpp_field_search`, `il2cpp_method_detail`, `il2cpp_find_by_rva`, `il2cpp_related_methods`

`root` manual di bawah `reverse/` masih didukung untuk kompatibilitas.

Report reverse lokal:

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"reverse_report_create","arguments":{"game":"game","report":"combat","title":"Combat notes","summary":"Ringkasan aman"}}}' | cargo run -q
printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"reverse_report_add_finding","arguments":{"game":"game","report":"combat","title":"Health setter","summary":"Candidate method","module":"GameAssembly.dll","rva":"0x1234"}}}' | cargo run -q
printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"reverse_report_list","arguments":{"game":"game"}}}' | cargo run -q
```

Report tersimpan di `reverse/<game>/tools/reports/` sebagai JSON + Markdown, tetap ignored oleh Git.

## Cheat table watchlist

Entry table bisa simpan hasil reverse (`module` + `rva`, `method_signature`, `scan_query`) lalu resolve ke address live PID:

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"table_add_entry","arguments":{"table":"game-proc","name":"health","scan":"100","value_type":"int32","module":"GameAssembly.dll","rva":"0x1234","method_signature":"Hero::Health","scan_query":"Health"}}}' | cargo run -q
printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"table_resolve_entries","arguments":{"table":"game-proc","pid":1234}}}' | cargo run -q
printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"table_validate_entries","arguments":{"table":"game-proc","pid":1234,"read_values":true}}}' | cargo run -q
```

## Tool utama

- `ping`
- `scanmem_version`
- `list_processes`
- `process_search`, `process_info`, `process_suggest_target`
- `process_list_modules`, `process_read_maps`, `process_module_base`, `rva_to_address`, `address_to_rva`
- `gdb_hook_preview`, `gdb_hook_start`, `gdb_hook_stop`, `gdb_hook_group_preview`, `gdb_hook_group_start`, `gdb_hook_group_stop`, `gdb_probe_preview`, `gdb_probe_start`, `gdb_probe_stop`, `rva_disassemble_preview`, `gdb_disassemble_address`, `gdb_breakpoint_probe_preview`
- `memory_read_bytes`, `memory_read_int`, `memory_read_float`, `memory_read_string`
- `workspace_list`, `workspace_status`, `workspace_set_active`
- `reverse_report_create`, `reverse_report_add_finding`, `reverse_report_list`
- `il2cpp_artifacts_status`, `il2cpp_method_search`, `il2cpp_string_search`, `il2cpp_script_search`
- `il2cpp_class_search`, `il2cpp_field_search`, `il2cpp_method_detail`, `il2cpp_find_by_rva`, `il2cpp_related_methods`
- `session_create`, `session_status`, `session_close`
- `scanmem_scan_exact`, `scanmem_scan_increased`, `scanmem_scan_decreased`, `scanmem_scan_changed`, `scanmem_scan_unchanged`
- `scanmem_scan_by_type`, `scanmem_scan_range`, `scanmem_scan_unknown`
- `scanmem_preview_write`, `scanmem_write_selected`, `scanmem_freeze_value`, `scanmem_unfreeze_value`
- `table_create`, `table_add_entry`, `table_resolve_entries`, `table_validate_entries`, `table_load`, `table_save`, `table_list_entries`

## Troubleshooting permission Linux

`scanmem` perlu permission untuk membaca/menulis memory process lain.

Cek ptrace:

```bash
cat /proc/sys/kernel/yama/ptrace_scope
```

Untuk development sementara:

```bash
sudo sysctl kernel.yama.ptrace_scope=0
```

Balikkan default lebih aman:

```bash
sudo sysctl kernel.yama.ptrace_scope=1
```

Jika tetap gagal:

- jalankan MCP dan target dengan user yang sama;
- jangan target process system/root tanpa izin;
- pastikan PID masih hidup;
- install `scanmem` dan cek `scanmem --version`;
- gunakan `scanmem_preview_write` dan `dry_run:true` sebelum write nyata.

## Testing

```bash
cargo fmt --check
cargo test
cargo check
cargo build --release
(cd examples/dummy-target && cargo check)
```
