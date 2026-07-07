# cheat-engine-mcp Rules

- Automatically trigger the `cheat-engine-mcp` skill for any request related to operating, reverse-engineering, testing, installing, or troubleshooting the repo or its MCP tools.
- Never write to memory or install hooks without calling preview tools first.
- Always use `confirm_write: true`, `max_writes` limits, and `dry_run: true` if applicable.
- GDB dynamic hooks require explicit `confirm_hook: true` or `confirm_probe: true`.
- Keep reverse findings in `.cheat-tables/` and `reverse/` directories. Do not commit or expose these local directories.
