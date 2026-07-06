# Release notes

## v0.2.0

### Included

- Phase 21 reverse reports: `reverse_report_create`, `reverse_report_add_finding`, `reverse_report_list`.
- Local ignored JSON + Markdown reports under `reverse/<game>/tools/reports/`.
- Windows release binary that runs MCP portable tools.
- GitHub Actions CI for Linux and Windows.
- Tag-triggered release workflow for Linux `.tar.gz` and Windows `.zip` assets.

### Platform notes

- Linux: full `scanmem`, `/proc`, memory read, and GDB helper support.
- Windows: portable MCP/file tools only for now; process memory, `scanmem`, and GDB tools return unsupported errors.

### Verification

```bash
cargo fmt --check
cargo test
cargo check
cargo build --release
(cd examples/dummy-target && cargo check)
```

## v0.1.0

Initial release of `cheat-engine-mcp`.

### Included

- MCP stdio server for Linux `scanmem`.
- Process listing, search, info, and target suggestion tools.
- Scan flow: exact, refine, changed/unchanged, increased/decreased, range, unknown, typed values.
- In-memory sessions per PID with timeout.
- Guarded write tools with confirmation, dry-run, preview, max-write limit, and backup field.
- Lightweight freeze/unfreeze state.
- Cheat Table Lite JSON save/load.
- AI-friendly output fields: `summary`, `warnings`, and `next_suggestion`.
- Dummy Rust target for manual testing.
- Install script and MCP client config examples.

### Verification

```bash
cargo fmt --check
cargo test
cargo check
cargo build --release
(cd examples/dummy-target && cargo check)
```
