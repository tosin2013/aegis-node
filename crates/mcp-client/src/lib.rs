//! MCP client used by the Aegis-Node runtime mediator (per ADR-018,
//! F2-MCP-B / issue #44).
//!
//! Aegis-Node is an MCP **client**: it consumes external MCP servers'
//! tool catalogs and invokes their tools. This crate wraps the
//! [Model Context Protocol](https://modelcontextprotocol.io/) wire
//! format. Phase 1 (this crate) ships stdio transport over JSON-RPC 2.0
//! with newline-delimited messages — enough to spawn `mcp-server-foo`
//! as a child process and call its tools. HTTP/SSE transport, tool
//! result streaming, and MCP server attestation are out of scope for
//! Phase 1 (see ADR-018 §Out of scope).
//!
//! ## Design
//!
//! [`McpClient`] is the trait the mediator depends on. The concrete
//! [`StdioMcpClient`] implementation spawns a child process per
//! `server_uri` (cached across calls so the initialize handshake runs
//! once per server) and exchanges JSON-RPC messages over its stdio.
//!
//! Tests in the runtime use a mock client that implements [`McpClient`]
//! directly without spawning anything; this crate's own tests exercise
//! the real stdio transport against a tiny test fixture binary
//! (`tests/fixtures/echo-mcp-server.rs`).

#![allow(clippy::result_large_err)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub mod error;
pub use error::{Error, Result};

/// Aegis-Node's MCP client surface. Only `call_tool` is needed by the
/// mediator. Implementations may cache connections, run an initialize
/// handshake on first contact, and dispatch by `server_uri`.
///
/// Implementors must be `Send` so the mediator can hold one inside a
/// `Box<dyn McpClient>` field on `Session`.
pub trait McpClient: Send {
    /// Invoke a single MCP tool and return its `result` JSON object as
    /// the server returned it. The caller decides what (if anything) to
    /// do with the structured content; the mediator's job is to log the
    /// invocation, not to interpret the payload.
    ///
    /// `server_uri` is the value the manifest's `tools.mcp[].server_uri`
    /// field carries — typically `stdio:/path/to/server-binary` for
    /// Phase 1 stdio transport. `tool_name` is one of the tools listed
    /// in the same entry's `allowed_tools`. Argument validation against
    /// the upstream tool's `input_schema` is the upstream server's job;
    /// this client passes `args` through verbatim.
    fn call_tool(&mut self, server_uri: &str, tool_name: &str, args: Value) -> Result<Value>;
}

/// JSON-RPC 2.0 request frame. Held in the public surface so test
/// fixtures and mock implementations can build the same shape the
/// real transport uses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    pub params: Value,
}

/// JSON-RPC 2.0 response frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

/// `stdio:` transport. Spawns the named binary as a child process,
/// runs the MCP `initialize` handshake on first use of each
/// `server_uri`, then dispatches `tools/call` invocations on demand.
///
/// One [`StdioMcpClient`] instance can handle multiple distinct
/// `server_uri`s — each is cached by URI so the initialize handshake
/// runs once per server per session. Children are killed on Drop.
pub struct StdioMcpClient {
    /// Cached connections keyed by full `server_uri` (e.g.
    /// `"stdio:/usr/local/bin/mcp-fs"`). Each entry owns the child
    /// process plus its stdin/stdout handles and a monotonic id counter.
    connections: HashMap<String, StdioConnection>,
}

struct StdioConnection {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Default for StdioMcpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl StdioMcpClient {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
        }
    }

    fn ensure_connection(&mut self, server_uri: &str) -> Result<&mut StdioConnection> {
        if !self.connections.contains_key(server_uri) {
            let path = parse_stdio_uri(server_uri)?;
            let mut child = Command::new(path)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .map_err(|e| Error::Spawn {
                    server_uri: server_uri.to_string(),
                    source: e,
                })?;
            let stdin = child
                .stdin
                .take()
                .ok_or_else(|| Error::Protocol("child stdin missing".to_string()))?;
            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| Error::Protocol("child stdout missing".to_string()))?;
            let mut conn = StdioConnection {
                child,
                stdin,
                stdout: BufReader::new(stdout),
                next_id: 0,
            };
            initialize_handshake(&mut conn)?;
            self.connections.insert(server_uri.to_string(), conn);
        }
        // The contains_key check above guarantees the entry exists; the
        // get_mut below cannot fail. Fall back to a Protocol error rather
        // than expect()/unwrap() so clippy::expect_used stays clean.
        self.connections
            .get_mut(server_uri)
            .ok_or_else(|| Error::Protocol("connection map race".to_string()))
    }
}

