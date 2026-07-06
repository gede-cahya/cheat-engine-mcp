# Rules & Instalasi cheat-engine-mcp

## Rules pemakaian

1. Pakai hanya untuk process/game milik sendiri atau yang kamu punya izin untuk uji.
2. Jangan target process system/root kecuali benar-benar paham risikonya.
3. Selalu mulai dari tool read-only/preview:
   - `process_search`
   - `process_info`
   - `scanmem_preview_write`
   - `gdb_hook_preview`
   - `gdb_probe_preview`
4. Write memory wajib pakai `confirm_write:true` dan idealnya `dry_run:true` dulu.
5. Batasi jumlah write dengan `max_writes`; jangan write kalau match terlalu banyak.
6. Simpan hasil reverse lokal di `reverse/<game>/tools/`; folder `reverse/` di-ignore dan tidak untuk dipush.
7. GDB hook/probe wajib pakai `confirm_hook:true` / `confirm_probe:true` hanya setelah script dicek.
8. Windows v0.3.0 hanya mendukung MCP portable tools: `ping`, table/report, workspace, IL2CPP artifact search. Tool `scanmem`, memory process, `/proc`, dan GDB masih Linux-only.

## Instalasi dari release

### Linux x86_64

Download asset:

```text
cheat-engine-mcp-v0.3.0-x86_64-unknown-linux-gnu.tar.gz
```

Install dependency:

```bash
# Arch
sudo pacman -S scanmem gdb

# Debian/Ubuntu
sudo apt update
sudo apt install -y scanmem gdb
```

Extract dan install:

```bash
tar -xzf cheat-engine-mcp-v0.3.0-x86_64-unknown-linux-gnu.tar.gz
install -Dm755 cheat-engine-mcp ~/.local/bin/cheat-engine-mcp
```

### Windows x86_64

Download asset:

```text
cheat-engine-mcp-v0.3.0-x86_64-pc-windows-msvc.zip
```

Extract ke contoh path:

```text
C:\Tools\cheat-engine-mcp.exe
```

Catatan: Windows binary bisa dipakai untuk MCP portable tools, bukan memory scanning.

## Build dari source

```bash
git clone https://github.com/gede-cahya/cheat-engine-mcp.git
cd cheat-engine-mcp
cargo build --release
```

Linux binary:

```text
target/release/cheat-engine-mcp
```

Windows binary:

```text
target\release\cheat-engine-mcp.exe
```

## Config MCP client

### Linux

```json
{
  "mcpServers": {
    "scanmem": {
      "command": "/home/USER/.local/bin/cheat-engine-mcp"
    }
  }
}
```

### Windows

```json
{
  "mcpServers": {
    "scanmem": {
      "command": "C:\\Tools\\cheat-engine-mcp.exe"
    }
  }
}
```

## Cek instalasi

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | cheat-engine-mcp
printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ping","arguments":{}}}' | cheat-engine-mcp
```

Expected:

```text
"version":"0.3.0"
"message":"pong"
```

## Troubleshooting Linux

Cek permission ptrace:

```bash
cat /proc/sys/kernel/yama/ptrace_scope
```

Untuk development sementara:

```bash
sudo sysctl kernel.yama.ptrace_scope=0
```

Balikkan ke default lebih aman:

```bash
sudo sysctl kernel.yama.ptrace_scope=1
```

Jika masih gagal:

- jalankan MCP dan target dengan user yang sama;
- pastikan PID masih hidup;
- pastikan `scanmem --version` dan `gdb --version` jalan;
- ulangi dengan preview/dry-run sebelum write nyata.
