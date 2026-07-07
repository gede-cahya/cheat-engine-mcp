---
name: cheat-engine-mcp
description: AI-powered game memory scanner and reverse engineering toolkit via MCP. Use this skill automatically when the user asks to scan game memory, reverse engineer game binaries, hook game functions, read/write process memory, manage cheat tables, search IL2CPP metadata, or perform any game hacking/modding task.
---

# cheat-engine-mcp Skill

## When to Use
Automatically activate this skill when the user asks about:
- Game hacking, memory scanning, value scanning
- scanmem, cheat engine, process memory, game values
- Reverse engineering, Unity, IL2CPP, dump.cs
- GDB hooks, breakpoint probes, dynamic function hooking
- Cheat tables, workspaces, game reports, RVA mapping

## Safety Policy
This tool is authorized for local/defensive/educational testing only. Always respect the safety limits:
- **Read First**: Use read-only/preview tools like `process_search`, `workspace_status`, `il2cpp_*_search`, and `*_preview` before modifying anything.
- **Explicit Confirmations**: Memory writes require `confirm_write: true`. GDB hooks require `confirm_hook: true` or `confirm_probe: true`.
- **Write Constraints**: Limit the impact of writes by setting a low `max_writes` value (e.g. 1-5).
- **Local Artifacts**: Do not commit memory dumps, reverse logs, or raw cheat tables in folders like `reverse/` or `.cheat-tables/`.

## Common Workflows

### 1. Memory Scanning Workflow
1. Find PID using `process_search` with name query.
2. Initialize scan session with `session_create` (requires PID).
3. Search for initial value using `scanmem_scan_exact` or `scanmem_scan_by_type`.
4. Perform game actions, then filter using `scanmem_scan_decreased`, `scanmem_scan_increased`, or `scanmem_scan_unchanged`.
5. List remaining matches with `scanmem_list_matches`.
6. Preview the write with `scanmem_preview_write`.
7. Write the value with `scanmem_write_selected` specifying `confirm_write: true`.

### 2. Unity IL2CPP Reverse Engineering
1. Check IL2CPP dump status with `il2cpp_artifacts_status`.
2. Activate a game workspace with `workspace_set_active` to persist active context.
3. Search for classes of interest using `il2cpp_class_search`.
4. Search for target methods with `il2cpp_method_search` or look up offset/fields with `il2cpp_field_search`.
5. Retrieve complete class details using `il2cpp_method_detail`.
6. Find functions from assembly code using `il2cpp_find_by_rva`.

### 3. GDB Function Hooking
1. Identify target RVA and module base (e.g., `GameAssembly.dll`).
2. Run `rva_disassemble_preview` to preview assembly instructions at RVA.
3. Formulate GDB commands (e.g., `set $xmm0.v4_float[0] = 9999.0` to set return value).
4. Run `gdb_hook_preview` to inspect the generated GDB script.
5. Launch hook with `gdb_hook_start` using `confirm_hook: true`.
6. Stop and clean up using `gdb_hook_stop` with target ID.
