//! Stdio transport tests against the bundled echo-mcp-server fixture.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use aegis_mcp_client::{Error, McpClient, StdioMcpClient};
use serde_json::json;

fn server_uri() -> String {
    // Cargo exposes the test fixture binary's path via this env var.
    let path = env!("CARGO_BIN_EXE_echo-mcp-server");
    format!("stdio:{path}")
}

#[test]
fn stdio_initialize_and_tools_call_round_trip() {
    let mut client = StdioMcpClient::new();
    let uri = server_uri();
    let result = client
        .call_tool(&uri, "echo", json!({"hello": "world"}))
        .unwrap();
    assert_eq!(result, json!({"echoed": {"hello": "world"}}));
}

#[test]
fn stdio_caches_connection_across_calls() {
    let mut client = StdioMcpClient::new();
    let uri = server_uri();
    let a = client.call_tool(&uri, "echo", json!({"i": 1})).unwrap();
    let b = client.call_tool(&uri, "echo", json!({"i": 2})).unwrap();
    assert_eq!(a, json!({"echoed": {"i": 1}}));
    assert_eq!(b, json!({"echoed": {"i": 2}}));
}

#[test]
fn stdio_propagates_server_side_error() {
    let mut client = StdioMcpClient::new();
    let uri = server_uri();
    let err = client
        .call_tool(&uri, "fail", json!({}))
        .expect_err("expected server error");
    match err {
        Error::ServerError { code, message } => {
            assert_eq!(code, -32000);
            assert!(message.contains("deliberate failure"));
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[test]
fn stdio_rejects_non_stdio_uri() {
    let mut client = StdioMcpClient::new();
    let err = client
        .call_tool("https://mcp.example.com:8443", "echo", json!({}))
        .expect_err("expected unsupported transport");
    assert!(matches!(err, Error::UnsupportedTransport { .. }));
}
