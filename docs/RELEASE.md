# Release notes

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
