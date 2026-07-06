---
name: cheat-engine-mcp
description: Use this skill automatically for this repository whenever the user asks to operate, reverse, test, validate, install, configure, package, release, or troubleshoot cheat-engine-mcp or its MCP tools.
---

# cheat-engine-mcp project workflow

## Auto-use triggers

Use this skill before answering when the request mentions any of:

- cheat-engine-mcp, scanmem MCP, MCP tool list, MCP JSON-RPC, Claude Desktop config
- install, setup, build, release, package, binary, Windows/Linux support
- test all tools, smoke test, verify, validate, dummy target, gdb, scanmem
- reverse, IL2CPP, workspace, report, table, RVA, module, hook, probe, memory read/write
- safety rules, usage rules, dry-run, confirm_write, confirm_hook, confirm_probe

## Safety boundary

This project is defensive/authorized tooling only. Do not help target processes/games without user authorization. Keep destructive writes guarded:

- Prefer read-only tools first: `process_search`, `process_info`, `workspace_status`, `il2cpp_*_search`, `memory_read_*`, `*_preview`.
- Real memory write requires `confirm_write:true`, live PID, `max_writes`, and preferably `dry_run:true` first.
- GDB attach/hook requires preview first, then `confirm_hook:true` or `confirm_probe:true`.
- Never commit or expose files under `reverse/` or `.cheat-tables/`; they are local artifacts.

## Common commands

```bash
cargo fmt --check
cargo test
cargo check
cargo build --release
(cd examples/dummy-target && cargo check)
git diff --check
```

Manual MCP smoke:

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | cargo run -q
printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | cargo run -q
printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ping","arguments":{}}}' | cargo run -q
```

## Test policy

For normal changes, run the common commands. For tool/schema/session changes, run a live MCP smoke against `target/debug/cheat-engine-mcp` and `examples/dummy-target`.

When testing write/freeze behavior, avoid real writes unless explicitly asked. Use `dry_run:true`; use a stable dummy value for scanmem match counting.

## Project facts

- Rust MCP stdio server in `src/main.rs`.
- Dummy test target in `examples/dummy-target`.
- Local reverse artifacts live under ignored `reverse/<game>/tools/`.
- Cheat tables live under ignored `.cheat-tables/`.
- Active workspace state lives at `reverse/.active-workspace`.
- Windows binary supports portable tools; scanmem, `/proc`, memory process, and GDB are Linux-only.

## Minimal implementation rule

Keep changes boring and small. Prefer updating existing `src/main.rs`, README, ROADMAP, and docs over adding scaffolding. If adding non-trivial logic, add one focused unit test or smoke check.
