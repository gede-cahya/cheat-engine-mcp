# Graph Report - /home/cahya/2026/cheat-engine-mcp  (2026-07-04)

## Corpus Check
- Corpus is ~5,234 words - fits in a single context window. You may not need a graph.

## Summary
- 79 nodes · 273 edges · 9 communities detected
- Extraction: 100% EXTRACTED · 0% INFERRED · 0% AMBIGUOUS
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- [[_COMMUNITY_Rust MCP Tools|Rust MCP Tools]]
- [[_COMMUNITY_Rust MCP Tools|Rust MCP Tools]]
- [[_COMMUNITY_Rust MCP Tools|Rust MCP Tools]]
- [[_COMMUNITY_Rust MCP Tools|Rust MCP Tools]]
- [[_COMMUNITY_Rust MCP Tools|Rust MCP Tools]]
- [[_COMMUNITY_Rust MCP Tools|Rust MCP Tools]]
- [[_COMMUNITY_Rust MCP Tools|Rust MCP Tools]]
- [[_COMMUNITY_Rust MCP Tools|Rust MCP Tools]]
- [[_COMMUNITY_Rust MCP Tools|Rust MCP Tools]]

## God Nodes (most connected - your core abstractions)
1. `ok()` - 41 edges
2. `call_tool()` - 35 edges
3. `tool_ok()` - 30 edges
4. `touch_session()` - 14 edges
5. `valid_pid()` - 13 edges
6. `run_scanmem_script()` - 13 edges
7. `valid_value_arg()` - 11 edges
8. `scanmem_scan_by_type()` - 10 edges
9. `scanmem_preview_write()` - 9 edges
10. `scanmem_scan_range()` - 9 edges

## Surprising Connections (you probably didn't know these)
- `main()` --calls--> `ok()`  [EXTRACTED]
  examples/dummy-target/src/main.rs → src/main.rs
- `main()` --calls--> `handle()`  [EXTRACTED]
  examples/dummy-target/src/main.rs → src/main.rs

## Communities (9 total, 1 thin omitted)

### Community 0 - "Rust MCP Tools"
Cohesion: 0.35
Nodes (19): count_matches(), guarded_write_inputs(), ok(), run_scanmem_script(), scanmem_attach_process(), scanmem_freeze_value(), scanmem_preview_write(), scanmem_refine_scan() (+11 more)

### Community 1 - "Rust MCP Tools"
Cohesion: 0.13
Nodes (6): parse_process_line(), Request, Session, sessions_are_one_per_pid(), typed_value(), write_requires_confirmation()

### Community 2 - "Rust MCP Tools"
Cohesion: 0.23
Nodes (12): call_tool(), output_warning(), process_info(), scanmem_pick_match(), scanmem_unfreeze_value(), scanmem_version(), session_close(), tool_content() (+4 more)

### Community 3 - "Rust MCP Tools"
Cohesion: 0.36
Nodes (10): read_json(), required_str(), safe_name(), table_add_entry(), table_create(), table_list_entries(), table_load(), table_path() (+2 more)

### Community 4 - "Rust MCP Tools"
Cohesion: 0.33
Nodes (5): error(), GameState, handle(), lists_scanmem_tools(), main()

### Community 5 - "Rust MCP Tools"
Cohesion: 0.4
Nodes (6): expire_sessions(), new_session(), now_secs(), session_create(), session_json(), session_status()

### Community 6 - "Rust MCP Tools"
Cohesion: 0.4
Nodes (5): previews_scanmem_script(), rejects_weird_scan_value(), scan_args(), scanmem_exact_scan(), scanmem_script_preview()

### Community 7 - "Rust MCP Tools"
Cohesion: 0.5
Nodes (4): command_output(), is_system_process(), list_processes(), process_search()

## Knowledge Gaps
- **3 isolated node(s):** `Request`, `Session`, `GameState`
  These have ≤1 connection - possible missing edges or undocumented components.
- **1 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `ok()` connect `Rust MCP Tools` to `Rust MCP Tools`, `Rust MCP Tools`, `Rust MCP Tools`, `Rust MCP Tools`, `Rust MCP Tools`, `Rust MCP Tools`, `Rust MCP Tools`, `Rust MCP Tools`?**
  _High betweenness centrality (0.094) - this node is a cross-community bridge._
- **Why does `call_tool()` connect `Rust MCP Tools` to `Rust MCP Tools`, `Rust MCP Tools`, `Rust MCP Tools`, `Rust MCP Tools`, `Rust MCP Tools`, `Rust MCP Tools`, `Rust MCP Tools`, `Rust MCP Tools`?**
  _High betweenness centrality (0.051) - this node is a cross-community bridge._
- **Why does `main()` connect `Rust MCP Tools` to `Rust MCP Tools`, `Rust MCP Tools`?**
  _High betweenness centrality (0.051) - this node is a cross-community bridge._
- **What connects `Request`, `Session`, `GameState` to the rest of the system?**
  _3 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Rust MCP Tools` be split into smaller, more focused modules?**
  _Cohesion score 0.13 - nodes in this community are weakly interconnected._