impl McpClient for StdioMcpClient {
    fn call_tool(&mut self, server_uri: &str, tool_name: &str, args: Value) -> Result<Value> {
        let conn = self.ensure_connection(server_uri)?;
        let id = conn.next_id;
        conn.next_id += 1;
        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: "tools/call".to_string(),
            params: json!({
                "name": tool_name,
                "arguments": args,
            }),
        };
        write_message(&mut conn.stdin, &req)?;
        let resp: JsonRpcResponse = read_message(&mut conn.stdout)?;
        if let Some(err) = resp.error {
            return Err(Error::ServerError {
                code: err.code,
                message: err.message,
            });
        }
        resp.result
            .ok_or_else(|| Error::Protocol("response missing both result and error".to_string()))
    }
}

impl Drop for StdioMcpClient {
    fn drop(&mut self) {
        for (_, mut conn) in self.connections.drain() {
            // Best-effort kill; the child may already have exited
            // (e.g. if it noticed stdin EOF). Errors here are not
            // actionable.
            let _ = conn.child.kill();
            let _ = conn.child.wait();
        }
    }
}

fn parse_stdio_uri(server_uri: &str) -> Result<&str> {
    server_uri
        .strip_prefix("stdio:")
        .ok_or_else(|| Error::UnsupportedTransport {
            server_uri: server_uri.to_string(),
        })
}

/// Run the MCP initialize handshake. Aegis-Node advertises the minimum
/// capabilities Phase 1 needs (tool calls; no resources/prompts) and
/// expects the server to respond with its own capability set. We don't
/// inspect the server's capabilities here — the manifest's
/// `allowed_tools` is the source of truth for what the agent may invoke.
fn initialize_handshake(conn: &mut StdioConnection) -> Result<()> {
    let id = conn.next_id;
    conn.next_id += 1;
    let req = JsonRpcRequest {
        jsonrpc: "2.0",
        id,
        method: "initialize".to_string(),
        params: json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {},
            },
            "clientInfo": {
                "name": "aegis-node",
                "version": env!("CARGO_PKG_VERSION"),
            },
        }),
    };
    write_message(&mut conn.stdin, &req)?;
    let resp: JsonRpcResponse = read_message(&mut conn.stdout)?;
    if let Some(err) = resp.error {
        return Err(Error::ServerError {
            code: err.code,
            message: err.message,
        });
    }
    // Per spec: send `notifications/initialized` after a successful
    // initialize response. No id (it's a notification, not a request).
    let init_done = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized",
    });
    write_value(&mut conn.stdin, &init_done)?;
    Ok(())
}

fn write_message<T: Serialize>(stdin: &mut ChildStdin, msg: &T) -> Result<()> {
    write_value(stdin, msg)
}

fn write_value<T: Serialize>(stdin: &mut ChildStdin, msg: &T) -> Result<()> {
    let mut bytes = serde_json::to_vec(msg).map_err(|e| Error::Protocol(e.to_string()))?;
    bytes.push(b'\n');
    stdin
        .write_all(&bytes)
        .map_err(|e| Error::Protocol(format!("write: {e}")))?;
    stdin
        .flush()
        .map_err(|e| Error::Protocol(format!("flush: {e}")))?;
    Ok(())
}

fn read_message<T: for<'de> Deserialize<'de>>(stdout: &mut BufReader<ChildStdout>) -> Result<T> {
    let mut line = String::new();
    let n = stdout
        .read_line(&mut line)
        .map_err(|e| Error::Protocol(format!("read: {e}")))?;
    if n == 0 {
        return Err(Error::Protocol(
            "server closed stdout before reply".to_string(),
        ));
    }
    serde_json::from_str(line.trim_end_matches('\n'))
        .map_err(|e| Error::Protocol(format!("decode: {e}; raw: {line:?}")))
}
