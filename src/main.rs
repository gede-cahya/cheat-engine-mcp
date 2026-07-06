#![recursion_limit = "256"]

use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SESSION_TIMEOUT_SECS: u64 = 30 * 60;
const OUT_MAX: usize = 8000;
const SM_DONE: &str = "__SM_DONE__";
const ARTIFACT_ROOT: &str = "reverse";
const ARTIFACT_FILES: [&str; 3] = ["dump.cs", "script.json", "stringliteral.json"];
const ARTIFACT_PREVIEW_MAX: usize = 240;
const SCRIPT_JSON_MAX_BYTES: u64 = 256 * 1024 * 1024;
const MEMORY_READ_MAX_BYTES: usize = 4096;
const REPORT_TITLE_MAX: usize = 160;
const REPORT_FIELD_MAX: usize = 240;
const REPORT_TEXT_MAX: usize = 2000;

#[derive(Deserialize)]
struct Request {
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

struct ScanmemProc {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

// ponytail: Rust's Child drop closes fds but does NOT kill the process, so without
// this scanmem orphans on session close/expire. kill+wait on drop; stdin EOF (field
// drops after this impl) also tells scanmem to exit.
impl Drop for ScanmemProc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Session {
    pid: u64,
    created_at: u64,
    last_seen: u64,
    last_command: String,
    last_output: String,
    last_match_count: usize,
    frozen_value: Option<String>,
    proc: Option<ScanmemProc>,
}

type Sessions = HashMap<u64, Session>;
type Hooks = HashMap<String, GdbHook>;

struct AppState {
    sessions: Sessions,
    hooks: Hooks,
    active_workspace: Option<String>,
}

struct GdbHook {
    pid: u64,
    child: Child,
    script_path: PathBuf,
    log_path: PathBuf,
}

impl Drop for GdbHook {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = Command::new("kill")
            .arg("-CONT")
            .arg(self.pid.to_string())
            .status();
    }
}

fn new_state() -> AppState {
    AppState {
        sessions: Sessions::new(),
        hooks: Hooks::new(),
        active_workspace: None,
    }
}

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut state = new_state();

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<Request>(&line) {
            Ok(req) => handle(req, &mut state),
            Err(err) => json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": { "code": -32700, "message": err.to_string() }
            }),
        };

        writeln!(stdout, "{}", response).ok();
        stdout.flush().ok();
    }
}

fn handle(req: Request, state: &mut AppState) -> Value {
    if req.jsonrpc.as_deref() != Some("2.0") {
        return error(req.id, -32600, "jsonrpc must be 2.0");
    }

    expire_sessions(&mut state.sessions);
    match req.method.as_str() {
        "initialize" => ok(
            req.id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "cheat-engine-mcp", "version": env!("CARGO_PKG_VERSION") }
            }),
        ),
        "tools/list" => ok(req.id, json!({ "tools": tools() })),
        "tools/call" => call_tool(req.id, req.params, state),
        _ => error(req.id, -32601, "method not found"),
    }
}

