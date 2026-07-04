# Roadmap Fitur cheat-engine-mcp

## Phase 1 — Core MCP Stabil

Status: selesai.

- [x] Rust MCP server via stdio
- [x] `ping`
- [x] `scanmem_version`
- [x] `list_processes`
- [x] `scanmem_script_preview`
- [x] `scanmem_exact_scan`
- [x] `scanmem_write_value`
- [x] Perbaiki error response agar lebih rapi untuk AI
- [x] Tambah schema input/output yang konsisten

## Phase 2 — Scan Flow Lengkap

Tujuan: AI bisa melakukan proses scan bertahap seperti Cheat Engine.

- [x] `scanmem_attach_process`
- [x] `scanmem_reset_scan`
- [x] `scanmem_scan_exact`
- [x] `scanmem_scan_increased`
- [x] `scanmem_scan_decreased`
- [x] `scanmem_scan_changed`
- [x] `scanmem_scan_unchanged`
- [x] `scanmem_list_matches`
- [x] `scanmem_pick_match`

Contoh flow:

```text
attach PID
scan exact 100
ubah value di game
scan decreased
list matches
write value
```

## Phase 3 — Session Mode

Tujuan: scan tidak selalu mulai dari nol.

- [x] Session per PID
- [x] Simpan state scan sementara
- [x] `session_create`
- [x] `session_status`
- [x] `session_close`
- [x] Timeout session otomatis
- [x] Satu session aktif per process

Catatan: session saat ini in-memory di proses MCP; belum menjaga child `scanmem` interaktif hidup terus.

## Phase 4 — Write Safety

Tujuan: write memory tetap aman dan tidak asal merusak process.

- [x] Wajib `confirm_write=true`
- [x] Validasi PID masih hidup
- [x] Validasi hasil scan tidak kosong
- [x] Write hanya lewat hasil scan `current_value`, bukan address bebas
- [x] Preview sebelum write
- [x] Backup old value lewat field `current_value` / `backup_old_value` di output
- [x] `dry_run=true`
- [x] Limit jumlah address yang bisa ditulis

Tool target:

- [x] `scanmem_preview_write`
- [x] `scanmem_write_selected`
- [x] `scanmem_freeze_value`
- [x] `scanmem_unfreeze_value`

Catatan: freeze saat ini marker session + write sekali; belum background loop persistent.

## Phase 5 — Value Type Support

Tujuan: tidak hanya angka biasa.

- [x] int32
- [x] int64
- [x] float
- [x] double
- [x] string scan
- [x] hex value
- [x] unknown initial value
- [x] range scan

Tool target:

- [x] `scanmem_scan_by_type`
- [x] `scanmem_scan_range`
- [x] `scanmem_scan_unknown`

## Phase 6 — Process UX

Tujuan: AI gampang memilih target process.

- [x] Filter process by name
- [x] Detail process
- [x] Detect game/window process
- [x] Exclude system process
- [x] Tampilkan command line process
- [x] Rekomendasi PID terbaik jika ada banyak process mirip

Tool target:

- [x] `process_search`
- [x] `process_info`
- [x] `process_suggest_target`

## Phase 7 — Cheat Table Lite

Tujuan: punya versi ringan dari `.CT`.

Contoh format:

```json
{
  "game": "example-game",
  "process": "example",
  "entries": [
    {
      "name": "Health",
      "scan": "exact",
      "value_type": "int32",
      "last_value": 100
    }
  ]
}
```

- [x] Save cheat profile
- [x] Load cheat profile
- [x] Named entries
- [x] Notes per entry
- [x] Export/import JSON

Tool target:

- [x] `table_create`
- [x] `table_add_entry`
- [x] `table_load`
- [x] `table_save`
- [x] `table_list_entries`

## Phase 8 — AI-Friendly Explain Mode

Tujuan: model AI mudah paham kondisi scan.

- [x] Output ringkas
- [x] Field standar: `ok`, `message`, `data`, `next_suggestion`
- [x] Jelaskan langkah berikutnya
- [x] Warning jika terlalu banyak result
- [x] Human-readable summary

Catatan: semua output tool sekarang membawa `summary` dan `warnings` selain field standar.

## Phase 9 — Testing & Demo Target

Tujuan: bisa dites tanpa game asli.

- [x] Buat dummy target Rust kecil
- [x] Variable health/coins berubah tiap beberapa detik
- [x] Test scan exact
- [x] Test write value
- [x] Test freeze value

Folder target:

```text
examples/
  dummy-target/
```

Catatan: test scan/write/freeze memakai dummy target secara manual karena akses memory process bergantung permission OS (`ptrace_scope`/sudo).

## Phase 10 — Packaging

Tujuan: gampang dipakai di MCP client.

- [x] Release binary Linux x86_64
- [x] Contoh config Claude Desktop / MCP client
- [x] Install script
- [x] README usage lengkap
- [x] Troubleshooting permission Linux

Contoh config:

```json
{
  "mcpServers": {
    "scanmem": {
      "command": "/path/to/cheat-engine-mcp"
    }
  }
}
```

## Prioritas Terbaik

1. Session Mode
2. Refine scan: increased/decreased/changed
3. List matches
4. Write selected match
5. Dummy target untuk testing
6. Cheat Table Lite
7. Freeze value

## Fitur Selanjutnya yang Disarankan

Lanjut buat:

```text
scanmem session mode + reset/next scan
```

Alasan: tanpa session, fitur scan bertahap seperti Cheat Engine belum enak dipakai.
