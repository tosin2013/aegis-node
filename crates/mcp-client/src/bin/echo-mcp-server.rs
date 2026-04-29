//! Tiny MCP test fixture server: speaks JSON-RPC 2.0 over stdio,
//! responds to `initialize` and `tools/call` with deterministic
//! payloads, and exits cleanly when stdin reaches EOF.
//!
//! Used by `crates/mcp-client/tests/integration.rs` to exercise the
//! real stdio transport without depending on an external binary.
//! Cargo exposes this binary's path to integration tests via
//! `env!("CARGO_BIN_EXE_echo-mcp-server")`.
//!
//! Tools surfaced:
//!   - `echo`: returns `{ "echoed": <args> }`.
//!   - `fail`: returns a JSON-RPC error with code -32000.

use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        // Notifications have no `id`; ignore them silently (per JSON-RPC).
        let id = match msg.get("id") {
            Some(v) if !v.is_null() => v.clone(),
            _ => continue,
        };
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let response = handle(method, &params, &id);
        let mut bytes = serde_json::to_vec(&response).unwrap_or_default();
        bytes.push(b'\n');
        if out.write_all(&bytes).is_err() {
            break;
        }
        if out.flush().is_err() {
            break;
        }
    }
}

fn handle(method: &str, params: &Value, id: &Value) -> Value {
    match method {
        "initialize" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "echo-mcp-server", "version": "0.0.1" },
            },
        }),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or_default();
            let args = params.get("arguments").cloned().unwrap_or(Value::Null);
            match name {
                "echo" => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "echoed": args },
                }),
                "fail" => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32000,
                        "message": "deliberate failure for testing",
                    },
                }),
                other => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": format!("unknown tool: {other}"),
                    },
                }),
            }
        }
        _ => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": format!("unknown method: {method}"),
            },
        }),
    }
}