fn tools() -> Value {
    json!([
        { "name": "ping", "description": "Check that the MCP server is running.", "inputSchema": { "type": "object", "properties": {} } },
        { "name": "scanmem_version", "description": "Return installed scanmem/libscanmem version.", "inputSchema": { "type": "object", "properties": {} } },
        { "name": "list_processes", "description": "List running processes as PID and command name.", "inputSchema": { "type": "object", "properties": { "filter": { "type": "string" } } } },
        { "name": "scanmem_script_preview", "description": "Build a small scanmem command script preview. Does not execute writes.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "value": { "type": "string" } }, "required": ["pid", "value"] } },
        { "name": "scanmem_exact_scan", "description": "Run a read-only exact value scan for a PID using scanmem. This does not write memory.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "value": { "type": "string" } }, "required": ["pid", "value"] } },
        { "name": "scanmem_write_value", "description": "Scan current_value in a PID, then write new_value using scanmem. Requires confirm_write=true.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "current_value": { "type": "string" }, "new_value": { "type": "string" }, "confirm_write": { "type": "boolean" } }, "required": ["pid", "current_value", "new_value", "confirm_write"] } },
        scan_tool("scanmem_attach_process", "Validate that a PID exists and scanmem can select it."),
        scan_tool("scanmem_reset_scan", "Run reset after selecting a PID."),
        scan_tool("scanmem_scan_exact", "Alias for exact value scan."),
        refine_tool("scanmem_scan_increased", "Scan/refine for increased values after an initial exact value."),
        refine_tool("scanmem_scan_decreased", "Scan/refine for decreased values after an initial exact value."),
        refine_tool("scanmem_scan_changed", "Scan/refine for changed values after an initial exact value."),
        refine_tool("scanmem_scan_unchanged", "Scan/refine for unchanged values after an initial exact value."),
        scan_tool("scanmem_list_matches", "Run a scan and list current matches from scanmem output."),
        { "name": "scanmem_pick_match", "description": "Pick a match line by index from scanmem output text.", "inputSchema": { "type": "object", "properties": { "output": { "type": "string" }, "index": { "type": "integer" } }, "required": ["output", "index"] } },
        session_tool("session_create", "Create or refresh one in-memory scan session for a PID."),
        session_tool("session_status", "Show current in-memory session status for a PID, or all sessions if pid is omitted."),
        session_tool("session_close", "Close an in-memory scan session for a PID."),
        { "name": "scanmem_preview_write", "description": "Preview a guarded write before changing memory.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "current_value": { "type": "string" }, "new_value": { "type": "string" }, "max_writes": { "type": "integer" } }, "required": ["pid", "current_value", "new_value"] } },
        { "name": "scanmem_write_selected", "description": "Write a new value after guard checks. Requires confirm_write=true and a live PID.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "current_value": { "type": "string" }, "new_value": { "type": "string" }, "confirm_write": { "type": "boolean" }, "max_writes": { "type": "integer" }, "dry_run": { "type": "boolean" } }, "required": ["pid", "current_value", "new_value", "confirm_write"] } },
        { "name": "scanmem_freeze_value", "description": "Guarded freeze marker: writes value once and stores frozen state in session. Requires confirm_write=true.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "current_value": { "type": "string" }, "freeze_value": { "type": "string" }, "confirm_write": { "type": "boolean" }, "max_writes": { "type": "integer" }, "dry_run": { "type": "boolean" } }, "required": ["pid", "current_value", "freeze_value", "confirm_write"] } },
        { "name": "scanmem_unfreeze_value", "description": "Clear frozen state for a PID session.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" } }, "required": ["pid"] } },
        { "name": "scanmem_scan_by_type", "description": "Scan value with a declared value_type: int32, int64, float, double, string, or hex.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "value": { "type": "string" }, "value_type": { "type": "string" } }, "required": ["pid", "value", "value_type"] } },
        { "name": "scanmem_scan_range", "description": "Scan a numeric range as min..max.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "min": { "type": "string" }, "max": { "type": "string" }, "value_type": { "type": "string" } }, "required": ["pid", "min", "max"] } },
        { "name": "scanmem_scan_unknown", "description": "Start an unknown initial value scan for a PID.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "value_type": { "type": "string" } }, "required": ["pid"] } },
        { "name": "process_search", "description": "Search processes by name/command line and hide system processes by default.", "inputSchema": { "type": "object", "properties": { "query": { "type": "string" }, "include_system": { "type": "boolean" } } } },
        { "name": "process_info", "description": "Show process detail including command line.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" } }, "required": ["pid"] } },
        { "name": "process_suggest_target", "description": "Suggest best target PID for a process name.", "inputSchema": { "type": "object", "properties": { "query": { "type": "string" } }, "required": ["query"] } },
        { "name": "process_list_modules", "description": "List mapped executable/modules from /proc/<pid>/maps with base addresses.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "filter": { "type": "string" } }, "required": ["pid"] } },
        { "name": "process_read_maps", "description": "Read raw /proc/<pid>/maps rows with optional filter.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "filter": { "type": "string" }, "limit": { "type": "integer" } }, "required": ["pid"] } },
        { "name": "process_module_base", "description": "Find the base address for one mapped module name/path.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "module": { "type": "string" } }, "required": ["pid", "module"] } },
        { "name": "rva_to_address", "description": "Convert module RVA to runtime address: module_base + rva.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "module": { "type": "string" }, "rva": { "type": "string" } }, "required": ["pid", "module", "rva"] } },
        { "name": "address_to_rva", "description": "Convert a runtime address to mapped module + RVA.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "address": { "type": "string" } }, "required": ["pid", "address"] } },
        { "name": "gdb_hook_preview", "description": "Build a safe single-breakpoint GDB hook script. Does not attach.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "module": { "type": "string" }, "rva": { "type": "string" }, "commands": { "type": "array", "items": { "type": "string" } }, "name": { "type": "string" } }, "required": ["pid", "module", "rva", "commands"] } },
        { "name": "gdb_hook_start", "description": "Start a safe single-breakpoint GDB hook. Requires confirm_hook=true.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "module": { "type": "string" }, "rva": { "type": "string" }, "commands": { "type": "array", "items": { "type": "string" } }, "name": { "type": "string" }, "confirm_hook": { "type": "boolean" } }, "required": ["pid", "module", "rva", "commands", "confirm_hook"] } },
        { "name": "gdb_hook_stop", "description": "Stop a GDB hook started by this MCP server.", "inputSchema": { "type": "object", "properties": { "hook_id": { "type": "string" } }, "required": ["hook_id"] } },
        { "name": "gdb_hook_group_preview", "description": "Build a multi-breakpoint GDB hook script. Does not attach.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "breakpoints": { "type": "array", "items": { "type": "object", "properties": { "name": { "type": "string" }, "module": { "type": "string" }, "rva": { "type": "string" }, "commands": { "type": "array", "items": { "type": "string" } } }, "required": ["module", "rva", "commands"] } }, "name": { "type": "string" } }, "required": ["pid", "breakpoints"] } },
        { "name": "gdb_hook_group_start", "description": "Start a multi-breakpoint GDB hook. Requires confirm_hook=true.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "breakpoints": { "type": "array", "items": { "type": "object", "properties": { "name": { "type": "string" }, "module": { "type": "string" }, "rva": { "type": "string" }, "commands": { "type": "array", "items": { "type": "string" } } }, "required": ["module", "rva", "commands"] } }, "name": { "type": "string" }, "confirm_hook": { "type": "boolean" } }, "required": ["pid", "breakpoints", "confirm_hook"] } },
        { "name": "gdb_hook_group_stop", "description": "Stop a GDB hook group started by this MCP server.", "inputSchema": { "type": "object", "properties": { "group_id": { "type": "string" }, "hook_id": { "type": "string" } } } },
        { "name": "rva_disassemble_preview", "description": "Preview live GDB disassembly command for module + RVA. Does not attach.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "module": { "type": "string" }, "rva": { "type": "string" }, "count": { "type": "integer" } }, "required": ["pid", "module", "rva"] } },
        { "name": "gdb_disassemble_address", "description": "Attach GDB in batch mode and disassemble instructions at an address.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "address": { "type": "string" }, "count": { "type": "integer" } }, "required": ["pid", "address"] } },
        { "name": "gdb_probe_preview", "description": "Preview a read-only GDB breakpoint probe script. Does not attach.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "module": { "type": "string" }, "rva": { "type": "string" }, "max_hits": { "type": "integer" } }, "required": ["pid", "module", "rva"] } },
        { "name": "gdb_probe_start", "description": "Start a read-only GDB breakpoint probe. Requires confirm_probe=true.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "module": { "type": "string" }, "rva": { "type": "string" }, "max_hits": { "type": "integer" }, "name": { "type": "string" }, "confirm_probe": { "type": "boolean" } }, "required": ["pid", "module", "rva", "confirm_probe"] } },
        { "name": "gdb_probe_stop", "description": "Stop a GDB probe started by this MCP server.", "inputSchema": { "type": "object", "properties": { "probe_id": { "type": "string" }, "hook_id": { "type": "string" } } } },
        { "name": "gdb_breakpoint_probe_preview", "description": "Compatibility alias for gdb_probe_preview.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "module": { "type": "string" }, "rva": { "type": "string" }, "max_hits": { "type": "integer" } }, "required": ["pid", "module", "rva"] } },
        { "name": "memory_read_bytes", "description": "Read bounded raw bytes from a live readable process mapping.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "address": { "type": "string" }, "count": { "type": "integer" } }, "required": ["pid", "address"] } },
        { "name": "memory_read_int", "description": "Read bounded int32/int64 values from a live readable process mapping.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "address": { "type": "string" }, "count": { "type": "integer" }, "value_type": { "type": "string" } }, "required": ["pid", "address"] } },
        { "name": "memory_read_float", "description": "Read bounded float/double values from a live readable process mapping.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "address": { "type": "string" }, "count": { "type": "integer" }, "value_type": { "type": "string" } }, "required": ["pid", "address"] } },
        { "name": "memory_read_string", "description": "Read a bounded NUL-terminated string from a live readable process mapping.", "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "address": { "type": "string" }, "max_bytes": { "type": "integer" } }, "required": ["pid", "address"] } },
        { "name": "workspace_list", "description": "List game workspaces under reverse/ and their IL2CPP artifact status.", "inputSchema": { "type": "object", "properties": {} } },
        { "name": "workspace_status", "description": "Show IL2CPP artifact status for a workspace/game, active workspace, or root.", "inputSchema": { "type": "object", "properties": { "workspace": { "type": "string" }, "game": { "type": "string" }, "root": { "type": "string" } } } },
        { "name": "workspace_set_active", "description": "Set the in-memory active game workspace for IL2CPP tools.", "inputSchema": { "type": "object", "properties": { "workspace": { "type": "string" }, "game": { "type": "string" } } } },
        { "name": "reverse_report_create", "description": "Create a local ignored reverse report JSON and Markdown summary.", "inputSchema": { "type": "object", "properties": { "workspace": { "type": "string" }, "game": { "type": "string" }, "root": { "type": "string" }, "report": { "type": "string" }, "title": { "type": "string" }, "summary": { "type": "string" } }, "required": ["title"] } },
        { "name": "reverse_report_add_finding", "description": "Append a sanitized finding to a local ignored reverse report.", "inputSchema": { "type": "object", "properties": { "workspace": { "type": "string" }, "game": { "type": "string" }, "root": { "type": "string" }, "report": { "type": "string" }, "title": { "type": "string" }, "summary": { "type": "string" }, "severity": { "type": "string" }, "category": { "type": "string" }, "source": { "type": "string" }, "module": { "type": "string" }, "rva": { "type": "string" }, "address": { "type": "string" }, "class": { "type": "string" }, "method": { "type": "string" }, "field": { "type": "string" }, "offset": { "type": "string" }, "notes": { "type": "string" } }, "required": ["report", "title", "summary"] } },
        { "name": "reverse_report_list", "description": "List local ignored reverse reports for one workspace.", "inputSchema": { "type": "object", "properties": { "workspace": { "type": "string" }, "game": { "type": "string" }, "root": { "type": "string" } } } },
        { "name": "il2cpp_artifacts_status", "description": "Show local ignored IL2CPP artifact file status without exposing contents.", "inputSchema": { "type": "object", "properties": { "workspace": { "type": "string" }, "game": { "type": "string" }, "root": { "type": "string" } } } },
        { "name": "il2cpp_method_search", "description": "Search local ignored dump.cs method declarations and RVA metadata.", "inputSchema": { "type": "object", "properties": { "query": { "type": "string" }, "workspace": { "type": "string" }, "game": { "type": "string" }, "root": { "type": "string" }, "limit": { "type": "integer" }, "case_sensitive": { "type": "boolean" } }, "required": ["query"] } },
        { "name": "il2cpp_string_search", "description": "Search local ignored stringliteral.json with clipped previews.", "inputSchema": { "type": "object", "properties": { "query": { "type": "string" }, "workspace": { "type": "string" }, "game": { "type": "string" }, "root": { "type": "string" }, "limit": { "type": "integer" }, "case_sensitive": { "type": "boolean" } }, "required": ["query"] } },
        { "name": "il2cpp_script_search", "description": "Search local ignored script.json selected IL2CPP metadata fields.", "inputSchema": { "type": "object", "properties": { "query": { "type": "string" }, "workspace": { "type": "string" }, "game": { "type": "string" }, "root": { "type": "string" }, "section": { "type": "string" }, "limit": { "type": "integer" }, "case_sensitive": { "type": "boolean" } }, "required": ["query"] } },
        { "name": "il2cpp_class_search", "description": "Search dump.cs class/struct/interface declarations.", "inputSchema": { "type": "object", "properties": { "query": { "type": "string" }, "workspace": { "type": "string" }, "game": { "type": "string" }, "root": { "type": "string" }, "limit": { "type": "integer" }, "case_sensitive": { "type": "boolean" } }, "required": ["query"] } },
        { "name": "il2cpp_field_search", "description": "Search dump.cs field declarations with offsets.", "inputSchema": { "type": "object", "properties": { "query": { "type": "string" }, "workspace": { "type": "string" }, "game": { "type": "string" }, "root": { "type": "string" }, "limit": { "type": "integer" }, "case_sensitive": { "type": "boolean" } }, "required": ["query"] } },
        { "name": "il2cpp_method_detail", "description": "Find method metadata by RVA or signature query.", "inputSchema": { "type": "object", "properties": { "rva": { "type": "string" }, "query": { "type": "string" }, "workspace": { "type": "string" }, "game": { "type": "string" }, "root": { "type": "string" }, "limit": { "type": "integer" }, "case_sensitive": { "type": "boolean" } } } },
        { "name": "il2cpp_find_by_rva", "description": "Find one dump.cs method by exact RVA.", "inputSchema": { "type": "object", "properties": { "rva": { "type": "string" }, "workspace": { "type": "string" }, "game": { "type": "string" }, "root": { "type": "string" } }, "required": ["rva"] } },
        { "name": "il2cpp_related_methods", "description": "List methods related to a class and optional namespace.", "inputSchema": { "type": "object", "properties": { "class": { "type": "string" }, "namespace": { "type": "string" }, "workspace": { "type": "string" }, "game": { "type": "string" }, "root": { "type": "string" }, "limit": { "type": "integer" }, "case_sensitive": { "type": "boolean" } }, "required": ["class"] } },
        { "name": "table_create", "description": "Create a cheat table JSON profile in .cheat-tables.", "inputSchema": { "type": "object", "properties": { "game": { "type": "string" }, "process": { "type": "string" }, "notes": { "type": "string" } }, "required": ["game", "process"] } },
        { "name": "table_add_entry", "description": "Add a named entry to a cheat table.", "inputSchema": { "type": "object", "properties": { "table": { "type": "string" }, "name": { "type": "string" }, "scan": { "type": "string" }, "value_type": { "type": "string" }, "last_value": { "type": "string" }, "notes": { "type": "string" }, "module": { "type": "string" }, "rva": { "type": "string" }, "method_signature": { "type": "string" }, "scan_query": { "type": "string" } }, "required": ["table", "name", "scan", "value_type"] } },
        { "name": "table_resolve_entries", "description": "Resolve table module+RVA entries to runtime addresses for a live PID.", "inputSchema": { "type": "object", "properties": { "table": { "type": "string" }, "pid": { "type": "integer" } }, "required": ["table", "pid"] } },
        { "name": "table_validate_entries", "description": "Validate table entries against a live PID and optionally read value previews.", "inputSchema": { "type": "object", "properties": { "table": { "type": "string" }, "pid": { "type": "integer" }, "read_values": { "type": "boolean" } }, "required": ["table", "pid"] } },
        { "name": "table_load", "description": "Load a cheat table JSON profile.", "inputSchema": { "type": "object", "properties": { "table": { "type": "string" } }, "required": ["table"] } },
        { "name": "table_save", "description": "Save/import a cheat table JSON profile.", "inputSchema": { "type": "object", "properties": { "table": { "type": "string" }, "data": { "type": "object" } }, "required": ["table", "data"] } },
        { "name": "table_list_entries", "description": "List named entries from a cheat table.", "inputSchema": { "type": "object", "properties": { "table": { "type": "string" } }, "required": ["table"] } }
    ])
}

fn scan_tool(name: &str, description: &str) -> Value {
    json!({ "name": name, "description": description, "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "value": { "type": "string" } }, "required": ["pid"] } })
}
fn refine_tool(name: &str, description: &str) -> Value {
    json!({ "name": name, "description": description, "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" }, "initial_value": { "type": "string" } }, "required": ["pid", "initial_value"] } })
}
fn session_tool(name: &str, description: &str) -> Value {
    json!({ "name": name, "description": description, "inputSchema": { "type": "object", "properties": { "pid": { "type": "integer" } } } })
}

fn call_tool(id: Option<Value>, params: Value, state: &mut AppState) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let result = match name {
        "ping" => Ok(tool_ok("pong", json!({}), None)),
        "scanmem_version" => scanmem_version()
            .map(|text| tool_ok("scanmem is available", json!({ "version": text }), None)),
        "list_processes" => list_processes(args.get("filter").and_then(Value::as_str)),
        "scanmem_script_preview" => scanmem_script_preview(&args),
        "scanmem_exact_scan" | "scanmem_scan_exact" => {
            scanmem_exact_scan(&args, &mut state.sessions)
        }
        "scanmem_write_value" => scanmem_write_value(&args, &mut state.sessions),
        "scanmem_attach_process" => scanmem_attach_process(&args, &mut state.sessions),
        "scanmem_reset_scan" => {
            scanmem_simple_command(&args, "reset", "Reset scan completed.", &mut state.sessions)
        }
        "scanmem_scan_increased" => {
            scanmem_refine_scan(&args, "+", "Increased scan completed.", &mut state.sessions)
        }
        "scanmem_scan_decreased" => {
            scanmem_refine_scan(&args, "-", "Decreased scan completed.", &mut state.sessions)
        }
        "scanmem_scan_changed" => scanmem_refine_scan(
            &args,
            "changed",
            "Changed scan completed.",
            &mut state.sessions,
        ),
        "scanmem_scan_unchanged" => scanmem_refine_scan(
            &args,
            "unchanged",
            "Unchanged scan completed.",
            &mut state.sessions,
        ),
        "scanmem_list_matches" => scanmem_simple_command(
            &args,
            "list",
            "List matches completed.",
            &mut state.sessions,
        ),
        "scanmem_pick_match" => scanmem_pick_match(&args),
        "session_create" => session_create(&args, &mut state.sessions),
        "session_status" => session_status(&args, &mut state.sessions),
        "session_close" => session_close(&args, &mut state.sessions),
        "scanmem_preview_write" => scanmem_preview_write(&args, &mut state.sessions),
        "scanmem_write_selected" => scanmem_write_selected(&args, &mut state.sessions),
        "scanmem_freeze_value" => scanmem_freeze_value(&args, &mut state.sessions),
        "scanmem_unfreeze_value" => scanmem_unfreeze_value(&args, &mut state.sessions),
        "scanmem_scan_by_type" => scanmem_scan_by_type(&args, &mut state.sessions),
        "scanmem_scan_range" => scanmem_scan_range(&args, &mut state.sessions),
        "scanmem_scan_unknown" => scanmem_scan_unknown(&args, &mut state.sessions),
        "process_search" => process_search(&args),
        "process_info" => process_info(&args),
        "process_suggest_target" => process_suggest_target(&args),
        "process_list_modules" => process_list_modules(&args),
        "process_read_maps" => process_read_maps(&args),
        "process_module_base" => process_module_base_tool(&args),
        "rva_to_address" => rva_to_address_tool(&args),
        "address_to_rva" => address_to_rva_tool(&args),
        "gdb_hook_preview" => gdb_hook_preview(&args),
        "gdb_hook_start" => gdb_hook_start(&args, &mut state.hooks),
        "gdb_hook_stop" => gdb_hook_stop(&args, &mut state.hooks),
        "gdb_hook_group_preview" => gdb_hook_group_preview(&args),
        "gdb_hook_group_start" => gdb_hook_group_start(&args, &mut state.hooks),
        "gdb_hook_group_stop" => gdb_hook_group_stop(&args, &mut state.hooks),
        "rva_disassemble_preview" => rva_disassemble_preview(&args),
        "gdb_disassemble_address" => gdb_disassemble_address(&args),
        "gdb_probe_preview" | "gdb_breakpoint_probe_preview" => gdb_probe_preview(&args),
        "gdb_probe_start" => gdb_probe_start(&args, &mut state.hooks),
        "gdb_probe_stop" => gdb_probe_stop(&args, &mut state.hooks),
        "memory_read_bytes" => memory_read_bytes(&args),
        "memory_read_int" => memory_read_int(&args),
        "memory_read_float" => memory_read_float(&args),
        "memory_read_string" => memory_read_string(&args),
        "workspace_list" => workspace_list(state.active_workspace.as_deref()),
        "workspace_status" => workspace_status(&args, state.active_workspace.as_deref()),
        "workspace_set_active" => workspace_set_active(&args, state),
        "reverse_report_create" => reverse_report_create(&args, state.active_workspace.as_deref()),
        "reverse_report_add_finding" => {
            reverse_report_add_finding(&args, state.active_workspace.as_deref())
        }
        "reverse_report_list" => reverse_report_list(&args, state.active_workspace.as_deref()),
        "il2cpp_artifacts_status" => {
            il2cpp_artifacts_status(&args, state.active_workspace.as_deref())
        }
        "il2cpp_method_search" => il2cpp_method_search(&args, state.active_workspace.as_deref()),
        "il2cpp_string_search" => il2cpp_string_search(&args, state.active_workspace.as_deref()),
        "il2cpp_script_search" => il2cpp_script_search(&args, state.active_workspace.as_deref()),
        "il2cpp_class_search" => il2cpp_class_search(&args, state.active_workspace.as_deref()),
        "il2cpp_field_search" => il2cpp_field_search(&args, state.active_workspace.as_deref()),
        "il2cpp_method_detail" => il2cpp_method_detail(&args, state.active_workspace.as_deref()),
        "il2cpp_find_by_rva" => il2cpp_find_by_rva(&args, state.active_workspace.as_deref()),
        "il2cpp_related_methods" => {
            il2cpp_related_methods(&args, state.active_workspace.as_deref())
        }
        "table_create" => table_create(&args),
        "table_add_entry" => table_add_entry(&args),
        "table_resolve_entries" => table_resolve_entries(&args),
        "table_validate_entries" => table_validate_entries(&args),
        "table_load" => table_load(&args),
        "table_save" => table_save(&args),
        "table_list_entries" => table_list_entries(&args),
        _ => return error(id, -32602, "unknown tool"),
    };
    match result {
        Ok(value) => ok(id, tool_content(value, false)),
        Err(message) => ok(id, tool_content(tool_err(&message), true)),
    }
}

fn tool_content(value: Value, is_error: bool) -> Value {
    json!({ "isError": is_error, "content": [{ "type": "text", "text": value.to_string() }] })
}
fn tool_ok(message: &str, data: Value, next_suggestion: Option<&str>) -> Value {
    let warnings = output_warning(&data);
    json!({ "ok": true, "message": message, "summary": human_summary(message, &data), "warnings": warnings, "data": data, "next_suggestion": next_suggestion })
}
fn tool_err(message: &str) -> Value {
    json!({ "ok": false, "message": message, "summary": message, "warnings": [], "data": null, "next_suggestion": "Fix the input or verify scanmem/process permissions, then retry." })
}

// ponytail: Windows binary is supported; memory/process backends stay Linux-only
// until native Windows support is requested.
#[cfg(target_os = "linux")]
fn linux_only(_feature: &str) -> Result<(), String> {
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn linux_only(feature: &str) -> Result<(), String> {
    Err(format!(
        "tool unsupported on this platform: {feature} is Linux-only"
    ))
}

fn command_output(program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| format!("failed to run {program}: {err}"))?;
    let text = if out.stdout.is_empty() {
        String::from_utf8_lossy(&out.stderr).to_string()
    } else {
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    if out.status.success() {
        Ok(text.trim().to_string())
    } else {
        Err(text.trim().to_string())
    }
}

fn human_summary(message: &str, data: &Value) -> String {
    if let Some(count) = data.get("match_count").and_then(Value::as_u64) {
        return format!("{message} match_count={count}");
    }
    if let Some(processes) = data.get("processes").and_then(Value::as_array) {
        return format!("{message} processes={}", processes.len());
    }
    if let Some(entries) = data.get("entries").and_then(Value::as_array) {
        return format!("{message} entries={}", entries.len());
    }
    message.to_string()
}

fn output_warning(data: &Value) -> Vec<String> {
    let mut warnings = Vec::new();
    if data
        .get("output")
        .and_then(Value::as_str)
        .is_some_and(|s| s.len() >= 8000)
    {
        warnings.push("output was truncated to 8000 characters".to_string());
    }
    if data.get("match_count").and_then(Value::as_u64).unwrap_or(0) > 100 {
        warnings.push("too many matches; refine scan before writing".to_string());
    }
    warnings
}

fn scanmem_version() -> Result<String, String> {
    linux_only("scanmem backend")?;
    let out = Command::new("scanmem")
        .arg("--version")
        .output()
        .map_err(|err| format!("failed to run scanmem: {err}"))?;
    let text = if out.stdout.is_empty() {
        String::from_utf8_lossy(&out.stderr).to_string()
    } else {
        String::from_utf8_lossy(&out.stdout).to_string()
    }
    .trim()
    .to_string();
    if text.is_empty() {
        Err("scanmem returned empty version output".to_string())
    } else {
        Ok(text)
    }
}

fn list_processes(filter: Option<&str>) -> Result<Value, String> {
    linux_only("process listing")?;
    let text = command_output("ps", &["-eo", "pid=,comm="])?;
    let filter = filter.unwrap_or("").to_lowercase();
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| filter.is_empty() || line.to_lowercase().contains(&filter))
        .take(100)
        .collect();
    Ok(tool_ok(
        &format!("Found {} process entries.", lines.len()),
        json!({ "processes": lines }),
        Some("Use a target PID with session_create or scanmem_exact_scan."),
    ))
}

fn scanmem_script_preview(args: &Value) -> Result<Value, String> {
    let (pid, value) = scan_args(args)?;
    Ok(tool_ok(
        "Generated scanmem script preview.",
        json!({ "script": format!("# preview only (stateful scanmem session)\n# attach: session_create or scanmem_scan_exact/snapshot for pid {pid}\n{value}\n# refine with >  <  !=  =  (changed values), then `set <new>` when sure") }),
        Some("Run scanmem_exact_scan when ready."),
    ))
}

fn scanmem_exact_scan(args: &Value, sessions: &mut Sessions) -> Result<Value, String> {
    linux_only("scanmem backend")?;
    let (pid, value) = scan_args(args)?;
    let sess = ensure_session(sessions, pid)?;
    let output = scanmem_send(proc_of(sess)?, &value)?;
    touch_session_on(sess, "exact", &output);
    Ok(tool_ok(
        "Exact scan completed.",
        json!({ "pid": pid, "value": value, "output": output }),
        Some("Change the value in the target app, then run a refine scan."),
    ))
}

fn proc_of(sess: &mut Session) -> Result<&mut ScanmemProc, String> {
    sess.proc
        .as_mut()
        .ok_or_else(|| "scanmem not attached".to_string())
}

fn scanmem_write_value(args: &Value, sessions: &mut Sessions) -> Result<Value, String> {
    linux_only("scanmem backend")?;
    if args.get("confirm_write").and_then(Value::as_bool) != Some(true) {
        return Err("confirm_write must be true because this changes process memory".to_string());
    }
    let pid = valid_live_pid(args)?;
    let current_value = valid_value_arg(args, "current_value")?;
    let new_value = valid_value_arg(args, "new_value")?;
    let sess = ensure_session(sessions, pid)?;
    let output = scanmem_send(proc_of(sess)?, &format!("{current_value}\nset {new_value}"))?;
    touch_session_on(sess, "write_value", &output);
    Ok(tool_ok(
        "Write command completed.",
        json!({ "pid": pid, "current_value": current_value, "new_value": new_value, "output": output }),
        Some("Verify the target value changed."),
    ))
}

fn scanmem_preview_write(args: &Value, sessions: &mut Sessions) -> Result<Value, String> {
    linux_only("scanmem backend")?;
    let pid = valid_live_pid(args)?;
    let current_value = valid_value_arg(args, "current_value")?;
    let new_value = valid_value_arg(args, "new_value")?;
    let max_writes = args.get("max_writes").and_then(Value::as_u64).unwrap_or(1);
    let sess = ensure_session(sessions, pid)?;
    let output = scanmem_send(proc_of(sess)?, &format!("{current_value}\nlist"))?;
    let match_count = count_matches(&output);
    touch_session_on(sess, "preview_write", &output);
    Ok(tool_ok(
        "Write preview completed.",
        json!({ "pid": pid, "current_value": current_value, "backup_old_value": current_value, "new_value": new_value, "match_count": match_count, "max_writes": max_writes, "allowed": match_count > 0 && match_count as u64 <= max_writes, "dry_run": true, "output": output }),
        Some("If allowed is true, run scanmem_write_selected with confirm_write=true."),
    ))
}

fn scanmem_write_selected(args: &Value, sessions: &mut Sessions) -> Result<Value, String> {
    linux_only("scanmem backend")?;
    let pid = valid_pid(args)?;
    let current_value = valid_value_arg(args, "current_value")?;
    let new_value = valid_value_arg(args, "new_value")?;
    let confirm = args.get("confirm_write").and_then(Value::as_bool) == Some(true);
    let dry_run = args.get("dry_run").and_then(Value::as_bool) == Some(true);
    let max_writes = args.get("max_writes").and_then(Value::as_u64).unwrap_or(1);
    let sess = ensure_session(sessions, pid)?;
    let preview = scanmem_send(proc_of(sess)?, &format!("{current_value}\nlist"))?;
    let match_count = count_matches(&preview);
    if !confirm {
        return Err("confirm_write must be true because this changes process memory".to_string());
    }
    if match_count == 0 {
        return Err("no scan matches found; write blocked".to_string());
    }
    if match_count as u64 > max_writes {
        return Err(format!(
            "{match_count} matches exceed max_writes={max_writes}; write blocked"
        ));
    }
    if dry_run {
        return Ok(tool_ok(
            "Dry run only; no memory changed.",
            json!({ "pid": pid, "match_count": match_count, "backup_old_value": current_value, "preview": preview }),
            Some("Set dry_run=false or omit it to write."),
        ));
    }
    let output = scanmem_send(proc_of(sess)?, &format!("set {new_value}"))?;
    touch_session_on(sess, "write_selected", &output);
    Ok(tool_ok(
        "Selected write completed.",
        json!({ "pid": pid, "current_value": current_value, "backup_old_value": current_value, "new_value": new_value, "match_count": match_count, "output": output }),
        Some("Verify the target value changed."),
    ))
}

fn scanmem_freeze_value(args: &Value, sessions: &mut Sessions) -> Result<Value, String> {
    linux_only("scanmem backend")?;
    let mut write_args = args.clone();
    if let Some(obj) = write_args.as_object_mut() {
        if let Some(v) = obj.remove("freeze_value") {
            obj.insert("new_value".to_string(), v);
        }
    }
    let res = scanmem_write_selected(&write_args, sessions)?;
    let pid = valid_pid(args)?;
    let freeze_value = valid_value_arg(args, "freeze_value")?;
    sessions.entry(pid).or_insert(new_session(pid)).frozen_value = Some(freeze_value.clone());
    Ok(tool_ok(
        "Freeze state recorded after guarded write.",
        json!({ "pid": pid, "frozen_value": freeze_value, "write_result": res, "persistent_loop": false }),
        Some("Call scanmem_unfreeze_value to clear the freeze marker."),
    ))
}

fn scanmem_unfreeze_value(args: &Value, sessions: &mut Sessions) -> Result<Value, String> {
    let pid = valid_pid(args)?;
    let old = sessions.get_mut(&pid).and_then(|s| s.frozen_value.take());
    Ok(tool_ok(
        "Freeze state cleared.",
        json!({ "pid": pid, "was_frozen": old.is_some(), "old_frozen_value": old }),
        None,
    ))
}

fn scanmem_scan_by_type(args: &Value, sessions: &mut Sessions) -> Result<Value, String> {
    linux_only("scanmem backend")?;
    let pid = valid_pid(args)?;
    let value = valid_value_arg(args, "value")?;
    let value_type = valid_value_type(args)?;
    let scan_value = typed_value(&value, &value_type);
    let sess = ensure_session(sessions, pid)?;
    let output = scanmem_send(proc_of(sess)?, &scan_value)?;
    touch_session_on(sess, &format!("scan_by_type:{value_type}"), &output);
    Ok(tool_ok(
        "Typed scan completed.",
        json!({ "pid": pid, "value": value, "value_type": value_type, "scan_value": scan_value, "output": output }),
        Some("Refine or list matches next."),
    ))
}

fn scanmem_scan_range(args: &Value, sessions: &mut Sessions) -> Result<Value, String> {
    linux_only("scanmem backend")?;
    let pid = valid_pid(args)?;
    let min = valid_value_arg(args, "min")?;
    let max = valid_value_arg(args, "max")?;
    let value_type = args
        .get("value_type")
        .and_then(Value::as_str)
        .unwrap_or("auto")
        .to_string();
    if value_type != "auto" {
        valid_value_type(args)?;
    }
    let scan_value = format!("{min}..{max}");
    let sess = ensure_session(sessions, pid)?;
    let output = scanmem_send(proc_of(sess)?, &scan_value)?;
    touch_session_on(sess, &format!("scan_range:{value_type}"), &output);
    Ok(tool_ok(
        "Range scan completed.",
        json!({ "pid": pid, "min": min, "max": max, "value_type": value_type, "scan_value": scan_value, "output": output }),
        Some("Use scanmem_scan_increased/decreased/changed to refine."),
    ))
}

fn scanmem_scan_unknown(args: &Value, sessions: &mut Sessions) -> Result<Value, String> {
    linux_only("scanmem backend")?;
    let pid = valid_pid(args)?;
    let value_type = args
        .get("value_type")
        .and_then(Value::as_str)
        .unwrap_or("auto")
        .to_string();
    if value_type != "auto" {
        valid_value_type(args)?;
    }
    let sess = ensure_session(sessions, pid)?;
    let output = scanmem_send(proc_of(sess)?, "snapshot")?;
    touch_session_on(sess, &format!("scan_unknown:{value_type}"), &output);
    Ok(tool_ok(
        "Unknown initial value scan completed.",
        json!({ "pid": pid, "value_type": value_type, "output": output }),
        Some("Change the target value, then run increased/decreased/changed scan."),
    ))
}

fn process_search(args: &Value) -> Result<Value, String> {
    linux_only("process listing")?;
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();
    let include_system = args
        .get("include_system")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let text = command_output("ps", &["-eo", "pid=,comm=,args="])?;
    let processes: Vec<Value> = text
        .lines()
        .filter_map(parse_process_line)
        .filter(|p| include_system || !is_system_process(p))
        .filter(|p| query.is_empty() || p.to_string().to_lowercase().contains(&query))
        .take(50)
        .collect();
    Ok(tool_ok(
        &format!("Found {} candidate process(es).", processes.len()),
        json!({"processes": processes}),
        Some("Use process_info or process_suggest_target before attaching."),
    ))
}

fn process_info(args: &Value) -> Result<Value, String> {
    linux_only("process info")?;
    let pid = valid_live_pid(args)?;
    let status = fs::read_to_string(format!("/proc/{pid}/status")).unwrap_or_default();
    let comm = fs::read_to_string(format!("/proc/{pid}/comm"))
        .unwrap_or_default()
        .trim()
        .to_string();
    let cmdline = fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    let cmdline = String::from_utf8_lossy(&cmdline)
        .replace('\0', " ")
        .trim()
        .to_string();
    let uid = status.lines().find(|l| l.starts_with("Uid:")).unwrap_or("");
    Ok(tool_ok(
        "Process info loaded.",
        json!({"pid": pid, "name": comm, "cmdline": cmdline, "uid": uid, "likely_game": likely_game(&cmdline)}),
        Some("If this is the target, run session_create."),
    ))
}

fn process_suggest_target(args: &Value) -> Result<Value, String> {
    let mut search = args.clone();
    if let Some(o) = search.as_object_mut() {
        o.insert("include_system".into(), json!(false));
    }
    let res = process_search(&search)?;
    let best = res["data"]["processes"]
        .as_array()
        .and_then(|a| a.iter().max_by_key(|p| score_process(p)).cloned());
    Ok(tool_ok(
        if best.is_some() {
            "Suggested target found."
        } else {
            "No target suggestion found."
        },
        json!({"best": best, "candidates": res["data"]["processes"]}),
        Some("Attach suggested PID only if it matches your intended app/game."),
    ))
}

fn process_list_modules(args: &Value) -> Result<Value, String> {
    linux_only("process modules")?;
    let pid = valid_live_pid(args)?;
    let filter = args
        .get("filter")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();
    let modules: Vec<Value> = read_modules(pid)?
        .into_iter()
        .filter(|m| filter.is_empty() || m.path.to_lowercase().contains(&filter))
        .take(200)
        .map(
            |m| json!({ "base": hex(m.base), "end": hex(m.end), "perms": m.perms, "path": m.path }),
        )
        .collect();
    Ok(tool_ok(
        &format!("Found {} module mapping(s).", modules.len()),
        json!({ "pid": pid, "modules": modules }),
        Some("Use process_module_base or rva_to_address for a specific module."),
    ))
}

fn process_read_maps(args: &Value) -> Result<Value, String> {
    linux_only("process maps")?;
    let pid = valid_live_pid(args)?;
    let filter = args
        .get("filter")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();
    let limit = limit_arg(args, 200, 500);
    let maps = read_maps_entries(pid)?;
    let mut total = 0_usize;
    let mut entries = Vec::new();
    for m in maps {
        if !filter.is_empty() && !m.raw.to_lowercase().contains(&filter) {
            continue;
        }
        total += 1;
        if entries.len() < limit {
            entries.push(map_entry_json(&m));
        }
    }
    Ok(tool_ok(
        &format!("Loaded {total} map row(s)."),
        json!({ "pid": pid, "count": total, "truncated": total > entries.len(), "entries": entries }),
        Some("Use address_to_rva to convert an address, or process_list_modules for named modules only."),
    ))
}

fn process_module_base_tool(args: &Value) -> Result<Value, String> {
    let pid = valid_live_pid(args)?;
    let module = required_str(args, "module")?;
    let m = find_module(pid, module)?;
    Ok(tool_ok(
        "Module base found.",
        json!({ "pid": pid, "module": module, "base": hex(m.base), "end": hex(m.end), "perms": m.perms, "path": m.path }),
        Some("Use rva_to_address with this module and an RVA."),
    ))
}

fn rva_to_address_tool(args: &Value) -> Result<Value, String> {
    let pid = valid_live_pid(args)?;
    let module = required_str(args, "module")?;
    let rva = parse_u64(required_str(args, "rva")?)?;
    let m = find_module(pid, module)?;
    let address = m
        .base
        .checked_add(rva)
        .ok_or_else(|| "module_base + rva overflowed".to_string())?;
    Ok(tool_ok(
        "RVA converted to runtime address.",
        json!({ "pid": pid, "module": module, "module_base": hex(m.base), "rva": hex(rva), "address": hex(address), "path": m.path }),
        Some("Use the address with a debugger breakpoint or memory read/write tool."),
    ))
}

fn address_to_rva_tool(args: &Value) -> Result<Value, String> {
    let pid = valid_live_pid(args)?;
    let address = parse_u64(required_str(args, "address")?)?;
    let maps = read_maps_entries(pid)?;
    let m = maps
        .iter()
        .find(|m| m.start <= address && address < m.end)
        .ok_or_else(|| format!("address {} is not in any mapped region", hex(address)))?;
    let base = if m.path.is_empty() || m.path.starts_with('[') {
        m.start
    } else {
        maps.iter()
            .filter(|x| x.path == m.path)
            .map(|x| x.start)
            .min()
            .unwrap_or(m.start)
    };
    Ok(tool_ok(
        "Address converted to module RVA.",
        json!({ "pid": pid, "address": hex(address), "module_base": hex(base), "mapping_start": hex(m.start), "mapping_end": hex(m.end), "rva": hex(address - base), "perms": m.perms, "path": m.path }),
        Some("Use il2cpp_find_by_rva or rva_disassemble_preview with this RVA."),
    ))
}

fn gdb_hook_preview(args: &Value) -> Result<Value, String> {
    linux_only("GDB hooks")?;
    let spec = gdb_hook_spec(args)?;
    Ok(tool_ok(
        "GDB hook script generated.",
        json!({
            "pid": spec.pid,
            "module": spec.module,
            "module_base": hex(spec.module_base),
            "rva": hex(spec.rva),
            "address": hex(spec.address),
            "script": gdb_hook_script(spec.pid, spec.address, &spec.commands),
        }),
        Some("Review the script, then run gdb_hook_start with confirm_hook=true."),
    ))
}

fn gdb_hook_group_preview(args: &Value) -> Result<Value, String> {
    linux_only("GDB hooks")?;
    let spec = gdb_hook_group_spec(args)?;
    let script = gdb_hook_group_script(spec.pid, &spec.breakpoints);
    Ok(tool_ok(
        "GDB hook group script generated.",
        json!({ "pid": spec.pid, "breakpoint_count": spec.breakpoints.len(), "breakpoints": group_breakpoints_json(&spec.breakpoints), "script": script }),
        Some("Review the script, then run gdb_hook_group_start with confirm_hook=true."),
    ))
}

fn rva_disassemble_preview(args: &Value) -> Result<Value, String> {
    linux_only("GDB disassembly")?;
    let spec = gdb_address_spec(args)?;
    let count = count_arg(args, 8, 40);
    let script = gdb_disassemble_script(spec.pid, spec.address, count);
    Ok(tool_ok(
        "GDB disassembly preview generated.",
        json!({ "pid": spec.pid, "module": spec.module, "module_base": hex(spec.module_base), "rva": hex(spec.rva), "address": hex(spec.address), "count": count, "script": script }),
        Some("Run gdb_disassemble_address with this address when ready."),
    ))
}

fn gdb_disassemble_address(args: &Value) -> Result<Value, String> {
    linux_only("GDB disassembly")?;
    let pid = valid_live_pid(args)?;
    let address = parse_u64(required_str(args, "address")?)?;
    let count = count_arg(args, 8, 40);
    let script = gdb_disassemble_script(pid, address, count);
    let output = command_output(
        "gdb",
        &[
            "-batch",
            "-nx",
            "-p",
            &pid.to_string(),
            "-ex",
            &format!("x/{count}i {}", hex(address)),
            "-ex",
            "detach",
            "-ex",
            "quit",
        ],
    )?;
    Ok(tool_ok(
        "GDB live disassembly completed.",
        json!({ "pid": pid, "address": hex(address), "count": count, "script": script, "output": clip_out(&output) }),
        Some("Verify instructions, then use gdb_hook_preview or gdb_probe_preview."),
    ))
}

fn gdb_probe_preview(args: &Value) -> Result<Value, String> {
    linux_only("GDB probes")?;
    let spec = gdb_address_spec(args)?;
    let max_hits = max_hits_arg(args);
    let script = gdb_probe_script(spec.pid, spec.address, max_hits);
    Ok(tool_ok(
        "GDB probe preview generated.",
        json!({ "pid": spec.pid, "module": spec.module, "module_base": hex(spec.module_base), "rva": hex(spec.rva), "address": hex(spec.address), "max_hits": max_hits, "script": script }),
        Some("Review the script, then run gdb_probe_start with confirm_probe=true."),
    ))
}

fn memory_read_bytes(args: &Value) -> Result<Value, String> {
    let pid = valid_live_pid(args)?;
    let address = parse_u64(required_str(args, "address")?)?;
    let count = memory_read_count(args, 64);
    let (bytes, mapping) = read_process_memory(pid, address, count)?;
    Ok(tool_ok(
        "Memory bytes read.",
        json!({ "pid": pid, "address": hex(address), "count": count, "bytes_read": bytes.len(), "hex": bytes_hex(&bytes), "ascii": bytes_ascii(&bytes), "mapping": mapping }),
        Some("Use memory_read_int/float/string if you know the value type."),
    ))
}

fn memory_read_int(args: &Value) -> Result<Value, String> {
    let pid = valid_live_pid(args)?;
    let address = parse_u64(required_str(args, "address")?)?;
    let value_type = args
        .get("value_type")
        .and_then(Value::as_str)
        .unwrap_or("int32");
    let unit = match value_type {
        "int32" => 4,
        "int64" => 8,
        _ => return Err("value_type must be int32 or int64".into()),
    };
    let count = count_arg(args, 1, MEMORY_READ_MAX_BYTES / unit);
    let (bytes, mapping) = read_process_memory(pid, address, count * unit)?;
    let mut values = Vec::new();
    let mut hex_values = Vec::new();
    for chunk in bytes.chunks_exact(unit) {
        if unit == 4 {
            let raw = u32::from_ne_bytes(chunk.try_into().unwrap());
            values.push(json!(raw as i32));
            hex_values.push(format!("0x{raw:x}"));
        } else {
            let raw = u64::from_ne_bytes(chunk.try_into().unwrap());
            values.push(json!(raw as i64));
            hex_values.push(format!("0x{raw:x}"));
        }
    }
    Ok(tool_ok(
        "Memory integer value(s) read.",
        json!({ "pid": pid, "address": hex(address), "count": count, "value_type": value_type, "values": values, "hex_values": hex_values, "mapping": mapping }),
        None,
    ))
}

fn memory_read_float(args: &Value) -> Result<Value, String> {
    let pid = valid_live_pid(args)?;
    let address = parse_u64(required_str(args, "address")?)?;
    let value_type = args
        .get("value_type")
        .and_then(Value::as_str)
        .unwrap_or("float");
    let unit = match value_type {
        "float" => 4,
        "double" => 8,
        _ => return Err("value_type must be float or double".into()),
    };
    let count = count_arg(args, 1, MEMORY_READ_MAX_BYTES / unit);
    let (bytes, mapping) = read_process_memory(pid, address, count * unit)?;
    let mut values = Vec::new();
    for chunk in bytes.chunks_exact(unit) {
        if unit == 4 {
            values.push(json!(f32::from_ne_bytes(chunk.try_into().unwrap())));
        } else {
            values.push(json!(f64::from_ne_bytes(chunk.try_into().unwrap())));
        }
    }
    Ok(tool_ok(
        "Memory float value(s) read.",
        json!({ "pid": pid, "address": hex(address), "count": count, "value_type": value_type, "values": values, "mapping": mapping }),
        None,
    ))
}

fn memory_read_string(args: &Value) -> Result<Value, String> {
    let pid = valid_live_pid(args)?;
    let address = parse_u64(required_str(args, "address")?)?;
    let max_bytes = memory_read_count(args, 256);
    let (bytes, mapping) = read_process_memory(pid, address, max_bytes)?;
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    let string = String::from_utf8_lossy(&bytes[..end]).to_string();
    Ok(tool_ok(
        "Memory string read.",
        json!({ "pid": pid, "address": hex(address), "max_bytes": max_bytes, "length": string.len(), "truncated": end == bytes.len(), "string": string, "hex": bytes_hex(&bytes[..end]), "mapping": mapping }),
        None,
    ))
}

fn memory_read_count(args: &Value, default: usize) -> usize {
    args.get("count")
        .or_else(|| args.get("max_bytes"))
        .and_then(Value::as_u64)
        .map(|n| (n as usize).clamp(1, MEMORY_READ_MAX_BYTES))
        .unwrap_or(default)
}

fn read_process_memory(pid: u64, address: u64, len: usize) -> Result<(Vec<u8>, Value), String> {
    linux_only("process memory reads")?;
    if len == 0 || len > MEMORY_READ_MAX_BYTES {
        return Err(format!("read length must be 1..{MEMORY_READ_MAX_BYTES}"));
    }
    let end = address
        .checked_add(len as u64)
        .ok_or_else(|| "address + length overflowed".to_string())?;
    let maps = read_maps_entries(pid)?;
    let m = maps
        .iter()
        .find(|m| m.start <= address && address < m.end)
        .ok_or_else(|| format!("address {} is not in any mapped region", hex(address)))?;
    if !m.perms.starts_with('r') {
        return Err(format!(
            "address {} is not in a readable mapping",
            hex(address)
        ));
    }
    if end > m.end {
        return Err(format!(
            "read crosses mapping end: {} > {}",
            hex(end),
            hex(m.end)
        ));
    }
    let mut file = std::fs::File::open(format!("/proc/{pid}/mem")).map_err(|e| e.to_string())?;
    let mut bytes = vec![0; len];
    file.seek(SeekFrom::Start(address))
        .map_err(|e| format!("memory seek failed: {e}"))?;
    let n = file
        .read(&mut bytes)
        .map_err(|e| format!("memory read failed: {e}"))?;
    bytes.truncate(n);
    Ok((bytes, map_entry_json(m)))
}

fn bytes_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn bytes_ascii(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| match b {
            0x20..=0x7e => *b as char,
            _ => '.',
        })
        .collect()
}

fn gdb_hook_start(args: &Value, hooks: &mut Hooks) -> Result<Value, String> {
    linux_only("GDB hooks")?;
    if args.get("confirm_hook").and_then(Value::as_bool) != Some(true) {
        return Err("confirm_hook must be true because this attaches GDB to a live process".into());
    }
    let spec = gdb_hook_spec(args)?;
    let hook_id = hook_id(args, spec.pid, "gdb-hook");
    if hooks.contains_key(&hook_id) {
        return Err("hook_id collision; retry with a different name".into());
    }
    let script_path = std::env::temp_dir().join(format!("cheat-engine-mcp-{hook_id}.gdb"));
    let log_path = std::env::temp_dir().join(format!("cheat-engine-mcp-{hook_id}.log"));
    fs::write(
        &script_path,
        gdb_hook_script(spec.pid, spec.address, &spec.commands),
    )
    .map_err(|e| e.to_string())?;
    let log = std::fs::File::create(&log_path).map_err(|e| e.to_string())?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    let child = Command::new("gdb")
        .arg("-q")
        .arg("-nx")
        .arg("-x")
        .arg(&script_path)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err))
        .spawn()
        .map_err(|e| format!("failed to start gdb: {e}"))?;
    let gdb_pid = child.id();
    hooks.insert(
        hook_id.clone(),
        GdbHook {
            pid: spec.pid,
            child,
            script_path: script_path.clone(),
            log_path: log_path.clone(),
        },
    );
    Ok(tool_ok(
        "GDB hook started.",
        json!({
            "hook_id": hook_id,
            "pid": spec.pid,
            "gdb_pid": gdb_pid,
            "module": spec.module,
            "module_base": hex(spec.module_base),
            "rva": hex(spec.rva),
            "address": hex(spec.address),
            "script_path": script_path.to_string_lossy(),
            "log_path": log_path.to_string_lossy(),
        }),
        Some("Run gdb_hook_stop with hook_id when done."),
    ))
}

fn gdb_hook_stop(args: &Value, hooks: &mut Hooks) -> Result<Value, String> {
    let hook_id = required_str(args, "hook_id")?;
    stop_gdb_child(
        hook_id,
        hooks,
        "hook_id",
        "GDB hook stopped.",
        "No hook existed for hook_id.",
    )
}

fn gdb_hook_group_start(args: &Value, hooks: &mut Hooks) -> Result<Value, String> {
    linux_only("GDB hooks")?;
    if args.get("confirm_hook").and_then(Value::as_bool) != Some(true) {
        return Err("confirm_hook must be true because this attaches GDB to a live process".into());
    }
    let spec = gdb_hook_group_spec(args)?;
    let group_id = hook_id(args, spec.pid, "gdb-hook-group");
    if hooks.contains_key(&group_id) {
        return Err("group_id collision; retry with a different name".into());
    }
    let script_path = std::env::temp_dir().join(format!("cheat-engine-mcp-{group_id}.gdb"));
    let log_path = std::env::temp_dir().join(format!("cheat-engine-mcp-{group_id}.log"));
    fs::write(
        &script_path,
        gdb_hook_group_script(spec.pid, &spec.breakpoints),
    )
    .map_err(|e| e.to_string())?;
    let log = std::fs::File::create(&log_path).map_err(|e| e.to_string())?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    let child = Command::new("gdb")
        .arg("-q")
        .arg("-nx")
        .arg("-x")
        .arg(&script_path)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err))
        .spawn()
        .map_err(|e| format!("failed to start gdb: {e}"))?;
    let gdb_pid = child.id();
    hooks.insert(
        group_id.clone(),
        GdbHook {
            pid: spec.pid,
            child,
            script_path: script_path.clone(),
            log_path: log_path.clone(),
        },
    );
    Ok(tool_ok(
        "GDB hook group started.",
        json!({
            "group_id": group_id,
            "hook_id": group_id,
            "pid": spec.pid,
            "gdb_pid": gdb_pid,
            "breakpoint_count": spec.breakpoints.len(),
            "breakpoints": group_breakpoints_json(&spec.breakpoints),
            "script_path": script_path.to_string_lossy(),
            "log_path": log_path.to_string_lossy(),
        }),
        Some("Run gdb_hook_group_stop with group_id when done."),
    ))
}

fn gdb_hook_group_stop(args: &Value, hooks: &mut Hooks) -> Result<Value, String> {
    let group_id = args
        .get("group_id")
        .or_else(|| args.get("hook_id"))
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "group_id is required".to_string())?;
    stop_gdb_child(
        group_id,
        hooks,
        "group_id",
        "GDB hook group stopped.",
        "No hook group existed for group_id.",
    )
}

fn gdb_probe_start(args: &Value, hooks: &mut Hooks) -> Result<Value, String> {
    linux_only("GDB probes")?;
    if args.get("confirm_probe").and_then(Value::as_bool) != Some(true) {
        return Err(
            "confirm_probe must be true because this attaches GDB to a live process".into(),
        );
    }
    let spec = gdb_address_spec(args)?;
    let max_hits = max_hits_arg(args);
    let probe_id = hook_id(args, spec.pid, "gdb-probe");
    if hooks.contains_key(&probe_id) {
        return Err("probe_id collision; retry with a different name".into());
    }
    let script_path = std::env::temp_dir().join(format!("cheat-engine-mcp-{probe_id}.gdb"));
    let log_path = std::env::temp_dir().join(format!("cheat-engine-mcp-{probe_id}.log"));
    fs::write(
        &script_path,
        gdb_probe_script(spec.pid, spec.address, max_hits),
    )
    .map_err(|e| e.to_string())?;
    let log = std::fs::File::create(&log_path).map_err(|e| e.to_string())?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    let child = Command::new("gdb")
        .arg("-q")
        .arg("-nx")
        .arg("-x")
        .arg(&script_path)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err))
        .spawn()
        .map_err(|e| format!("failed to start gdb: {e}"))?;
    let gdb_pid = child.id();
    hooks.insert(
        probe_id.clone(),
        GdbHook {
            pid: spec.pid,
            child,
            script_path: script_path.clone(),
            log_path: log_path.clone(),
        },
    );
    Ok(tool_ok(
        "GDB probe started.",
        json!({
            "probe_id": probe_id,
            "hook_id": probe_id,
            "pid": spec.pid,
            "gdb_pid": gdb_pid,
            "module": spec.module,
            "module_base": hex(spec.module_base),
            "rva": hex(spec.rva),
            "address": hex(spec.address),
            "max_hits": max_hits,
            "script_path": script_path.to_string_lossy(),
            "log_path": log_path.to_string_lossy(),
        }),
        Some("Inspect log_path or run gdb_probe_stop with probe_id."),
    ))
}

fn gdb_probe_stop(args: &Value, hooks: &mut Hooks) -> Result<Value, String> {
    let probe_id = args
        .get("probe_id")
        .or_else(|| args.get("hook_id"))
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "probe_id is required".to_string())?;
    stop_gdb_child(
        probe_id,
        hooks,
        "probe_id",
        "GDB probe stopped.",
        "No probe existed for probe_id.",
    )
}

fn stop_gdb_child(
    id: &str,
    hooks: &mut Hooks,
    id_field: &str,
    stopped_message: &str,
    missing_message: &str,
) -> Result<Value, String> {
    if let Some(hook) = hooks.remove(id) {
        let pid = hook.pid;
        let script_path = hook.script_path.to_string_lossy().to_string();
        let log_path = hook.log_path.to_string_lossy().to_string();
        drop(hook);
        return Ok(tool_ok(
            stopped_message,
            json!({ id_field: id, "pid": pid, "stopped": true, "script_path": script_path, "log_path": log_path }),
            None,
        ));
    }
    Ok(tool_ok(
        missing_message,
        json!({ id_field: id, "stopped": false }),
        None,
    ))
}

struct GdbHookSpec {
    pid: u64,
    module: String,
    module_base: u64,
    rva: u64,
    address: u64,
    commands: Vec<String>,
}

struct GdbHookGroupSpec {
    pid: u64,
    breakpoints: Vec<GdbGroupBreakpoint>,
}

struct GdbGroupBreakpoint {
    name: String,
    module: String,
    module_base: u64,
    rva: u64,
    address: u64,
    commands: Vec<String>,
}

struct GdbAddressSpec {
    pid: u64,
    module: String,
    module_base: u64,
    rva: u64,
    address: u64,
}

fn gdb_address_spec(args: &Value) -> Result<GdbAddressSpec, String> {
    let pid = valid_live_pid(args)?;
    let module = required_str(args, "module")?.to_string();
    let rva = parse_u64(required_str(args, "rva")?)?;
    let m = find_module(pid, &module)?;
    let address = m
        .base
        .checked_add(rva)
        .ok_or_else(|| "module_base + rva overflowed".to_string())?;
    Ok(GdbAddressSpec {
        pid,
        module,
        module_base: m.base,
        rva,
        address,
    })
}

fn gdb_hook_spec(args: &Value) -> Result<GdbHookSpec, String> {
    let pid = valid_live_pid(args)?;
    let module = required_str(args, "module")?.to_string();
    let rva = parse_u64(required_str(args, "rva")?)?;
    let m = find_module(pid, &module)?;
    let address = m
        .base
        .checked_add(rva)
        .ok_or_else(|| "module_base + rva overflowed".to_string())?;
    Ok(GdbHookSpec {
        pid,
        module,
        module_base: m.base,
        rva,
        address,
        commands: valid_gdb_commands(args)?,
    })
}

fn gdb_hook_group_spec(args: &Value) -> Result<GdbHookGroupSpec, String> {
    let pid = valid_live_pid(args)?;
    let arr = args
        .get("breakpoints")
        .and_then(Value::as_array)
        .ok_or_else(|| "breakpoints must be an array".to_string())?;
    if arr.is_empty() || arr.len() > 16 {
        return Err("breakpoints must contain 1..16 items".into());
    }
    let mut breakpoints = Vec::new();
    for (idx, bp) in arr.iter().enumerate() {
        let module = required_str(bp, "module")?.to_string();
        let rva = parse_u64(required_str(bp, "rva")?)?;
        let m = find_module(pid, &module)?;
        let address = m
            .base
            .checked_add(rva)
            .ok_or_else(|| "module_base + rva overflowed".to_string())?;
        breakpoints.push(GdbGroupBreakpoint {
            name: bp
                .get("name")
                .and_then(Value::as_str)
                .map(safe_label)
                .unwrap_or_else(|| format!("bp{}", idx + 1)),
            module,
            module_base: m.base,
            rva,
            address,
            commands: valid_gdb_commands(bp)?,
        });
    }
    Ok(GdbHookGroupSpec { pid, breakpoints })
}

fn group_breakpoints_json(breakpoints: &[GdbGroupBreakpoint]) -> Value {
    Value::Array(
        breakpoints
            .iter()
            .map(|bp| {
                json!({ "name": bp.name, "module": bp.module, "module_base": hex(bp.module_base), "rva": hex(bp.rva), "address": hex(bp.address) })
            })
            .collect(),
    )
}

fn valid_gdb_commands(args: &Value) -> Result<Vec<String>, String> {
    let arr = args
        .get("commands")
        .and_then(Value::as_array)
        .ok_or_else(|| "commands must be an array of GDB lines".to_string())?;
    if arr.is_empty() || arr.len() > 40 {
        return Err("commands must contain 1..40 lines".into());
    }
    let mut balance = 0_i32;
    let mut commands = Vec::new();
    for item in arr {
        let line = item
            .as_str()
            .ok_or_else(|| "each command must be a string".to_string())?
            .trim();
        if line.is_empty() {
            continue;
        }
        if line.contains('\n') || line.contains('\r') {
            return Err("commands must be one GDB line each".into());
        }
        let first = line
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        match first.as_str() {
            "if" => balance += 1,
            "else" if balance > 0 => {}
            "end" if balance > 0 => balance -= 1,
            "set" | "printf" => {}
            _ => return Err(format!("unsupported gdb command: {first}")),
        }
        commands.push(line.to_string());
    }
    if commands.is_empty() || balance != 0 {
        return Err("commands must be non-empty and have balanced if/end".into());
    }
    Ok(commands)
}

fn gdb_hook_script(pid: u64, address: u64, commands: &[String]) -> String {
    let mut script = gdb_prologue(pid);
    push_gdb_breakpoint(&mut script, address, commands);
    script.push_str("continue\n");
    script
}

fn gdb_hook_group_script(pid: u64, breakpoints: &[GdbGroupBreakpoint]) -> String {
    let mut script = gdb_prologue(pid);
    for bp in breakpoints {
        script.push_str("# ");
        script.push_str(&bp.name);
        script.push('\n');
        push_gdb_breakpoint(&mut script, bp.address, &bp.commands);
    }
    script.push_str("continue\n");
    script
}

fn gdb_prologue(pid: u64) -> String {
    format!(
        "set pagination off\nset confirm off\nset debuginfod enabled off\nset print thread-events off\nhandle SIGUSR1 nostop noprint pass\nhandle SIGUSR2 nostop noprint pass\nhandle SIGPIPE nostop noprint pass\nattach {pid}\n"
    )
}

fn push_gdb_breakpoint(script: &mut String, address: u64, commands: &[String]) {
    script.push_str(&format!("break *{}\ncommands\n  silent\n", hex(address)));
    for command in commands {
        script.push_str("  ");
        script.push_str(command);
        script.push('\n');
    }
    script.push_str("  continue\nend\n");
}

fn gdb_disassemble_script(pid: u64, address: u64, count: usize) -> String {
    format!(
        "set pagination off\nset confirm off\nset debuginfod enabled off\nattach {pid}\nx/{count}i {}\ndetach\nquit\n",
        hex(address)
    )
}

fn gdb_probe_script(pid: u64, address: u64, max_hits: usize) -> String {
    format!(
        "set pagination off\nset confirm off\nset debuginfod enabled off\nset print thread-events off\nhandle SIGUSR1 nostop noprint pass\nhandle SIGUSR2 nostop noprint pass\nhandle SIGPIPE nostop noprint pass\nset $cemcp_probe_hits = 0\nattach {pid}\nbreak *{}\ncommands\n  silent\n  set $cemcp_probe_hits = $cemcp_probe_hits + 1\n  printf \"probe_hit hit=%d pc=%p\\n\", $cemcp_probe_hits, $pc\n  info registers rax rbx rcx rdx rsi rdi rbp rsp\n  x/4gx $rsp\n  if $cemcp_probe_hits >= {max_hits}\n    printf \"probe_done hits=%d\\n\", $cemcp_probe_hits\n    detach\n    quit\n  end\n  continue\nend\ncontinue\n",
        hex(address)
    )
}

fn max_hits_arg(args: &Value) -> usize {
    args.get("max_hits")
        .and_then(Value::as_u64)
        .map(|n| (n as usize).clamp(1, 100))
        .unwrap_or(5)
}

fn hook_id(args: &Value, pid: u64, default: &str) -> String {
    let raw = args.get("name").and_then(Value::as_str).unwrap_or(default);
    let label = safe_label(raw);
    format!("{label}-{pid}-{}", now_secs())
}

fn safe_label(raw: &str) -> String {
    let label: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .take(48)
        .collect();
    let label = label.trim_matches('-');
    if label.is_empty() {
        "gdb-hook".to_string()
    } else {
        label.to_string()
    }
}

fn workspace_list(active_workspace: Option<&str>) -> Result<Value, String> {
    let mut workspaces = Vec::new();
    if let Ok(entries) = fs::read_dir(ARTIFACT_ROOT) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let Ok(name) = checked_workspace_name(name) else {
                continue;
            };
            let root = detect_artifact_root(workspace_candidates(&name)?)?;
            let status = artifact_status(&root)?;
            workspaces.push(json!({
                "workspace": name,
                "game": name,
                "root": root.to_string_lossy(),
                "ready": status["ready"],
                "files": status["files"],
            }));
        }
    }
    workspaces.sort_by_key(|w| w["workspace"].as_str().unwrap_or("").to_string());
    Ok(tool_ok(
        &format!("Found {} workspace(s).", workspaces.len()),
        json!({ "active_workspace": active_workspace, "workspaces": workspaces }),
        Some("Use workspace_set_active or pass game/workspace to IL2CPP tools."),
    ))
}

fn workspace_status(args: &Value, active_workspace: Option<&str>) -> Result<Value, String> {
    let root = artifact_root(args, active_workspace)?;
    let status = artifact_status(&root)?;
    Ok(tool_ok(
        "Workspace status loaded.",
        json!({
            "active_workspace": active_workspace,
            "workspace": workspace_arg(args),
            "game": args.get("game").and_then(Value::as_str),
            "root": root.to_string_lossy(),
            "ready": status["ready"],
            "files": status["files"],
        }),
        Some("Use workspace_set_active or run an IL2CPP search."),
    ))
}

fn workspace_set_active(args: &Value, state: &mut AppState) -> Result<Value, String> {
    let workspace = workspace_arg(args)
        .ok_or_else(|| "workspace or game is required".to_string())
        .and_then(checked_workspace_name)?;
    let root = detect_artifact_root(workspace_candidates(&workspace)?)?;
    if !root.exists() && !Path::new(ARTIFACT_ROOT).join(&workspace).exists() {
        return Err(format!("workspace not found: {workspace}"));
    }
    state.active_workspace = Some(workspace.clone());
    let status = artifact_status(&root)?;
    Ok(tool_ok(
        "Active workspace set.",
        json!({
            "active_workspace": workspace,
            "root": root.to_string_lossy(),
            "ready": status["ready"],
            "files": status["files"],
        }),
        Some("IL2CPP tools will use this workspace when no root/game/workspace is passed."),
    ))
}

fn il2cpp_artifacts_status(args: &Value, active_workspace: Option<&str>) -> Result<Value, String> {
    let root = artifact_root(args, active_workspace)?;
    let status = artifact_status(&root)?;
    Ok(tool_ok(
        "IL2CPP artifact status loaded.",
        json!({
            "root": root.to_string_lossy(),
            "ready": status["ready"],
            "files": status["files"],
        }),
        Some("Use il2cpp_method_search or il2cpp_string_search next."),
    ))
}

fn il2cpp_method_search(args: &Value, active_workspace: Option<&str>) -> Result<Value, String> {
    let root = artifact_root(args, active_workspace)?;
    let path = artifact_file(&root, "dump.cs")?;
    let query = required_str(args, "query")?;
    let limit = limit_arg(args, 20, 50);
    let case_sensitive = args
        .get("case_sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let mut namespace = String::new();
    let mut type_name = String::new();
    let mut pending = MethodMeta::default();
    let mut matches = Vec::new();
    let mut total = 0_usize;
    for (idx, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|e| e.to_string())?;
        let trimmed = line.trim();
        if let Some(ns) = trimmed.strip_prefix("// Namespace:") {
            namespace = ns.trim().to_string();
            continue;
        }
        if let Some(name) = parse_type_name(trimmed) {
            type_name = name;
            continue;
        }
        if trimmed.starts_with("// RVA:") {
            pending = parse_method_meta(trimmed);
            continue;
        }
        if pending.rva.is_some() && looks_like_method_decl(trimmed) {
            if matches_query(trimmed, query, case_sensitive)
                || matches_query(&type_name, query, case_sensitive)
                || matches_query(&namespace, query, case_sensitive)
            {
                total += 1;
                if matches.len() < limit {
                    matches.push(method_match_json(
                        &namespace,
                        &type_name,
                        trimmed,
                        &pending,
                        idx + 1,
                    ));
                }
            }
            pending = MethodMeta::default();
        }
    }
    Ok(tool_ok(
        &format!("Found {total} IL2CPP method match(es)."),
        json!({ "source": "dump.cs", "query": query, "count": total, "truncated": total > matches.len(), "matches": matches }),
        Some("Use rva_to_address then gdb_hook_preview for a selected method."),
    ))
}

fn il2cpp_string_search(args: &Value, active_workspace: Option<&str>) -> Result<Value, String> {
    let root = artifact_root(args, active_workspace)?;
    let path = artifact_file(&root, "stringliteral.json")?;
    let query = required_str(args, "query")?;
    let limit = limit_arg(args, 20, 50);
    let case_sensitive = args
        .get("case_sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let data: Value =
        serde_json::from_reader(std::fs::File::open(&path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut total = 0_usize;
    let mut matches = Vec::new();
    if let Some(arr) = data.as_array() {
        for (idx, item) in arr.iter().enumerate() {
            let value = item.get("value").and_then(Value::as_str).unwrap_or("");
            if matches_query(value, query, case_sensitive) {
                total += 1;
                if matches.len() < limit {
                    matches.push(json!({
                        "index": idx,
                        "address": item.get("address").cloned().unwrap_or(Value::Null),
                        "value_preview": clip(value, ARTIFACT_PREVIEW_MAX),
                    }));
                }
            }
        }
    }
    Ok(tool_ok(
        &format!("Found {total} IL2CPP string match(es)."),
        json!({ "source": "stringliteral.json", "query": query, "count": total, "truncated": total > matches.len(), "matches": matches }),
        None,
    ))
}

fn il2cpp_script_search(args: &Value, active_workspace: Option<&str>) -> Result<Value, String> {
    let root = artifact_root(args, active_workspace)?;
    let path = artifact_file(&root, "script.json")?;
    let size = path.metadata().map_err(|e| e.to_string())?.len();
    if size > SCRIPT_JSON_MAX_BYTES {
        return Err(format!("script.json is too large: {size} bytes"));
    }
    let query = required_str(args, "query")?;
    let section = args
        .get("section")
        .and_then(Value::as_str)
        .unwrap_or("methods");
    let limit = limit_arg(args, 20, 50);
    let case_sensitive = args
        .get("case_sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let data: Value =
        serde_json::from_reader(std::fs::File::open(&path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut matches = Vec::new();
    let mut total = 0_usize;
    for (label, key) in script_sections(section)? {
        if let Some(arr) = data.get(key).and_then(Value::as_array) {
            for (idx, item) in arr.iter().enumerate() {
                let haystack = ["Name", "Signature", "value", "Value", "Address"]
                    .iter()
                    .filter_map(|k| item.get(*k).and_then(value_as_text))
                    .collect::<Vec<_>>()
                    .join(" ");
                if matches_query(&haystack, query, case_sensitive) {
                    total += 1;
                    if matches.len() < limit {
                        matches.push(script_match(label, idx, item));
                    }
                }
            }
        }
    }
    Ok(tool_ok(
        &format!("Found {total} script metadata match(es)."),
        json!({ "source": "script.json", "section": section, "query": query, "count": total, "truncated": total > matches.len(), "matches": matches }),
        None,
    ))
}

fn il2cpp_class_search(args: &Value, active_workspace: Option<&str>) -> Result<Value, String> {
    let root = artifact_root(args, active_workspace)?;
    let path = artifact_file(&root, "dump.cs")?;
    let query = required_str(args, "query")?;
    let limit = limit_arg(args, 20, 50);
    let case_sensitive = args
        .get("case_sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let mut namespace = String::new();
    let mut matches = Vec::new();
    let mut total = 0_usize;
    for (idx, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|e| e.to_string())?;
        let trimmed = line.trim();
        if let Some(ns) = trimmed.strip_prefix("// Namespace:") {
            namespace = ns.trim().to_string();
            continue;
        }
        let Some(decl) = parse_type_decl(trimmed) else {
            continue;
        };
        if matches_query(&decl.name, query, case_sensitive)
            || matches_query(&decl.declaration, query, case_sensitive)
            || matches_query(&namespace, query, case_sensitive)
        {
            total += 1;
            if matches.len() < limit {
                matches.push(json!({
                    "namespace": namespace,
                    "class": decl.name,
                    "kind": decl.kind,
                    "base": decl.base,
                    "type_def_index": decl.type_def_index,
                    "signature": decl.declaration,
                    "rva": Value::Null,
                    "line": idx + 1,
                }));
            }
        }
    }
    Ok(tool_ok(
        &format!("Found {total} IL2CPP class match(es)."),
        json!({ "source": "dump.cs", "query": query, "count": total, "truncated": total > matches.len(), "matches": matches }),
        Some("Use il2cpp_related_methods or il2cpp_field_search on a found class."),
    ))
}

fn il2cpp_field_search(args: &Value, active_workspace: Option<&str>) -> Result<Value, String> {
    let root = artifact_root(args, active_workspace)?;
    let path = artifact_file(&root, "dump.cs")?;
    let query = required_str(args, "query")?;
    let limit = limit_arg(args, 20, 50);
    let case_sensitive = args
        .get("case_sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let mut namespace = String::new();
    let mut type_name = String::new();
    let mut matches = Vec::new();
    let mut total = 0_usize;
    for (idx, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|e| e.to_string())?;
        let trimmed = line.trim();
        if let Some(ns) = trimmed.strip_prefix("// Namespace:") {
            namespace = ns.trim().to_string();
            continue;
        }
        if let Some(decl) = parse_type_decl(trimmed) {
            type_name = decl.name;
            continue;
        }
        let Some(field) = parse_field_decl(trimmed) else {
            continue;
        };
        if matches_query(&field.field_name, query, case_sensitive)
            || matches_query(&field.field_type, query, case_sensitive)
            || matches_query(&type_name, query, case_sensitive)
            || matches_query(&namespace, query, case_sensitive)
        {
            total += 1;
            if matches.len() < limit {
                matches.push(json!({
                    "namespace": namespace,
                    "class": type_name,
                    "field_type": field.field_type,
                    "field_name": field.field_name,
                    "offset": field.offset,
                    "signature": field.declaration,
                    "rva": Value::Null,
                    "line": idx + 1,
                }));
            }
        }
    }
    Ok(tool_ok(
        &format!("Found {total} IL2CPP field match(es)."),
        json!({ "source": "dump.cs", "query": query, "count": total, "truncated": total > matches.len(), "matches": matches }),
        Some("Use il2cpp_method_search to find getter/setter or related methods."),
    ))
}

fn il2cpp_method_detail(args: &Value, active_workspace: Option<&str>) -> Result<Value, String> {
    let root = artifact_root(args, active_workspace)?;
    let path = artifact_file(&root, "dump.cs")?;
    let rva = args.get("rva").and_then(Value::as_str);
    let query = args.get("query").and_then(Value::as_str);
    if rva.is_none() && query.is_none() {
        return Err("rva or query is required".into());
    }
    let limit = limit_arg(args, 5, 20);
    let case_sensitive = args
        .get("case_sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let (matches, total) =
        find_methods_in_dump(&path, limit, |namespace, class, signature, meta| {
            rva.is_some_and(|needle| meta.rva.as_deref().is_some_and(|m| same_rva(m, needle)))
                || query.is_some_and(|needle| {
                    matches_query(signature, needle, case_sensitive)
                        || matches_query(class, needle, case_sensitive)
                        || matches_query(namespace, needle, case_sensitive)
                })
        })?;
    Ok(tool_ok(
        &format!("Found {total} IL2CPP method detail match(es)."),
        json!({ "source": "dump.cs", "rva": rva, "query": query, "count": total, "truncated": total > matches.len(), "matches": matches }),
        Some("Use rva_to_address then gdb_hook_preview for a selected method."),
    ))
}

fn il2cpp_find_by_rva(args: &Value, active_workspace: Option<&str>) -> Result<Value, String> {
    let root = artifact_root(args, active_workspace)?;
    let path = artifact_file(&root, "dump.cs")?;
    let rva = required_str(args, "rva")?;
    let (matches, _) = find_methods_in_dump(&path, 1, |_namespace, _class, _signature, meta| {
        meta.rva.as_deref().is_some_and(|m| same_rva(m, rva))
    })?;
    let Some(found) = matches.into_iter().next() else {
        return Err(format!("rva not found: {rva}"));
    };
    Ok(tool_ok(
        "RVA match found.",
        json!({ "source": "dump.cs", "rva": rva, "match": found }),
        Some("Use rva_to_address with PID/module to compute runtime address."),
    ))
}

fn il2cpp_related_methods(args: &Value, active_workspace: Option<&str>) -> Result<Value, String> {
    let root = artifact_root(args, active_workspace)?;
    let path = artifact_file(&root, "dump.cs")?;
    let class_query = required_str(args, "class")?;
    let namespace_query = args.get("namespace").and_then(Value::as_str);
    let limit = limit_arg(args, 20, 50);
    let case_sensitive = args
        .get("case_sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let (matches, total) =
        find_methods_in_dump(&path, limit, |namespace, class, _signature, _meta| {
            matches_query(class, class_query, case_sensitive)
                && namespace_query.is_none_or(|q| matches_query(namespace, q, case_sensitive))
        })?;
    Ok(tool_ok(
        &format!("Found {total} related IL2CPP method(s)."),
        json!({ "source": "dump.cs", "class": class_query, "namespace_filter": namespace_query, "count": total, "truncated": total > matches.len(), "matches": matches }),
        Some("Use il2cpp_method_detail for a selected method."),
    ))
}

fn find_methods_in_dump<F>(
    path: &Path,
    limit: usize,
    mut keep: F,
) -> Result<(Vec<Value>, usize), String>
where
    F: FnMut(&str, &str, &str, &MethodMeta) -> bool,
{
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut namespace = String::new();
    let mut type_name = String::new();
    let mut pending = MethodMeta::default();
    let mut matches = Vec::new();
    let mut total = 0_usize;
    for (idx, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|e| e.to_string())?;
        let trimmed = line.trim();
        if let Some(ns) = trimmed.strip_prefix("// Namespace:") {
            namespace = ns.trim().to_string();
            continue;
        }
        if let Some(name) = parse_type_name(trimmed) {
            type_name = name;
            continue;
        }
        if trimmed.starts_with("// RVA:") {
            pending = parse_method_meta(trimmed);
            continue;
        }
        if pending.rva.is_some() && looks_like_method_decl(trimmed) {
            if keep(&namespace, &type_name, trimmed, &pending) {
                total += 1;
                if matches.len() < limit {
                    matches.push(method_match_json(
                        &namespace,
                        &type_name,
                        trimmed,
                        &pending,
                        idx + 1,
                    ));
                }
            }
            pending = MethodMeta::default();
        }
    }
    Ok((matches, total))
}

#[derive(Default)]
struct MethodMeta {
    rva: Option<String>,
    offset: Option<String>,
    va: Option<String>,
}

fn artifact_root(args: &Value, active_workspace: Option<&str>) -> Result<PathBuf, String> {
    if let Some(raw) = args.get("root").and_then(Value::as_str) {
        return checked_artifact_root(raw);
    }
    if let Some(workspace) = workspace_arg(args).or(active_workspace) {
        return detect_artifact_root(workspace_candidates(&checked_workspace_name(workspace)?)?);
    }
    let root = PathBuf::from(ARTIFACT_ROOT);
    detect_artifact_root(artifact_root_candidates(&root))
}

fn checked_artifact_root(raw: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw);
    if path.is_absolute()
        || raw.contains('\0')
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || !path.starts_with(ARTIFACT_ROOT)
    {
        return Err("root must be a relative path under reverse/".into());
    }
    Ok(path)
}

fn artifact_root_candidates(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![root.to_path_buf()];
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.push(path.join("tools"));
                out.push(path);
            }
        }
    }
    out
}

fn workspace_arg(args: &Value) -> Option<&str> {
    args.get("workspace")
        .and_then(Value::as_str)
        .or_else(|| args.get("game").and_then(Value::as_str))
}

fn checked_workspace_name(raw: &str) -> Result<String, String> {
    let path = Path::new(raw);
    if raw.trim().is_empty()
        || raw.contains('\0')
        || path.is_absolute()
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        return Err("workspace must be one relative path segment".into());
    }
    safe_name(&json!({ "workspace": raw }), "workspace")
}

fn workspace_candidates(name: &str) -> Result<Vec<PathBuf>, String> {
    let name = checked_workspace_name(name)?;
    let base = PathBuf::from(ARTIFACT_ROOT).join(name);
    Ok(vec![base.join("tools"), base])
}

fn detect_artifact_root(candidates: Vec<PathBuf>) -> Result<PathBuf, String> {
    for candidate in &candidates {
        if ARTIFACT_FILES
            .iter()
            .any(|name| artifact_file(candidate, name).is_ok_and(|p| p.exists()))
        {
            return Ok(candidate.clone());
        }
    }
    candidates
        .into_iter()
        .next()
        .ok_or_else(|| "no artifact root candidates".to_string())
}

fn artifact_status(root: &Path) -> Result<Value, String> {
    let dump_cs = artifact_file(root, "dump.cs")?;
    let script_json = artifact_file(root, "script.json")?;
    let stringliteral_json = artifact_file(root, "stringliteral.json")?;
    Ok(json!({
        "ready": dump_cs.exists() && script_json.exists() && stringliteral_json.exists(),
        "files": {
            "dump_cs": file_status(&dump_cs),
            "script_json": file_status(&script_json),
            "stringliteral_json": file_status(&stringliteral_json),
        }
    }))
}

fn artifact_file(root: &Path, name: &str) -> Result<PathBuf, String> {
    match name {
        "dump.cs" | "script.json" | "stringliteral.json" => Ok(root.join(name)),
        _ => Err("unsupported artifact file".into()),
    }
}

fn file_status(path: &PathBuf) -> Value {
    match path.metadata() {
        Ok(meta) => {
            json!({ "exists": true, "size_bytes": meta.len(), "path": path.to_string_lossy() })
        }
        Err(_) => json!({ "exists": false, "path": path.to_string_lossy() }),
    }
}

fn limit_arg(args: &Value, default: usize, max: usize) -> usize {
    args.get("limit")
        .and_then(Value::as_u64)
        .map(|n| (n as usize).clamp(1, max))
        .unwrap_or(default)
}

fn count_arg(args: &Value, default: usize, max: usize) -> usize {
    args.get("count")
        .and_then(Value::as_u64)
        .map(|n| (n as usize).clamp(1, max))
        .unwrap_or(default)
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

fn matches_query(haystack: &str, query: &str, case_sensitive: bool) -> bool {
    if case_sensitive {
        haystack.contains(query)
    } else {
        haystack.to_lowercase().contains(&query.to_lowercase())
    }
}

fn parse_type_name(line: &str) -> Option<String> {
    parse_type_decl(line).map(|decl| decl.name)
}

fn parse_method_meta(line: &str) -> MethodMeta {
    fn after(line: &str, key: &str) -> Option<String> {
        let start = line.find(key)? + key.len();
        line[start..]
            .split_whitespace()
            .next()
            .map(|s| s.trim_end_matches(',').to_string())
    }
    MethodMeta {
        rva: after(line, "RVA:"),
        offset: after(line, "Offset:"),
        va: after(line, "VA:"),
    }
}

fn looks_like_method_decl(line: &str) -> bool {
    line.contains('(') && line.ends_with(" { }")
}

struct TypeDecl {
    kind: String,
    name: String,
    base: Option<String>,
    type_def_index: Option<String>,
    declaration: String,
}

struct FieldDecl {
    field_type: String,
    field_name: String,
    offset: String,
    declaration: String,
}

fn parse_type_decl(line: &str) -> Option<TypeDecl> {
    if line.starts_with("//") || line.contains('(') {
        return None;
    }
    let words: Vec<&str> = line.split_whitespace().collect();
    let pos = words
        .iter()
        .position(|w| matches!(*w, "class" | "struct" | "interface"))?;
    let name = words.get(pos + 1)?.trim_end_matches(':').to_string();
    let base = words
        .get(pos + 3)
        .filter(|_| words.get(pos + 2) == Some(&":"))
        .map(|s| s.trim_end_matches(',').to_string());
    let type_def_index = line
        .split("TypeDefIndex:")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .map(|s| s.trim_end_matches(',').to_string());
    Some(TypeDecl {
        kind: words[pos].to_string(),
        name,
        base,
        type_def_index,
        declaration: line.to_string(),
    })
}

fn parse_field_decl(line: &str) -> Option<FieldDecl> {
    if line.contains('(') || !line.contains("; // 0x") {
        return None;
    }
    let (decl, comment) = line.split_once("; //")?;
    let offset = comment.split_whitespace().find(|s| s.starts_with("0x"))?;
    let parts: Vec<&str> = decl.split_whitespace().collect();
    let field_name = parts.last()?.to_string();
    let field_type = parts.get(parts.len().checked_sub(2)?)?.to_string();
    Some(FieldDecl {
        field_type,
        field_name,
        offset: offset.to_string(),
        declaration: line.to_string(),
    })
}

fn same_rva(a: &str, b: &str) -> bool {
    fn norm(s: &str) -> String {
        s.trim()
            .trim_start_matches("0x")
            .trim_start_matches("0X")
            .trim_start_matches('0')
            .to_ascii_lowercase()
    }
    norm(a) == norm(b)
}

fn method_match_json(
    namespace: &str,
    class: &str,
    signature: &str,
    meta: &MethodMeta,
    line: usize,
) -> Value {
    json!({
        "namespace": namespace,
        "class": class,
        "type": class,
        "signature": signature,
        "rva": meta.rva,
        "offset": meta.offset,
        "va": meta.va,
        "line": line,
    })
}

fn value_as_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn script_sections(section: &str) -> Result<Vec<(&'static str, &'static str)>, String> {
    match section {
        "methods" => Ok(vec![("methods", "ScriptMethod")]),
        "strings" => Ok(vec![("strings", "ScriptString")]),
        "metadata" => Ok(vec![("metadata", "ScriptMetadata")]),
        "all" => Ok(vec![
            ("methods", "ScriptMethod"),
            ("strings", "ScriptString"),
            ("metadata", "ScriptMetadata"),
        ]),
        _ => Err("section must be one of: methods, strings, metadata, all".into()),
    }
}

fn script_match(section: &str, index: usize, item: &Value) -> Value {
    json!({
        "section": section,
        "index": index,
        "name": item.get("Name").cloned().unwrap_or(Value::Null),
        "signature": item.get("Signature").and_then(Value::as_str).map(|s| clip(s, ARTIFACT_PREVIEW_MAX)),
        "address": item.get("Address").cloned().unwrap_or(Value::Null),
        "rva": item.get("Address").and_then(Value::as_u64).map(hex),
        "value_preview": item.get("value").or_else(|| item.get("Value")).and_then(Value::as_str).map(|s| clip(s, ARTIFACT_PREVIEW_MAX)),
    })
}

fn reverse_report_create(args: &Value, active_workspace: Option<&str>) -> Result<Value, String> {
    let root = artifact_root(args, active_workspace)?;
    let title = report_text(args, "title", REPORT_TITLE_MAX)?;
    let summary = optional_report_text(args, "summary", REPORT_TEXT_MAX);
    let path = reverse_report_path(
        &root,
        args.get("report").and_then(Value::as_str).unwrap_or(&title),
    )?;
    let data = json!({
        "title": title,
        "summary": summary,
        "created_at": now_secs(),
        "updated_at": now_secs(),
        "findings": [],
    });
    write_json(&path, &data)?;
    write_report_markdown(&path, &data)?;
    Ok(tool_ok(
        "Reverse report created.",
        json!({ "report": path.to_string_lossy(), "markdown": report_md_path(&path).to_string_lossy(), "data": data }),
        Some("Add findings with reverse_report_add_finding."),
    ))
}

fn reverse_report_add_finding(
    args: &Value,
    active_workspace: Option<&str>,
) -> Result<Value, String> {
    let root = artifact_root(args, active_workspace)?;
    let path = reverse_report_path(&root, required_str(args, "report")?)?;
    let mut data = read_json(&path)?;
    let finding = json!({
        "title": report_text(args, "title", REPORT_TITLE_MAX)?,
        "summary": report_text(args, "summary", REPORT_TEXT_MAX)?,
        "severity": optional_report_field(args, "severity"),
        "category": optional_report_field(args, "category"),
        "source": optional_report_field(args, "source"),
        "module": optional_report_field(args, "module"),
        "rva": optional_report_field(args, "rva"),
        "address": optional_report_field(args, "address"),
        "class": optional_report_field(args, "class"),
        "method": optional_report_field(args, "method"),
        "field": optional_report_field(args, "field"),
        "offset": optional_report_field(args, "offset"),
        "notes": optional_report_text(args, "notes", REPORT_TEXT_MAX),
        "created_at": now_secs(),
    });
    data["findings"]
        .as_array_mut()
        .ok_or("report findings must be array")?
        .push(finding);
    data["updated_at"] = json!(now_secs());
    write_json(&path, &data)?;
    write_report_markdown(&path, &data)?;
    Ok(tool_ok(
        "Reverse finding added.",
        json!({ "report": path.to_string_lossy(), "markdown": report_md_path(&path).to_string_lossy(), "finding_count": data["findings"].as_array().map(Vec::len).unwrap_or(0), "data": data }),
        None,
    ))
}

fn reverse_report_list(args: &Value, active_workspace: Option<&str>) -> Result<Value, String> {
    let root = artifact_root(args, active_workspace)?;
    let dir = reverse_reports_dir(&root)?;
    let mut reports = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let data = read_json(&path)?;
            reports.push(json!({
                "report": path.to_string_lossy(),
                "markdown": report_md_path(&path).to_string_lossy(),
                "title": data.get("title").cloned().unwrap_or(Value::Null),
                "summary": data.get("summary").cloned().unwrap_or(Value::Null),
                "finding_count": data.get("findings").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
                "updated_at": data.get("updated_at").cloned().unwrap_or(Value::Null),
            }));
        }
    }
    reports.sort_by_key(|r| r["report"].as_str().unwrap_or("").to_string());
    Ok(tool_ok(
        &format!("Found {} reverse report(s).", reports.len()),
        json!({ "root": root.to_string_lossy(), "reports": reports }),
        Some("Use reverse_report_add_finding with a report name/path."),
    ))
}

fn reverse_reports_dir(root: &Path) -> Result<PathBuf, String> {
    let dir = root.join("reports");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn reverse_report_path(root: &Path, raw: &str) -> Result<PathBuf, String> {
    let reports_dir = reverse_reports_dir(root)?;
    let trimmed = raw.trim().trim_end_matches(".json");
    let root_prefix = reports_dir.to_string_lossy();
    let file = trimmed
        .strip_prefix(root_prefix.as_ref())
        .and_then(|s| s.strip_prefix('/').or_else(|| s.strip_prefix('\\')))
        .or_else(|| {
            trimmed
                .strip_prefix("reports/")
                .or_else(|| trimmed.strip_prefix("reports\\"))
        })
        .unwrap_or(trimmed);
    let name = safe_file_stem(file)?;
    Ok(reports_dir.join(format!("{name}.json")))
}

fn report_md_path(path: &Path) -> PathBuf {
    path.with_extension("md")
}

fn write_report_markdown(path: &Path, data: &Value) -> Result<(), String> {
    let mut out = format!(
        "# {}\n\n{}\n\n",
        data["title"].as_str().unwrap_or("Reverse report"),
        data["summary"].as_str().unwrap_or("")
    );
    if let Some(findings) = data["findings"].as_array() {
        for finding in findings {
            out.push_str(&format!(
                "## {}\n\n{}\n\n",
                finding["title"].as_str().unwrap_or("Finding"),
                finding["summary"].as_str().unwrap_or("")
            ));
            for key in [
                "severity", "category", "source", "module", "rva", "address", "class", "method",
                "field", "offset",
            ] {
                if let Some(value) = finding[key].as_str().filter(|s| !s.is_empty()) {
                    out.push_str(&format!("- {key}: `{value}`\n"));
                }
            }
            if let Some(notes) = finding["notes"].as_str().filter(|s| !s.is_empty()) {
                out.push_str(&format!("\n{notes}\n"));
            }
            out.push('\n');
        }
    }
    fs::write(report_md_path(path), out).map_err(|e| e.to_string())
}

fn report_text(args: &Value, name: &str, max: usize) -> Result<String, String> {
    let s = required_str(args, name)?.trim();
    if s.contains('\0') {
        return Err(format!("{name} contains unsupported characters"));
    }
    Ok(clip(s, max))
}

fn optional_report_text(args: &Value, name: &str, max: usize) -> String {
    args.get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| clip(s, max))
        .unwrap_or_default()
}

fn optional_report_field(args: &Value, name: &str) -> String {
    optional_report_text(args, name, REPORT_FIELD_MAX).replace('\n', " ")
}

#[derive(Clone)]
struct ModuleMap {
    base: u64,
    end: u64,
    perms: String,
    path: String,
}

struct MapEntry {
    start: u64,
    end: u64,
    perms: String,
    offset: String,
    dev: String,
    inode: String,
    path: String,
    raw: String,
}

fn read_maps_entries(pid: u64) -> Result<Vec<MapEntry>, String> {
    let maps = fs::read_to_string(format!("/proc/{pid}/maps")).map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    for line in maps.lines() {
        let mut parts = line.split_whitespace();
        let Some(range) = parts.next() else { continue };
        let Some(perms) = parts.next() else { continue };
        let offset = parts.next().unwrap_or("").to_string();
        let dev = parts.next().unwrap_or("").to_string();
        let inode = parts.next().unwrap_or("").to_string();
        let path = parts.collect::<Vec<_>>().join(" ");
        let Some((start, end)) = range.split_once('-') else {
            continue;
        };
        let Ok(start) = u64::from_str_radix(start, 16) else {
            continue;
        };
        let Ok(end) = u64::from_str_radix(end, 16) else {
            continue;
        };
        entries.push(MapEntry {
            start,
            end,
            perms: perms.to_string(),
            offset,
            dev,
            inode,
            path,
            raw: line.to_string(),
        });
    }
    Ok(entries)
}

fn map_entry_json(m: &MapEntry) -> Value {
    json!({
        "start": hex(m.start),
        "end": hex(m.end),
        "perms": &m.perms,
        "offset": &m.offset,
        "dev": &m.dev,
        "inode": &m.inode,
        "path": &m.path,
        "raw": &m.raw,
    })
}

fn read_modules(pid: u64) -> Result<Vec<ModuleMap>, String> {
    let maps = fs::read_to_string(format!("/proc/{pid}/maps")).map_err(|e| e.to_string())?;
    let mut modules = Vec::new();
    for line in maps.lines() {
        let mut parts = line.split_whitespace();
        let Some(range) = parts.next() else { continue };
        let Some(perms) = parts.next() else { continue };
        let _offset = parts.next();
        let _dev = parts.next();
        let _inode = parts.next();
        let path = parts.collect::<Vec<_>>().join(" ");
        if path.is_empty() || path.starts_with('[') {
            continue;
        }
        let Some((start, end)) = range.split_once('-') else {
            continue;
        };
        let Ok(base) = u64::from_str_radix(start, 16) else {
            continue;
        };
        let Ok(end) = u64::from_str_radix(end, 16) else {
            continue;
        };
        modules.push(ModuleMap {
            base,
            end,
            perms: perms.to_string(),
            path,
        });
    }
    modules.sort_by_key(|m| (m.path.clone(), m.base));
    modules.dedup_by(|a, b| a.path == b.path);
    Ok(modules)
}

fn find_module(pid: u64, module: &str) -> Result<ModuleMap, String> {
    let needle = module.to_lowercase();
    read_modules(pid)?
        .into_iter()
        .find(|m| {
            let path = m.path.to_lowercase();
            let file = Path::new(&m.path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            file == needle || path.contains(&needle)
        })
        .ok_or_else(|| format!("module not found: {module}"))
}

fn parse_u64(s: &str) -> Result<u64, String> {
    let s = s.trim().replace('_', "");
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).map_err(|e| e.to_string())
    } else {
        s.parse::<u64>().map_err(|e| e.to_string())
    }
}

fn hex(n: u64) -> String {
    format!("0x{n:x}")
}

fn clip_out(s: &str) -> String {
    clip(s, OUT_MAX)
}

fn parse_process_line(line: &str) -> Option<Value> {
    let mut parts = line.trim().splitn(3, char::is_whitespace);
    let pid: u64 = parts.next()?.parse().ok()?;
    let name = parts.next().unwrap_or("");
    let cmdline = parts.next().unwrap_or("");
    Some(json!({"pid": pid, "name": name, "cmdline": cmdline, "likely_game": likely_game(cmdline)}))
}
fn is_system_process(p: &Value) -> bool {
    p["pid"].as_u64().unwrap_or(0) < 1000 || p["cmdline"].as_str().unwrap_or("").starts_with('[')
}
fn likely_game(cmd: &str) -> bool {
    ["steam", "wine", "game", "unity", "unreal"]
        .iter()
        .any(|w| cmd.to_lowercase().contains(w))
}
fn score_process(p: &Value) -> i64 {
    (if p["likely_game"].as_bool() == Some(true) {
        100
    } else {
        0
    }) + p["cmdline"].as_str().unwrap_or("").len() as i64
}

fn table_create(args: &Value) -> Result<Value, String> {
    let game = safe_name(args, "game")?;
    let process = safe_name(args, "process")?;
    let notes = args.get("notes").and_then(Value::as_str).unwrap_or("");
    let table = json!({"game": game, "process": process, "notes": notes, "entries": []});
    let path = table_path(&format!("{}-{}", game, process))?;
    write_json(&path, &table)?;
    Ok(tool_ok(
        "Cheat table created.",
        json!({"table": path.to_string_lossy(), "data": table}),
        Some("Add entries with table_add_entry."),
    ))
}
fn table_add_entry(args: &Value) -> Result<Value, String> {
    let path = table_path(required_str(args, "table")?)?;
    let mut table = read_json(&path)?;
    let entry = json!({
        "name": safe_name(args,"name")?,
        "scan": required_str(args,"scan")?,
        "value_type": valid_value_type(args)?,
        "last_value": args.get("last_value").and_then(Value::as_str).unwrap_or(""),
        "notes": args.get("notes").and_then(Value::as_str).unwrap_or(""),
        "module": args.get("module").and_then(Value::as_str).unwrap_or(""),
        "rva": args.get("rva").and_then(Value::as_str).unwrap_or(""),
        "method_signature": args.get("method_signature").and_then(Value::as_str).unwrap_or(""),
        "scan_query": args.get("scan_query").and_then(Value::as_str).unwrap_or(""),
    });
    table["entries"]
        .as_array_mut()
        .ok_or("table entries must be array")?
        .push(entry);
    write_json(&path, &table)?;
    Ok(tool_ok(
        "Entry added.",
        json!({"table": path.to_string_lossy(), "data": table}),
        None,
    ))
}
fn table_load(args: &Value) -> Result<Value, String> {
    let path = table_path(required_str(args, "table")?)?;
    Ok(tool_ok(
        "Cheat table loaded.",
        json!({"table": path.to_string_lossy(), "data": read_json(&path)?}),
        None,
    ))
}
fn table_save(args: &Value) -> Result<Value, String> {
    let path = table_path(required_str(args, "table")?)?;
    let data = args.get("data").cloned().ok_or("data is required")?;
    write_json(&path, &data)?;
    Ok(tool_ok(
        "Cheat table saved.",
        json!({"table": path.to_string_lossy(), "data": data}),
        None,
    ))
}
fn table_list_entries(args: &Value) -> Result<Value, String> {
    let path = table_path(required_str(args, "table")?)?;
    let data = read_json(&path)?;
    Ok(tool_ok(
        "Entries listed.",
        json!({"table": path.to_string_lossy(), "entries": data["entries"]}),
        None,
    ))
}

fn table_resolve_entries(args: &Value) -> Result<Value, String> {
    let path = table_path(required_str(args, "table")?)?;
    let pid = valid_live_pid(args)?;
    let mut table = read_json(&path)?;
    let entries = table["entries"]
        .as_array_mut()
        .ok_or("table entries must be array")?;
    let mut results = Vec::new();
    for entry in entries {
        let result = resolve_table_entry(pid, entry);
        entry["resolve_status"] = json!(result["status"].as_str().unwrap_or("unknown"));
        if let Some(address) = result.get("address").and_then(Value::as_str) {
            entry["resolved_address"] = json!(address);
        }
        results.push(result);
    }
    write_json(&path, &table)?;
    Ok(tool_ok(
        "Table entries resolved.",
        json!({"pid": pid, "table": path.to_string_lossy(), "results": results, "data": table}),
        Some("Run table_validate_entries to check mappings/readability."),
    ))
}

fn table_validate_entries(args: &Value) -> Result<Value, String> {
    let path = table_path(required_str(args, "table")?)?;
    let pid = valid_live_pid(args)?;
    let read_values = args
        .get("read_values")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let table = read_json(&path)?;
    let entries = table["entries"].as_array().ok_or("entries must be array")?;
    let mut results = Vec::new();
    for entry in entries {
        let mut result = resolve_table_entry(pid, entry);
        let mut warnings = Vec::new();
        if result["status"] == "resolved" {
            let address = parse_u64(result["address"].as_str().unwrap_or("0"))?;
            match read_process_memory(pid, address, 1) {
                Ok(_) => result["readable"] = json!(true),
                Err(e) => {
                    result["readable"] = json!(false);
                    warnings.push(e);
                }
            }
            if read_values {
                match table_value_preview(pid, address, entry) {
                    Ok(value) => result["value_preview"] = value,
                    Err(e) => warnings.push(e),
                }
            }
        } else if entry
            .get("method_signature")
            .and_then(Value::as_str)
            .unwrap_or("")
            .is_empty()
            && entry
                .get("scan_query")
                .and_then(Value::as_str)
                .unwrap_or("")
                .is_empty()
        {
            warnings.push("no module/rva, method_signature, or scan_query".to_string());
        }
        result["warnings"] = json!(warnings);
        result["valid"] = json!(result["status"] == "resolved" && warnings.is_empty());
        results.push(result);
    }
    Ok(tool_ok(
        "Table entries validated.",
        json!({"pid": pid, "table": path.to_string_lossy(), "results": results}),
        None,
    ))
}

fn resolve_table_entry(pid: u64, entry: &Value) -> Value {
    let name = entry.get("name").and_then(Value::as_str).unwrap_or("");
    let module = entry.get("module").and_then(Value::as_str).unwrap_or("");
    let rva_s = entry.get("rva").and_then(Value::as_str).unwrap_or("");
    if module.is_empty() || rva_s.is_empty() {
        return json!({"name": name, "status": "no_module_rva"});
    }
    let rva = match parse_u64(rva_s) {
        Ok(rva) => rva,
        Err(e) => {
            return json!({"name": name, "module": module, "rva": rva_s, "status": "invalid_rva", "error": e})
        }
    };
    match find_module(pid, module) {
        Ok(m) => match m.base.checked_add(rva) {
            Some(address) => {
                json!({"name": name, "module": module, "path": m.path, "module_base": hex(m.base), "rva": hex(rva), "address": hex(address), "status": "resolved"})
            }
            None => json!({"name": name, "module": module, "rva": rva_s, "status": "overflow"}),
        },
        Err(e) => {
            json!({"name": name, "module": module, "rva": rva_s, "status": "module_not_found", "error": e})
        }
    }
}

fn table_value_preview(pid: u64, address: u64, entry: &Value) -> Result<Value, String> {
    let value_type = entry
        .get("value_type")
        .and_then(Value::as_str)
        .unwrap_or("int32");
    let size = match value_type {
        "int64" | "double" => 8,
        "string" => 64,
        _ => 4,
    };
    let (bytes, _) = read_process_memory(pid, address, size)?;
    Ok(json!({"value_type": value_type, "hex": bytes_hex(&bytes), "ascii": bytes_ascii(&bytes)}))
}

fn required_str<'a>(args: &'a Value, name: &str) -> Result<&'a str, String> {
    args.get(name)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("{name} is required"))
}
fn safe_name(args: &Value, name: &str) -> Result<String, String> {
    let s = required_str(args, name)?.trim();
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || " ._-".contains(c))
    {
        Ok(s.to_string())
    } else {
        Err(format!("{name} contains unsupported characters"))
    }
}
fn safe_file_stem(raw: &str) -> Result<String, String> {
    let stem = raw.trim().trim_end_matches(".json").trim_end_matches(".md");
    if stem.is_empty() || stem.contains('/') || stem.contains('\\') || stem.contains('\0') {
        return Err("invalid report path".into());
    }
    let name: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .take(80)
        .collect();
    let name = name.trim_matches(['.', '-']);
    if name.is_empty() || name.contains("..") {
        Err("invalid report path".into())
    } else {
        Ok(name.to_string())
    }
}
fn table_path(name: &str) -> Result<PathBuf, String> {
    let file = name
        .trim()
        .trim_end_matches(".json")
        .strip_prefix(".cheat-tables/")
        .or_else(|| {
            name.trim()
                .trim_end_matches(".json")
                .strip_prefix(".cheat-tables\\")
        })
        .unwrap_or_else(|| name.trim().trim_end_matches(".json"));
    if !file
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
        || file.contains("..")
        || file.is_empty()
    {
        return Err("invalid table path".into());
    }
    let mut p = PathBuf::from(".cheat-tables");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    p.push(format!("{file}.json"));
    Ok(p)
}
fn read_json(path: &PathBuf) -> Result<Value, String> {
    serde_json::from_str(&fs::read_to_string(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
fn write_json(path: &PathBuf, data: &Value) -> Result<(), String> {
    fs::write(
        path,
        serde_json::to_string_pretty(data).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn scanmem_attach_process(args: &Value, sessions: &mut Sessions) -> Result<Value, String> {
    let pid = valid_pid(args)?;
    let sess = ensure_session(sessions, pid)?;
    let output = scanmem_send(proc_of(sess)?, "list")?;
    touch_session_on(sess, "attach", &output);
    Ok(tool_ok(
        "Process selected by scanmem.",
        json!({ "pid": pid, "output": output }),
        Some("Run scanmem_scan_exact or scanmem_reset_scan next."),
    ))
}

fn scanmem_simple_command(
    args: &Value,
    command: &str,
    message: &str,
    sessions: &mut Sessions,
) -> Result<Value, String> {
    let pid = valid_pid(args)?;
    let prefix = args
        .get("value")
        .and_then(Value::as_str)
        .map(|_| valid_value_arg(args, "value"))
        .transpose()?;
    let mut script = String::new();
    if let Some(value) = prefix {
        script.push_str(&value);
        script.push('\n');
    }
    script.push_str(command);
    let sess = ensure_session(sessions, pid)?;
    let output = scanmem_send(proc_of(sess)?, &script)?;
    touch_session_on(sess, command, &output);
    Ok(tool_ok(
        message,
        json!({ "pid": pid, "output": output }),
        None,
    ))
}

fn scanmem_refine_scan(
    args: &Value,
    command: &str,
    message: &str,
    sessions: &mut Sessions,
) -> Result<Value, String> {
    let pid = valid_pid(args)?;
    // ponytail: initial_value unused — stateful refine relies on prior scan/snapshot
    // in the same persistent scanmem child. Re-sending the value would do an exact scan
    // and wipe the snapshot.
    let initial_value = valid_value_arg(args, "initial_value")?;
    let op = match command {
        "+" => ">",
        "-" => "<",
        "changed" => "!=",
        "unchanged" => "=",
        other => other,
    };
    let sess = ensure_session(sessions, pid)?;
    let output = scanmem_send(proc_of(sess)?, op)?;
    touch_session_on(sess, command, &output);
    Ok(tool_ok(
        message,
        json!({ "pid": pid, "initial_value": initial_value, "refine": command, "output": output }),
        Some("Use scanmem_list_matches or refine again."),
    ))
}

fn scanmem_pick_match(args: &Value) -> Result<Value, String> {
    let output = args
        .get("output")
        .and_then(Value::as_str)
        .ok_or_else(|| "output is required".to_string())?;
    let index = args
        .get("index")
        .and_then(Value::as_u64)
        .ok_or_else(|| "index is required".to_string())? as usize;
    let matches: Vec<&str> = output
        .lines()
        .filter(|line| line.contains(']') && line.contains("0x"))
        .collect();
    let picked = matches
        .get(index)
        .ok_or_else(|| "match index out of range".to_string())?;
    Ok(tool_ok(
        "Match picked.",
        json!({ "index": index, "match": picked }),
        Some("Use this as reference before write_selected in Phase 4."),
    ))
}

fn session_create(args: &Value, sessions: &mut Sessions) -> Result<Value, String> {
    linux_only("scanmem sessions")?;
    let pid = valid_pid(args)?;
    if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
        return Err("pid is not running".to_string());
    }
    let session = sessions.entry(pid).or_insert_with(|| new_session(pid));
    Ok(tool_ok(
        "Session created.",
        session_json(session),
        Some("Run scanmem_scan_exact or scanmem_scan_unknown for this PID."),
    ))
}

fn session_status(args: &Value, sessions: &mut Sessions) -> Result<Value, String> {
    expire_sessions(sessions);
    if let Some(pid) = args.get("pid").and_then(Value::as_u64) {
        let session = sessions
            .get(&pid)
            .ok_or_else(|| "session not found for pid".to_string())?;
        return Ok(tool_ok(
            "Session found.",
            session_json(session),
            Some("Continue scan or close the session."),
        ));
    }
    let all: Vec<Value> = sessions.values().map(session_json).collect();
    Ok(tool_ok(
        &format!("{} active session(s).", all.len()),
        json!({ "sessions": all, "timeout_secs": SESSION_TIMEOUT_SECS }),
        None,
    ))
}

fn session_close(args: &Value, sessions: &mut Sessions) -> Result<Value, String> {
    let pid = valid_pid(args)?;
    let existed = sessions.remove(&pid).is_some();
    Ok(tool_ok(
        if existed {
            "Session closed."
        } else {
            "No session existed for PID."
        },
        json!({ "pid": pid, "closed": existed }),
        None,
    ))
}

fn new_session(pid: u64) -> Session {
    let now = now_secs();
    Session {
        pid,
        created_at: now,
        last_seen: now,
        last_command: "session_create".to_string(),
        last_output: String::new(),
        last_match_count: 0,
        frozen_value: None,
        proc: None,
    }
}

fn session_json(session: &Session) -> Value {
    json!({ "pid": session.pid, "created_at": session.created_at, "last_seen": session.last_seen, "last_command": session.last_command, "last_output": session.last_output, "last_match_count": session.last_match_count, "frozen_value": session.frozen_value, "timeout_secs": SESSION_TIMEOUT_SECS })
}

fn touch_session_on(sess: &mut Session, command: &str, output: &str) {
    sess.last_seen = now_secs();
    sess.last_command = command.to_string();
    sess.last_match_count = count_matches(output);
    sess.last_output = output.to_string();
}

fn expire_sessions(sessions: &mut Sessions) {
    let now = now_secs();
    sessions.retain(|_, session| now.saturating_sub(session.last_seen) <= SESSION_TIMEOUT_SECS);
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}
fn scan_args(args: &Value) -> Result<(u64, String), String> {
    Ok((valid_pid(args)?, valid_value_arg(args, "value")?))
}
fn valid_pid(args: &Value) -> Result<u64, String> {
    args.get("pid")
        .and_then(Value::as_u64)
        .filter(|pid| *pid > 0)
        .ok_or_else(|| "pid is required".to_string())
}
fn valid_live_pid(args: &Value) -> Result<u64, String> {
    linux_only("pid liveness checks")?;
    let pid = valid_pid(args)?;
    if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
        return Err("pid is not running".to_string());
    }
    Ok(pid)
}
fn count_matches(output: &str) -> usize {
    output
        .lines()
        .filter(|line| line.contains(']') && line.contains("0x"))
        .count()
}

fn valid_value_arg(args: &Value, name: &str) -> Result<String, String> {
    let value = args
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("{name} is required"))?;
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || " .+-_xX".contains(c))
    {
        return Err(format!("{name} contains unsupported characters"));
    }
    Ok(value.to_string())
}

fn valid_value_type(args: &Value) -> Result<String, String> {
    let value_type = args
        .get("value_type")
        .and_then(Value::as_str)
        .unwrap_or("auto")
        .to_lowercase();
    match value_type.as_str() {
        "auto" | "int32" | "int64" | "float" | "double" | "string" | "hex" => Ok(value_type),
        _ => Err(
            "value_type must be one of: auto, int32, int64, float, double, string, hex".to_string(),
        ),
    }
}

fn typed_value(value: &str, value_type: &str) -> String {
    match value_type {
        "string" => format!("\"{value}\""),
        "hex" if !value.starts_with("0x") && !value.starts_with("0X") => format!("0x{value}"),
        _ => value.to_string(),
    }
}

// One long-lived `scanmem -p PID` per session: scanmem's match list lives only in its
// process, so a fresh child per call (the old --command approach) wiped matches and made
// snapshot→refine impossible. We feed commands over stdin and drain until a `shell echo`
// sentinel. Run under `script` so scanmem sees a pty and flushes scan progress (a bare
// pipe makes scanmem buffer its "searching ... ok" output until exit, hiding match counts).
fn spawn_scanmem(pid: u64) -> Result<(ScanmemProc, String), String> {
    let mut child = Command::new("script")
        .arg("-qec")
        .arg(format!("scanmem -p {pid}"))
        .arg("/dev/null")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("failed to run scanmem (via script): {err}"))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "no scanmem stdin".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "no scanmem stdout".to_string())?;
    let mut proc = ScanmemProc {
        child,
        stdin,
        stdout: BufReader::new(stdout),
    };
    // Drain scanmem's startup banner by sending one sentinel and reading until it appears.
    let banner = send_and_drain(&mut proc, "", &format!("shell echo {SM_DONE}"))?;
    Ok((proc, banner))
}

// ponytail: blocking read, no watchdog — a hung scanmem blocks the session.
// Add a reader-thread + recv_timeout if a scan ever hangs in practice.
fn scanmem_send(proc: &mut ScanmemProc, cmd: &str) -> Result<String, String> {
    send_and_drain(proc, cmd, &format!("shell echo {SM_DONE}"))
}

// Send `cmd` then a `shell echo SENTINEL` marker; read stdout until the sentinel appears.
// The pty local-echos our input, so we drop any line that matches a command we just sent
// (the echo) — that keeps the returned text to scanmem's actual output only.
fn send_and_drain(proc: &mut ScanmemProc, cmd: &str, sentinel: &str) -> Result<String, String> {
    let mut sent: Vec<String> = Vec::new();
    for line in cmd.lines() {
        if line.trim().is_empty() {
            continue;
        }
        writeln!(proc.stdin, "{line}").map_err(|e| format!("scanmem write: {e}"))?;
        sent.push(line.trim().to_string());
    }
    // Track the sentinel command too — the pty local-echos it, and that echo line
    // contains the sentinel string, so a naive contains() would break too early on the
    // echo rather than on scanmem's actual `__SM_DONE__` output.
    writeln!(proc.stdin, "{sentinel}").map_err(|e| format!("scanmem write: {e}"))?;
    sent.push(sentinel.trim().to_string());
    proc.stdin
        .flush()
        .map_err(|e| format!("scanmem flush: {e}"))?;

    let mut out = String::new();
    let mut line = String::new();
    loop {
        line.clear();
        let n = proc
            .stdout
            .read_line(&mut line)
            .map_err(|e| format!("scanmem read: {e}"))?;
        if n == 0 {
            return Err("scanmem exited unexpectedly".to_string());
        }
        let cleaned = strip_pty_escapes(&line);
        let trimmed = cleaned.trim_end();
        let stripped = trimmed.trim_start();
        // Break only on scanmem's sentinel OUTPUT line — a bare marker, not the echoed
        // command `shell echo __SM_DONE__` (which we skip as a sent echo below).
        if stripped == SM_DONE {
            break;
        }
        // Skip prompts, counted prompts, and the pty's local echo of our sent commands.
        if stripped == ">"
            || stripped.starts_with("> ")
            || is_counted_prompt(stripped)
            || sent.iter().any(|s| stripped == s)
        {
            continue;
        }
        if !trimmed.is_empty() {
            out.push_str(trimmed);
            out.push('\n');
        }
    }
    // ponytail: scanmem prints `error:` lines on stdout in piped mode, so we don't
    // drain stderr — that would block since the child stays alive across calls.
    let trimmed = out.trim();
    let text = if trimmed.len() > OUT_MAX {
        let cut = trimmed.len() - OUT_MAX;
        &trimmed[cut..]
    } else {
        trimmed
    };
    Ok(text.to_string())
}

// ponytail: scanmem under a pty emits CSI escapes (bracketed paste \x1b[?2004h/l, CR).
// Minimal strip — drops \r and ESC[... sequences; sufficient for scan output lines.
fn strip_pty_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            // skip ESC [ ... until a final byte 0x40..0x7e
            i += 2;
            while i < bytes.len() && !(0x40..=0x7e).contains(&bytes[i]) {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            continue;
        }
        if b == b'\r' {
            i += 1;
            continue;
        }
        out.push(b as char);
        i += 1;
    }
    out
}

// scanmem prefixes prompt lines with the running match count, e.g. "335872> snapshot".
// These are command echoes / prompts, not scan output — skip them.
fn is_counted_prompt(s: &str) -> bool {
    let Some((count, _rest)) = s.split_once('>') else {
        return false;
    };
    !count.is_empty() && count.chars().all(|c| c.is_ascii_digit())
}

fn ensure_session<'a>(sessions: &'a mut Sessions, pid: u64) -> Result<&'a mut Session, String> {
    linux_only("scanmem sessions")?;
    if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
        return Err("pid is not running".to_string());
    }
    let entry = sessions.entry(pid).or_insert_with(|| new_session(pid));
    if entry.proc.is_none() {
        let (proc, _banner) = spawn_scanmem(pid)?;
        entry.proc = Some(proc);
    }
    Ok(entry)
}

