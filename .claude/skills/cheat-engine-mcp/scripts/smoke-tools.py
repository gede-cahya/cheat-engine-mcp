#!/usr/bin/env python3
"""Small MCP smoke runner for cheat-engine-mcp.

Runs safe protocol checks only. Use full live scan/write checks manually when ptrace
and a dummy target are available.
"""

import json
import select
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
MCP = ROOT / "target" / "debug" / "cheat-engine-mcp"


def rpc(proc, request, timeout=10):
    proc.stdin.write(json.dumps(request) + "\n")
    proc.stdin.flush()
    if not select.select([proc.stdout], [], [], timeout)[0]:
        raise TimeoutError(request["method"])
    return json.loads(proc.stdout.readline())


def main():
    subprocess.run(["cargo", "build"], cwd=ROOT, check=True)
    proc = subprocess.Popen(
        [str(MCP)],
        cwd=ROOT,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
    )
    try:
        init = rpc(proc, {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}})
        tools = rpc(proc, {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
        ping = rpc(
            proc,
            {
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {"name": "ping", "arguments": {}},
            },
        )
        names = [tool["name"] for tool in tools["result"]["tools"]]
        assert init["result"]["serverInfo"]["name"] == "cheat-engine-mcp"
        assert "ping" in names
        assert "scanmem_version" in names
        assert not ping["result"]["isError"]
        print(f"ok: {len(names)} tools advertised")
    finally:
        proc.terminate()
        proc.wait(timeout=2)


if __name__ == "__main__":
    sys.exit(main())
