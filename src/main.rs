use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SESSION_TIMEOUT_SECS: u64 = 30 * 60;

#[derive(Deserialize)]
struct Request {
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Clone)]
struct Session {
    pid: u64,
    created_at: u64,
    last_seen: u64,
    last_command: String,
    last_output: String,
    last_match_count: usize,
    frozen_value: Option<String>,
}

type Sessions = HashMap<u64, Session>;

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut sessions = Sessions::new();

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<Request>(&line) {
            Ok(req) => handle(req, &mut sessions),
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

fn handle(req: Request, sessions: &mut Sessions) -> Value {
    if req.jsonrpc.as_deref() != Some("2.0") {
        return error(req.id, -32600, "jsonrpc must be 2.0");
    }

    expire_sessions(sessions);
    match req.method.as_str() {
        "initialize" => ok(
            req.id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "cheat-engine-mcp", "version": "0.1.0" }
            }),
        ),
        "tools/list" => ok(req.id, json!({ "tools": tools() })),
        "tools/call" => call_tool(req.id, req.params, sessions),
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
        { "name": "table_create", "description": "Create a cheat table JSON profile in .cheat-tables.", "inputSchema": { "type": "object", "properties": { "game": { "type": "string" }, "process": { "type": "string" }, "notes": { "type": "string" } }, "required": ["game", "process"] } },
        { "name": "table_add_entry", "description": "Add a named entry to a cheat table.", "inputSchema": { "type": "object", "properties": { "table": { "type": "string" }, "name": { "type": "string" }, "scan": { "type": "string" }, "value_type": { "type": "string" }, "last_value": { "type": "string" }, "notes": { "type": "string" } }, "required": ["table", "name", "scan", "value_type"] } },
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

fn call_tool(id: Option<Value>, params: Value, sessions: &mut Sessions) -> Value {
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
        "scanmem_exact_scan" | "scanmem_scan_exact" => scanmem_exact_scan(&args, sessions),
        "scanmem_write_value" => scanmem_write_value(&args),
        "scanmem_attach_process" => scanmem_attach_process(&args, sessions),
        "scanmem_reset_scan" => {
            scanmem_simple_command(&args, "reset", "Reset scan completed.", sessions)
        }
        "scanmem_scan_increased" => {
            scanmem_refine_scan(&args, "+", "Increased scan completed.", sessions)
        }
        "scanmem_scan_decreased" => {
            scanmem_refine_scan(&args, "-", "Decreased scan completed.", sessions)
        }
        "scanmem_scan_changed" => {
            scanmem_refine_scan(&args, "changed", "Changed scan completed.", sessions)
        }
        "scanmem_scan_unchanged" => {
            scanmem_refine_scan(&args, "unchanged", "Unchanged scan completed.", sessions)
        }
        "scanmem_list_matches" => {
            scanmem_simple_command(&args, "list", "List matches completed.", sessions)
        }
        "scanmem_pick_match" => scanmem_pick_match(&args),
        "session_create" => session_create(&args, sessions),
        "session_status" => session_status(&args, sessions),
        "session_close" => session_close(&args, sessions),
        "scanmem_preview_write" => scanmem_preview_write(&args, sessions),
        "scanmem_write_selected" => scanmem_write_selected(&args, sessions),
        "scanmem_freeze_value" => scanmem_freeze_value(&args, sessions),
        "scanmem_unfreeze_value" => scanmem_unfreeze_value(&args, sessions),
        "scanmem_scan_by_type" => scanmem_scan_by_type(&args, sessions),
        "scanmem_scan_range" => scanmem_scan_range(&args, sessions),
        "scanmem_scan_unknown" => scanmem_scan_unknown(&args, sessions),
        "process_search" => process_search(&args),
        "process_info" => process_info(&args),
        "process_suggest_target" => process_suggest_target(&args),
        "table_create" => table_create(&args),
        "table_add_entry" => table_add_entry(&args),
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
        json!({ "script": format!("# preview only\npid {pid}\n{value}\n# refine with next values, then use set only when you are sure") }),
        Some("Run scanmem_exact_scan when ready."),
    ))
}

fn scanmem_exact_scan(args: &Value, sessions: &mut Sessions) -> Result<Value, String> {
    let (pid, value) = scan_args(args)?;
    let output = run_scanmem_script(&format!("pid {pid}\n{value}\nexit\n"))?;
    touch_session(sessions, pid, value.clone(), output.clone());
    Ok(tool_ok(
        "Exact scan completed.",
        json!({ "pid": pid, "value": value, "output": output }),
        Some("Change the value in the target app, then run a refine scan."),
    ))
}

fn scanmem_write_value(args: &Value) -> Result<Value, String> {
    if args.get("confirm_write").and_then(Value::as_bool) != Some(true) {
        return Err("confirm_write must be true because this changes process memory".to_string());
    }
    let pid = valid_live_pid(args)?;
    let current_value = valid_value_arg(args, "current_value")?;
    let new_value = valid_value_arg(args, "new_value")?;
    let output = run_scanmem_script(&format!(
        "pid {pid}\n{current_value}\nset {new_value}\nexit\n"
    ))?;
    Ok(tool_ok(
        "Write command completed.",
        json!({ "pid": pid, "current_value": current_value, "new_value": new_value, "output": output }),
        Some("Verify the target value changed."),
    ))
}