fn ok(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}
fn error(id: Option<Value>, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_scanmem_tools() {
        let mut state = new_state();
        let res = handle(
            Request {
                jsonrpc: Some("2.0".into()),
                id: Some(json!(1)),
                method: "tools/list".into(),
                params: json!({}),
            },
            &mut state,
        );
        let tools = res["result"]["tools"].as_array().unwrap();
        assert!(tools.iter().any(|tool| tool["name"] == "scanmem_version"));
        assert!(tools.iter().any(|tool| tool["name"] == "session_create"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "scanmem_freeze_value"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "scanmem_scan_by_type"));
        assert!(tools.iter().any(|tool| tool["name"] == "process_search"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "process_module_base"));
        assert!(tools.iter().any(|tool| tool["name"] == "process_read_maps"));
        assert!(tools.iter().any(|tool| tool["name"] == "rva_to_address"));
        assert!(tools.iter().any(|tool| tool["name"] == "address_to_rva"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "rva_disassemble_preview"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "gdb_disassemble_address"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "gdb_breakpoint_probe_preview"));
        assert!(tools.iter().any(|tool| tool["name"] == "gdb_probe_preview"));
        assert!(tools.iter().any(|tool| tool["name"] == "gdb_probe_start"));
        assert!(tools.iter().any(|tool| tool["name"] == "gdb_probe_stop"));
        assert!(tools.iter().any(|tool| tool["name"] == "memory_read_bytes"));
        assert!(tools.iter().any(|tool| tool["name"] == "memory_read_int"));
        assert!(tools.iter().any(|tool| tool["name"] == "memory_read_float"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "memory_read_string"));
        assert!(tools.iter().any(|tool| tool["name"] == "gdb_hook_preview"));
        assert!(tools.iter().any(|tool| tool["name"] == "gdb_hook_start"));
        assert!(tools.iter().any(|tool| tool["name"] == "gdb_hook_stop"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "gdb_hook_group_preview"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "gdb_hook_group_start"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "gdb_hook_group_stop"));
        assert!(tools.iter().any(|tool| tool["name"] == "workspace_list"));
        assert!(tools.iter().any(|tool| tool["name"] == "workspace_status"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "workspace_set_active"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "reverse_report_create"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "reverse_report_add_finding"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "reverse_report_list"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "il2cpp_artifacts_status"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "il2cpp_method_search"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "il2cpp_script_search"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "il2cpp_class_search"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "il2cpp_field_search"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "il2cpp_method_detail"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "il2cpp_find_by_rva"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "il2cpp_related_methods"));
        assert!(tools.iter().any(|tool| tool["name"] == "table_create"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "table_resolve_entries"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "table_validate_entries"));
    }

    #[test]
    fn initialize_reports_package_version() {
        let mut state = new_state();
        let res = handle(
            Request {
                jsonrpc: Some("2.0".into()),
                id: Some(json!(1)),
                method: "initialize".into(),
                params: json!({}),
            },
            &mut state,
        );
        assert_eq!(
            res["result"]["serverInfo"]["version"],
            env!("CARGO_PKG_VERSION")
        );
    }

    #[test]
    fn tool_ok_has_explain_fields() {
        let res = tool_ok("done", json!({"match_count": 101}), None);
        assert_eq!(res["ok"], true);
        assert!(res["summary"].as_str().unwrap().contains("match_count=101"));
        assert!(!res["warnings"].as_array().unwrap().is_empty());
    }

    #[test]
    fn dummy_target_manifest_exists() {
        assert!(std::path::Path::new("examples/dummy-target/Cargo.toml").exists());
    }

    #[test]
    fn previews_scanmem_script() {
        let res = scanmem_script_preview(&json!({ "pid": 123, "value": "100" })).unwrap();
        let script = res["data"]["script"].as_str().unwrap();
        assert!(script.contains("pid 123"));
        assert!(script.contains("100"));
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn linux_tools_report_unsupported_off_linux() {
        let err = scanmem_version().unwrap_err();
        assert!(err.contains("Linux-only"));
    }

    #[test]
    fn tool_errors_are_ai_friendly() {
        let mut state = new_state();
        let res = call_tool(
            Some(json!(1)),
            json!({ "name": "scanmem_write_value", "arguments": { "pid": 123, "current_value": "100", "new_value": "999" } }),
            &mut state,
        );
        let text = res["result"]["content"][0]["text"].as_str().unwrap();
        assert!(res["result"]["isError"].as_bool().unwrap());
        assert!(text.contains("\"ok\":false"));
        assert!(text.contains("next_suggestion"));
    }

    #[test]
    fn rejects_weird_scan_value() {
        let err = scan_args(&json!({ "pid": 123, "value": "100;set 1" })).unwrap_err();
        assert!(err.contains("unsupported"));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn write_requires_confirmation() {
        let mut sessions = Sessions::new();
        let err = scanmem_write_value(&json!({ "pid": 123, "current_value": "100", "new_value": "999", "confirm_write": false }), &mut sessions).unwrap_err();
        assert!(err.contains("confirm_write"));
    }

    #[test]
    fn sessions_are_one_per_pid() {
        let mut sessions = Sessions::new();
        sessions.insert(123, new_session(123));
        touch_session_on(sessions.get_mut(&123).unwrap(), "first", "a");
        touch_session_on(sessions.get_mut(&123).unwrap(), "second", "b");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions.get(&123).unwrap().last_command, "second");
    }

    #[test]
    fn validates_value_types() {
        assert_eq!(
            valid_value_type(&json!({"value_type":"float"})).unwrap(),
            "float"
        );
        assert!(valid_value_type(&json!({"value_type":"bad"})).is_err());
        assert_eq!(typed_value("ff", "hex"), "0xff");
    }

    #[test]
    fn parses_rva_numbers() {
        assert_eq!(parse_u64("0xABCDEF").unwrap(), 0xabcdef);
        assert_eq!(parse_u64("9_999").unwrap(), 9999);
    }

    #[test]
    fn validates_gdb_hook_commands() {
        let args = json!({"commands": ["if $rdx > 0", "printf \"hit\\n\"", "set $rdx = 1", "end"]});
        assert_eq!(valid_gdb_commands(&args).unwrap().len(), 4);
        let err = valid_gdb_commands(&json!({"commands": ["shell rm -rf /tmp/nope"]})).unwrap_err();
        assert!(err.contains("unsupported"));
    }

    #[test]
    fn renders_gdb_hook_script() {
        let script = gdb_hook_script(123, 0xabc, &["printf \"hit\\n\"".into()]);
        assert!(script.contains("attach 123"));
        assert!(script.contains("break *0xabc"));
        assert!(script.contains("handle SIGPIPE nostop noprint pass"));
    }

    #[test]
    fn renders_gdb_hook_group_script() {
        let breakpoints = vec![
            GdbGroupBreakpoint {
                name: "one".into(),
                module: "m".into(),
                module_base: 0x1000,
                rva: 0x10,
                address: 0x1010,
                commands: vec!["printf \"one\\n\"".into()],
            },
            GdbGroupBreakpoint {
                name: "two".into(),
                module: "m".into(),
                module_base: 0x1000,
                rva: 0x20,
                address: 0x1020,
                commands: vec!["set $rax = 1".into()],
            },
        ];
        let script = gdb_hook_group_script(123, &breakpoints);
        assert_eq!(script.matches("attach 123").count(), 1);
        assert_eq!(script.matches("break *0x").count(), 2);
        assert!(script.contains("break *0x1010"));
        assert!(script.contains("break *0x1020"));
        assert!(script.contains("printf \"one\\n\""));
        assert!(script.contains("set $rax = 1"));
        assert!(script.ends_with("continue\n"));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn rejects_bad_gdb_hook_group_command() {
        let module = std::env::current_exe()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let err = match gdb_hook_group_spec(&json!({
            "pid": std::process::id(),
            "breakpoints": [{"module": module, "rva":"0x0", "commands":["shell id"]}]
        })) {
            Err(err) => err,
            Ok(_) => panic!("expected unsupported command"),
        };
        assert!(err.contains("unsupported"));
    }

    #[test]
    fn renders_gdb_phase16_scripts() {
        let disasm = gdb_disassemble_script(123, 0xabc, 8);
        assert!(disasm.contains("attach 123"));
        assert!(disasm.contains("x/8i 0xabc"));
        assert!(disasm.contains("detach"));
        let probe = gdb_probe_script(123, 0xabc, 3);
        assert!(probe.contains("break *0xabc"));
        assert!(probe.contains("probe_hit"));
        assert!(probe.contains("probe_done"));
        assert!(probe.contains("info registers rax rbx rcx rdx rsi rdi rbp rsp"));
        assert!(probe.contains("x/4gx $rsp"));
        assert!(probe.contains(">= 3"));
        assert!(probe.contains("detach"));
        assert!(probe.contains("quit"));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn reads_own_maps() {
        let pid = std::process::id() as u64;
        let maps = read_maps_entries(pid).unwrap();
        assert!(!maps.is_empty());
        assert!(maps.iter().any(|m| m.start < m.end));
        let res = process_read_maps(&json!({"pid": pid, "limit": 1})).unwrap();
        assert_eq!(res["ok"], true);
        assert_eq!(res["data"]["entries"].as_array().unwrap().len(), 1);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn reads_own_memory() {
        let pid = std::process::id() as u64;
        let marker = b"pony17\0";
        let address = hex(marker.as_ptr() as u64);
        let bytes =
            memory_read_bytes(&json!({"pid": pid, "address": address, "count": 6})).unwrap();
        assert_eq!(bytes["data"]["hex"], "706f6e793137");
        assert_eq!(bytes["data"]["ascii"], "pony17");
        let string =
            memory_read_string(&json!({"pid": pid, "address": address, "max_bytes": 16})).unwrap();
        assert_eq!(string["data"]["string"], "pony17");
        assert_eq!(bytes_hex(b"Hi"), "4869");
        assert_eq!(bytes_ascii(&[b'H', 0, b'i']), "H.i");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn memory_read_rejects_bad_targets() {
        let err = memory_read_bytes(&json!({"pid": 999999999_u64, "address": "0x1"})).unwrap_err();
        assert!(err.contains("pid is not running"));
        let err =
            memory_read_bytes(&json!({"pid": std::process::id(), "address": "0x0"})).unwrap_err();
        assert!(err.contains("not in any mapped region"));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn live_disassembly_checks_pid() {
        let err =
            gdb_disassemble_address(&json!({"pid": 999999999_u64, "address": "0x1"})).unwrap_err();
        assert!(err.contains("pid is not running"));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn gdb_start_requires_confirmation() {
        let mut hooks = Hooks::new();
        let err = gdb_hook_start(
            &json!({"pid": 123, "module": "x", "rva": "0x1", "commands": ["printf \"hit\\n\""]}),
            &mut hooks,
        )
        .unwrap_err();
        assert!(err.contains("confirm_hook"));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn gdb_hook_group_start_requires_confirmation() {
        let mut hooks = Hooks::new();
        let err = gdb_hook_group_start(
            &json!({"pid": 123, "breakpoints": [{"module": "x", "rva": "0x1", "commands": ["printf \"hit\\n\""]}]}),
            &mut hooks,
        )
        .unwrap_err();
        assert!(err.contains("confirm_hook"));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn gdb_probe_start_requires_confirmation() {
        let mut hooks = Hooks::new();
        let err = gdb_probe_start(
            &json!({"pid": 123, "module": "x", "rva": "0x1"}),
            &mut hooks,
        )
        .unwrap_err();
        assert!(err.contains("confirm_probe"));
    }

    #[test]
    fn artifact_root_is_local_only() {
        assert!(artifact_root(&json!({}), None)
            .unwrap()
            .starts_with(ARTIFACT_ROOT));
        assert!(artifact_root(&json!({"root":"/tmp"}), None).is_err());
        assert!(artifact_root(&json!({"root":"reverse/../x"}), None).is_err());
    }

    #[test]
    fn validates_workspace_names() {
        assert_eq!(checked_workspace_name("my-game").unwrap(), "my-game");
        assert!(checked_workspace_name("").is_err());
        assert!(checked_workspace_name("../x").is_err());
        assert!(checked_workspace_name("a/b").is_err());
    }

    #[test]
    fn resolves_workspace_artifact_roots() {
        assert_eq!(
            artifact_root(&json!({"root":"reverse/manual"}), Some("active")).unwrap(),
            PathBuf::from("reverse").join("manual")
        );
        assert_eq!(
            artifact_root(&json!({"workspace":"game-a"}), None).unwrap(),
            PathBuf::from("reverse").join("game-a").join("tools")
        );
        assert_eq!(
            artifact_root(&json!({"game":"game-b"}), None).unwrap(),
            PathBuf::from("reverse").join("game-b").join("tools")
        );
        assert_eq!(
            artifact_root(&json!({}), Some("game-c")).unwrap(),
            PathBuf::from("reverse").join("game-c").join("tools")
        );
    }

    #[test]
    fn parses_dump_metadata() {
        let meta = parse_method_meta("// RVA: 0x1234 Offset: 0x20 VA: 0x7ff");
        assert_eq!(meta.rva.as_deref(), Some("0x1234"));
        assert_eq!(
            parse_type_name("public class Hero : Unit").as_deref(),
            Some("Hero")
        );
        assert_eq!(
            parse_type_name("public interface IFoo // TypeDefIndex: 1").as_deref(),
            Some("IFoo")
        );
        let decl = parse_type_decl("public class Hero : Unit // TypeDefIndex: 2467").unwrap();
        assert_eq!(decl.kind, "class");
        assert_eq!(decl.base.as_deref(), Some("Unit"));
        assert_eq!(decl.type_def_index.as_deref(), Some("2467"));
        let field = parse_field_decl("public int health; // 0x20").unwrap();
        assert_eq!(field.field_type, "int");
        assert_eq!(field.field_name, "health");
        assert_eq!(field.offset, "0x20");
        assert!(same_rva("0xABC", "abc"));
        assert!(looks_like_method_decl("public void Hit(int a) { }"));
    }

    #[test]
    fn clips_long_artifact_preview() {
        let s = clip("abcdef", 3);
        assert_eq!(s, "abc…");
    }

    #[test]
    fn script_match_returns_selected_fields_only() {
        let m = script_match(
            "methods",
            0,
            &json!({"Name":"Foo", "Signature":"void Foo()", "Address":4660, "Other":"secret"}),
        );
        assert_eq!(m["name"], "Foo");
        assert!(m.get("Other").is_none());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn table_keeps_reverse_metadata_and_resolves_returned_path() {
        let name = format!("phase18-{}", std::process::id());
        let created = table_create(&json!({"game": name, "process": "proc"})).unwrap();
        let path = created["data"]["table"].as_str().unwrap().to_string();
        let module = std::env::current_exe()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();

        let added = table_add_entry(&json!({
            "table": path,
            "name": "health",
            "scan": "100",
            "value_type": "int32",
            "module": module,
            "rva": "0x0",
            "method_signature": "Hero::Health",
            "scan_query": "health"
        }))
        .unwrap();
        assert_eq!(added["data"]["data"]["entries"][0]["module"], module);

        let resolved =
            table_resolve_entries(&json!({"table": path, "pid": std::process::id()})).unwrap();
        assert_eq!(resolved["data"]["results"][0]["status"], "resolved");
        assert_eq!(
            resolved["data"]["data"]["entries"][0]["resolve_status"],
            "resolved"
        );

        let validated =
            table_validate_entries(&json!({"table": path, "pid": std::process::id()})).unwrap();
        assert_eq!(validated["data"]["results"][0]["valid"], true);
        fs::remove_file(path).ok();
    }

    #[test]
    fn reverse_reports_create_add_and_list_locally() {
        let workspace = format!("phase21-{}", std::process::id());
        let root = format!("reverse/{workspace}/tools");
        fs::create_dir_all(&root).unwrap();

        let created = reverse_report_create(
            &json!({"root": root, "report": "combat", "title": "Combat report", "summary": "safe summary"}),
            None,
        )
        .unwrap();
        let path = created["data"]["report"].as_str().unwrap().to_string();
        assert!(Path::new(&path).starts_with(ARTIFACT_ROOT));
        assert!(created["data"]["markdown"]
            .as_str()
            .unwrap()
            .ends_with(".md"));

        let added = reverse_report_add_finding(
            &json!({"root": root, "report": path, "title": "Health setter", "summary": "candidate", "module": "GameAssembly.dll", "rva": "0x1234", "notes": "verify before hook"}),
            None,
        )
        .unwrap();
        assert_eq!(added["data"]["finding_count"], 1);

        let listed = reverse_report_list(&json!({"root": root}), None).unwrap();
        assert_eq!(listed["data"]["reports"].as_array().unwrap().len(), 1);
        assert!(fs::read_to_string(path.replace(".json", ".md"))
            .unwrap()
            .contains("Health setter"));
        fs::remove_dir_all(format!("reverse/{workspace}")).ok();
    }
}
