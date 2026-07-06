# cheat-engine-mcp project rules

- Automatically use the `cheat-engine-mcp` skill for requests about operating, reverse-engineering, testing, validating, installing, configuring, packaging, releasing, or troubleshooting this repo or its MCP tools.
- Keep memory/process actions authorized and safe: preview/read-only first; real writes require `confirm_write:true`, `max_writes`, and preferably `dry_run:true` first.
- GDB attach/hook/probe requires preview first, then explicit `confirm_hook:true` or `confirm_probe:true`.
- Do not commit or expose local artifacts under `reverse/` or `.cheat-tables/`.
- For code changes, run at least: `cargo fmt --check`, `cargo test`, `cargo check`, and `git diff --check`.
