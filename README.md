# cheat-engine-mcp

MCP server ringan berbasis Rust untuk membungkus `scanmem` di Linux: scan value, refine, guarded write, process search, dan cheat table JSON.

## Fitur

- MCP stdio: `initialize`, `tools/list`, `tools/call`.
- Scan flow: exact, increased, decreased, changed, unchanged, unknown, range, typed value.
- Session in-memory per PID.
- Write safety: `confirm_write`, live PID check, preview, dry-run, max writes, backup field.
- Process UX: search/info/suggest target.
- Cheat Table Lite: save/load JSON di `.cheat-tables/`.
- Dummy target test di `examples/dummy-target`.

## Dependency OS

Install Rust dan `scanmem`.

```bash
# Arch
sudo pacman -S scanmem rust

# Debian/Ubuntu
sudo apt update
sudo apt install -y scanmem cargo
```

## Build release Linux x86_64

```bash
cargo build --release
./target/release/cheat-engine-mcp
```

Binary release lokal ada di:

```text
target/release/cheat-engine-mcp
```

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

Claude Desktop / MCP client:

```json
{
  "mcpServers": {
    "scanmem": {
      "command": "/home/USER/.local/bin/cheat-engine-mcp"
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

## Tool utama

- `ping`
- `scanmem_version`
- `list_processes`
- `process_search`, `process_info`, `process_suggest_target`
- `session_create`, `session_status`, `session_close`
- `scanmem_scan_exact`, `scanmem_scan_increased`, `scanmem_scan_decreased`, `scanmem_scan_changed`, `scanmem_scan_unchanged`
- `scanmem_scan_by_type`, `scanmem_scan_range`, `scanmem_scan_unknown`
- `scanmem_preview_write`, `scanmem_write_selected`, `scanmem_freeze_value`, `scanmem_unfreeze_value`
- `table_create`, `table_add_entry`, `table_load`, `table_save`, `table_list_entries`

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