fn scanmem_preview_write(args: &Value, sessions: &mut Sessions) -> Result<Value, String> {
    let pid = valid_live_pid(args)?;
    let current_value = valid_value_arg(args, "current_value")?;
    let new_value = valid_value_arg(args, "new_value")?;
    let max_writes = args.get("max_writes").and_then(Value::as_u64).unwrap_or(1);
    let output = run_scanmem_script(&format!("pid {pid}\n{current_value}\nlist\nexit\n"))?;
    let match_count = count_matches(&output);
    touch_session(sessions, pid, "preview_write".to_string(), output.clone());
    Ok(tool_ok(
        "Write preview completed.",
        json!({ "pid": pid, "current_value": current_value, "backup_old_value": current_value, "new_value": new_value, "match_count": match_count, "max_writes": max_writes, "allowed": match_count > 0 && match_count as u64 <= max_writes, "dry_run": true, "output": output }),
        Some("If allowed is true, run scanmem_write_selected with confirm_write=true."),
    ))
}

fn scanmem_write_selected(args: &Value, sessions: &mut Sessions) -> Result<Value, String> {
    let (pid, current_value, new_value, match_count, preview) = guarded_write_inputs(args)?;
    if args.get("dry_run").and_then(Value::as_bool) == Some(true) {
        return Ok(tool_ok(
            "Dry run only; no memory changed.",
            json!({ "pid": pid, "match_count": match_count, "backup_old_value": current_value, "preview": preview }),
            Some("Set dry_run=false or omit it to write."),
        ));
    }
    let output = run_scanmem_script(&format!(
        "pid {pid}\n{current_value}\nset {new_value}\nexit\n"
    ))?;
    touch_session(sessions, pid, "write_selected".to_string(), output.clone());
    Ok(tool_ok(
        "Selected write completed.",
        json!({ "pid": pid, "current_value": current_value, "backup_old_value": current_value, "new_value": new_value, "match_count": match_count, "output": output }),
        Some("Verify the target value changed."),
    ))
}

fn scanmem_freeze_value(args: &Value, sessions: &mut Sessions) -> Result<Value, String> {
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
    let pid = valid_pid(args)?;
    let value = valid_value_arg(args, "value")?;
    let value_type = valid_value_type(args)?;
    let scan_value = typed_value(&value, &value_type);
    let output = run_scanmem_script(&format!("pid {pid}\n{scan_value}\nexit\n"))?;
    touch_session(
        sessions,
        pid,
        format!("scan_by_type:{value_type}"),
        output.clone(),
    );
    Ok(tool_ok(
        "Typed scan completed.",
        json!({ "pid": pid, "value": value, "value_type": value_type, "scan_value": scan_value, "output": output }),
        Some("Refine or list matches next."),
    ))
}

fn scanmem_scan_range(args: &Value, sessions: &mut Sessions) -> Result<Value, String> {
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
    let output = run_scanmem_script(&format!("pid {pid}\n{scan_value}\nexit\n"))?;
    touch_session(
        sessions,
        pid,
        format!("scan_range:{value_type}"),
        output.clone(),
    );
    Ok(tool_ok(
        "Range scan completed.",
        json!({ "pid": pid, "min": min, "max": max, "value_type": value_type, "scan_value": scan_value, "output": output }),
        Some("Use scanmem_scan_increased/decreased/changed to refine."),
    ))
}

fn scanmem_scan_unknown(args: &Value, sessions: &mut Sessions) -> Result<Value, String> {
    let pid = valid_pid(args)?;
    let value_type = args
        .get("value_type")
        .and_then(Value::as_str)
        .unwrap_or("auto")
        .to_string();
    if value_type != "auto" {
        valid_value_type(args)?;
    }
    let output = run_scanmem_script(&format!("pid {pid}\nunknown\nexit\n"))?;
    touch_session(
        sessions,
        pid,
        format!("scan_unknown:{value_type}"),
        output.clone(),
    );
    Ok(tool_ok(
        "Unknown initial value scan completed.",
        json!({ "pid": pid, "value_type": value_type, "output": output }),
        Some("Change the target value, then run increased/decreased/changed scan."),
    ))
}

