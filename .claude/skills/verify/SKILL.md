# Verify cheat-engine-mcp

Run the release binary as an MCP stdio server and send JSON-RPC lines.

```bash
cargo build --release
{
  printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
  printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
  printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ping","arguments":{}}}'
} | ./target/release/cheat-engine-mcp
```

For portable report flow, create a temp workspace under `reverse/verify-*/tools`, call `reverse_report_create`, `reverse_report_add_finding`, then `reverse_report_list`, and delete the temp workspace afterward.

Probe path safety with bad `report` like `..\\bad` and bad `root` like `reverse/../bad`; both should return MCP `isError:true`.
