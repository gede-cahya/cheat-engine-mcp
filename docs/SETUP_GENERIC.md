# 🔌 Generic MCP Client Setup for cheat-engine-mcp

`cheat-engine-mcp` adheres to the official **Model Context Protocol (MCP)** specification. You can use it with any generic MCP client that supports stdio transport.

---

## 🛠️ Protocol Details

* **Transport Layer**: Stdio (Standard Input / Standard Output)
* **Message Format**: JSON-RPC 2.0
* **Framing Protocol**: Stdio JSON-RPC over stdout/stdin

---

## 🚀 Interactive Test via Command Line

You can manually interact with the MCP server to verify it is working correctly.

Run the binary directly and send JSON-RPC payloads:

### 1. Initialize Connection
Send the `initialize` JSON-RPC method request.

**Command:**
```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | cheat-engine-mcp
```

**Expected Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "protocolVersion": "2024-11-05",
    "capabilities": {},
    "serverInfo": {
      "name": "cheat-engine-mcp",
      "version": "0.3.0"
    }
  },
  "id": 1
}
```

### 2. List All Tools
Request the list of available tools.

**Command:**
```bash
printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | cheat-engine-mcp
```

**Expected Response:**
A JSON object listing all 72 tools under `tools`.

### 3. Call a Tool (e.g., ping)
Call the `ping` tool to ensure backend communication is healthy.

**Command:**
```bash
printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ping","arguments":{}}}' | cheat-engine-mcp
```

**Expected Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "content": [
      {
        "type": "text",
        "text": "pong"
      }
    ]
  },
  "id": 3
}
```

---

## 🤖 Client Implementations

To implement a custom client:
1. Spawn the `cheat-engine-mcp` binary as a child process.
2. Direct client output (JSON-RPC requests) to the child's `stdin`.
3. Read the child's `stdout` line-by-line to parse server responses.
4. Ensure `stderr` is logged separately for diagnostics (the server prints raw log lines to stderr to avoid corrupting the JSON-RPC stdio channel).
