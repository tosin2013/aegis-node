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
//!   - `echo_with_progress`: emits a server-to-client
//!     `notifications/message` (no id, level=info) BEFORE the
//!     `tools/call` response. Models the FastMCP / firecrawl-mcp
//!     progress-reporting pattern so the client can prove it skips
//!     notifications and matches responses by id.

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
        let frames = handle(method, &params, &id);
        let mut quit = false;
        for frame in frames {
            let mut bytes = serde_json::to_vec(&frame).unwrap_or_default();
            bytes.push(b'\n');
            if out.write_all(&bytes).is_err() {
                quit = true;
                break;
            }
            if out.flush().is_err() {
                quit = true;
                break;
            }
        }
        if quit {
            break;
        }
    }
}

fn progress_notification(message: &str, query: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "notifications/message",
        "params": {
            "level": "info",
            "data": {
                "message": message,
                "context": query,
            },
        },
    })
}

fn handle(method: &str, params: &Value, id: &Value) -> Vec<Value> {
    match method {
        "initialize" => vec![json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "echo-mcp-server", "version": "0.0.1" },
            },
        })],
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or_default();
            let args = params.get("arguments").cloned().unwrap_or(Value::Null);
            match name {
                "echo" => vec![json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "echoed": args },
                })],
                "fail" => vec![json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32000,
                        "message": "deliberate failure for testing",
                    },
                })],
                "echo_with_progress" => vec![
                    // Server-to-client notification BEFORE the response
                    // — no id, no result, no error. Mimics firecrawl-mcp's
                    // "Searching" notification. A correct client must
                    // skip this and keep reading until a frame whose id
                    // matches the request arrives.
                    progress_notification("Working", &args),
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": { "echoed": args },
                    }),
                ],
                other => vec![json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": format!("unknown tool: {other}"),
                    },
                })],
            }
        }
        _ => vec![json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": format!("unknown method: {method}"),
            },
        })],
    }
}