fn process_search(args: &Value) -> Result<Value, String> {
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
    let entry = json!({"name": safe_name(args,"name")?, "scan": required_str(args,"scan")?, "value_type": valid_value_type(args)?, "last_value": args.get("last_value").and_then(Value::as_str).unwrap_or(""), "notes": args.get("notes").and_then(Value::as_str).unwrap_or("")});
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
fn table_path(name: &str) -> Result<PathBuf, String> {
    let file = name.trim().trim_end_matches(".json");
    if !file
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "._-/".contains(c))
        || file.contains("..")
    {
        return Err("invalid table path".into());
    }
    let mut p = PathBuf::from(".cheat-tables");
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    p.push(format!("{}.json", file.replace('/', "_")));
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

fn guarded_write_inputs(args: &Value) -> Result<(u64, String, String, usize, String), String> {
    if args.get("confirm_write").and_then(Value::as_bool) != Some(true) {
        return Err("confirm_write must be true because this changes process memory".to_string());
    }
    let pid = valid_live_pid(args)?;
    let current_value = valid_value_arg(args, "current_value")?;
    let new_value = valid_value_arg(args, "new_value")?;
    let max_writes = args.get("max_writes").and_then(Value::as_u64).unwrap_or(1);
    let preview = run_scanmem_script(&format!("pid {pid}\n{current_value}\nlist\nexit\n"))?;
    let match_count = count_matches(&preview);
    if match_count == 0 {
        return Err("no scan matches found; write blocked".to_string());
    }
    if match_count as u64 > max_writes {
        return Err(format!(
            "{match_count} matches exceed max_writes={max_writes}; write blocked"
        ));
    }
    Ok((pid, current_value, new_value, match_count, preview))
}

fn scanmem_attach_process(args: &Value, sessions: &mut Sessions) -> Result<Value, String> {
    let pid = valid_pid(args)?;
    let output = run_scanmem_script(&format!("pid {pid}\nexit\n"))?;
    touch_session(sessions, pid, "attach".to_string(), output.clone());
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
    let mut script = format!("pid {pid}\n");
    if let Some(value) = prefix {
        script.push_str(&format!("{value}\n"));
    }
    script.push_str(command);
    script.push_str("\nexit\n");
    let output = run_scanmem_script(&script)?;
    touch_session(sessions, pid, command.to_string(), output.clone());
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
    let initial_value = valid_value_arg(args, "initial_value")?;
    let output = run_scanmem_script(&format!("pid {pid}\n{initial_value}\n{command}\nexit\n"))?;
    touch_session(sessions, pid, command.to_string(), output.clone());
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
    let pid = valid_pid(args)?;
    if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
        return Err("pid is not running".to_string());
    }
    let session = new_session(pid);
    sessions.insert(pid, session.clone());
    Ok(tool_ok(
        "Session created.",
        session_json(&session),
        Some("Run scanmem_scan_exact for this PID."),
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
    }
}

fn session_json(session: &Session) -> Value {
    json!({ "pid": session.pid, "created_at": session.created_at, "last_seen": session.last_seen, "last_command": session.last_command, "last_output": session.last_output, "last_match_count": session.last_match_count, "frozen_value": session.frozen_value, "timeout_secs": SESSION_TIMEOUT_SECS })
}

fn touch_session(sessions: &mut Sessions, pid: u64, command: String, output: String) {
    let now = now_secs();
    let match_count = count_matches(&output);
    let entry = sessions.entry(pid).or_insert(new_session(pid));
    entry.last_seen = now;
    entry.last_command = command;
    entry.last_match_count = match_count;
    entry.last_output = output;
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

fn run_scanmem_script(script: &str) -> Result<String, String> {
    let mut child = Command::new("scanmem")
        .arg("--command")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("failed to run scanmem: {err}"))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| "failed to open scanmem stdin".to_string())?
        .write_all(script.as_bytes())
        .map_err(|err| format!("failed to write scanmem script: {err}"))?;
    let out = child
        .wait_with_output()
        .map_err(|err| format!("failed to read scanmem output: {err}"))?;
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    if !out.stderr.is_empty() {
        text.push('\n');
        text.push_str(&String::from_utf8_lossy(&out.stderr));
    }
    let text = text.trim();
    let text = if text.len() > 8000 {
        &text[..8000]
    } else {
        text
    };
    if out.status.success() {
        Ok(text.to_string())
    } else {
        Err(text.to_string())
    }
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
        let mut sessions = Sessions::new();
        let res = handle(
            Request {
                jsonrpc: Some("2.0".into()),
                id: Some(json!(1)),
                method: "tools/list".into(),
                params: json!({}),
            },
            &mut sessions,
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
        assert!(tools.iter().any(|tool| tool["name"] == "table_create"));
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
    fn tool_errors_are_ai_friendly() {
        let mut sessions = Sessions::new();
        let res = call_tool(
            Some(json!(1)),
            json!({ "name": "scanmem_write_value", "arguments": { "pid": 123, "current_value": "100", "new_value": "999" } }),
            &mut sessions,
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
    fn write_requires_confirmation() {
        let err = scanmem_write_value(&json!({ "pid": 123, "current_value": "100", "new_value": "999", "confirm_write": false })).unwrap_err();
        assert!(err.contains("confirm_write"));
    }

    #[test]
    fn sessions_are_one_per_pid() {
        let mut sessions = Sessions::new();
        touch_session(&mut sessions, 123, "first".to_string(), "a".to_string());
        touch_session(&mut sessions, 123, "second".to_string(), "b".to_string());
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
}
