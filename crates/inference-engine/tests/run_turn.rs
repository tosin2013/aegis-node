//! Integration tests for `Session::run_turn` (LLM-B / issue #71).
//!
//! Uses a [`MockLoadedModel`] that returns a canned [`InferResponse`]
//! so the test asserts the F5 reasoning emission + MCP tool dispatch
//! flow without needing a real GGUF or libclang at build time.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::sync::{Arc, Mutex};

use aegis_identity::LocalCa;
use aegis_inference_engine::{
    BackendError, BootConfig, InferRequest, InferResponse, LoadedModel, Session, ToolCall,
    ToolCallResult,
};
use aegis_mcp_client::{McpClient, Result as McpResult};
use serde_json::Value;

const TRUST_DOMAIN: &str = "session-boot.local";

/// Test double for [`LoadedModel`]. Records every `infer` call's
/// request and returns the next canned response. Tests pre-load the
/// queue with the responses they want.
struct MockLoadedModel {
    queue: Arc<Mutex<Vec<Result<InferResponse, BackendError>>>>,
    captured: Arc<Mutex<Vec<InferRequest>>>,
}

impl MockLoadedModel {
    fn new(responses: Vec<Result<InferResponse, BackendError>>) -> (Self, MockHandle) {
        let queue = Arc::new(Mutex::new(responses));
        let captured = Arc::new(Mutex::new(Vec::new()));
        let handle = MockHandle {
            captured: captured.clone(),
        };
        (Self { queue, captured }, handle)
    }
}

impl LoadedModel for MockLoadedModel {
    fn infer(&mut self, request: InferRequest) -> Result<InferResponse, BackendError> {
        self.captured.lock().unwrap().push(request);
        let mut q = self.queue.lock().unwrap();
        q.remove(0)
    }
}

#[derive(Clone)]
struct MockHandle {
    captured: Arc<Mutex<Vec<InferRequest>>>,
}

impl MockHandle {
    fn captured_requests(&self) -> Vec<InferRequest> {
        self.captured.lock().unwrap().clone()
    }
}

/// Test double for [`McpClient`]. Returns a canned JSON value for
/// every `call_tool` invocation and records the (server, tool, args)
/// tuple so the test can assert dispatch happened correctly.
struct MockMcpClient {
    response: serde_json::Value,
    calls: Arc<Mutex<Vec<(String, String, serde_json::Value)>>>,
}

impl McpClient for MockMcpClient {
    fn call_tool(
        &mut self,
        server_uri: &str,
        tool_name: &str,
        args: serde_json::Value,
    ) -> McpResult<serde_json::Value> {
        self.calls
            .lock()
            .unwrap()
            .push((server_uri.to_string(), tool_name.to_string(), args));
        Ok(self.response.clone())
    }
}

fn write_manifest_with_one_mcp_server(path: &Path) {
    let yaml = r#"schemaVersion: "1"
agent: { name: "run-turn-test", version: "1.0.0" }
identity: { spiffeId: "spiffe://session-boot.local/agent/research/inst-001" }
tools:
  mcp:
    - server_name: "weather-mcp"
      server_uri: "stdio:///bin/weather-mcp"
      allowed_tools: ["get"]
"#;
    std::fs::write(path, yaml).unwrap();
}

fn boot_session(dir: &Path, ca_dir: &Path) -> Session {
    LocalCa::init(ca_dir, TRUST_DOMAIN).unwrap();

    let manifest_path = dir.join("manifest.yaml");
    let model_path = dir.join("model.gguf");
    let ledger_path = dir.join("ledger.jsonl");
    write_manifest_with_one_mcp_server(&manifest_path);
    std::fs::write(&model_path, b"fake-model-bytes-for-test").unwrap();

    let cfg = BootConfig {
        session_id: "session-run-turn".to_string(),
        manifest_path,
        model_path,
        config_path: None,
        chat_template_sidecar: None,
        identity_dir: ca_dir.to_path_buf(),
        workload_name: "research".to_string(),
        instance: "inst-001".to_string(),
        ledger_path,
    };
    Session::boot(cfg).unwrap()
}

#[test]
fn run_turn_without_loaded_model_returns_typed_error() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let mut session = boot_session(dir.path(), ca_dir.path());

    let err = session.run_turn("hello").unwrap_err();
    assert!(
        matches!(err, aegis_inference_engine::Error::NoBackendConfigured),
        "{err:?}"
    );
}

#[test]
fn run_turn_emits_reasoning_then_dispatches_one_mcp_tool_call() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let session = boot_session(dir.path(), ca_dir.path());

    // Canned response: one tool call with no preamble — model goes
    // straight to the call.
    let response = InferResponse {
        reasoning: "Looking up the weather.".to_string(),
        tool_calls: vec![ToolCall {
            name: "weather-mcp__get".to_string(),
            arguments: serde_json::json!({"city": "Paris"}),
        }],
        assistant_text: Some("Looking up the weather.".to_string()),
    };
    let (mock, mock_handle) = MockLoadedModel::new(vec![Ok(response)]);

    let mcp_calls = Arc::new(Mutex::new(Vec::new()));
    let mcp_client = MockMcpClient {
        response: serde_json::json!({"temp_c": 18}),
        calls: mcp_calls.clone(),
    };

    let mut session = session
        .with_mcp_client(Box::new(mcp_client))
        .with_loaded_model(Box::new(mock));

    let outcome = session.run_turn("What's the weather in Paris?").unwrap();

    // The driver dispatched the tool call through the MCP client.
    assert_eq!(outcome.tool_calls.len(), 1);
    assert_eq!(outcome.tool_calls[0].name, "weather-mcp__get");
    match &outcome.tool_calls[0].result {
        ToolCallResult::Success(v) => {
            assert_eq!(v["temp_c"], 18);
        }
        other => panic!("expected Success, got {other:?}"),
    }

    // The MCP client saw the dispatch with the right (server-uri, tool, args).
    let calls = mcp_calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "stdio:///bin/weather-mcp");
    assert_eq!(calls[0].1, "get");
    assert_eq!(calls[0].2, serde_json::json!({"city": "Paris"}));

    // The InferRequest the mock saw was built from the user message
    // and the manifest's MCP catalog (one tool: weather-mcp__get).
    let captured = mock_handle.captured_requests();
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].messages.len(), 1);
    assert_eq!(
        captured[0].messages[0].content,
        "What's the weather in Paris?"
    );
    assert_eq!(captured[0].tools.len(), 1);
    assert_eq!(captured[0].tools[0].name, "weather-mcp__get");

    // The ledger captured exactly the F5+F4 pair the issue's
    // acceptance criterion calls for: ReasoningStep then Access.
    let _ = session.shutdown().unwrap();
    let ledger = std::fs::read_to_string(dir.path().join("ledger.jsonl")).unwrap();
    let lines: Vec<Value> = ledger
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let entry_types: Vec<&str> = lines
        .iter()
        .map(|v| v["entryType"].as_str().unwrap())
        .collect();
    assert!(
        entry_types.contains(&"reasoning_step"),
        "missing reasoning_step in {entry_types:?}"
    );
    assert!(
        entry_types.contains(&"access"),
        "missing access entry in {entry_types:?}"
    );
    let pos_reasoning = entry_types
        .iter()
        .position(|t| *t == "reasoning_step")
        .unwrap();
    let pos_access = entry_types.iter().position(|t| *t == "access").unwrap();
    assert!(
        pos_reasoning < pos_access,
        "reasoning_step should precede access in {entry_types:?}"
    );

    // The reasoning step's tool_selected should be the qualified name
    // the model emitted — auditors trace through it.
    let reasoning_entry = &lines[pos_reasoning];
    assert_eq!(
        reasoning_entry["toolSelected"].as_str(),
        Some("weather-mcp__get")
    );
}

#[test]
fn run_turn_marks_unroutable_tool_call_without_propagating_error() {
    // Model emits a tool name not in the <server>__<tool> shape.
    // Driver records it as Unroutable; the turn still succeeds so
    // the next turn can show the model the problem.
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let session = boot_session(dir.path(), ca_dir.path());

    let response = InferResponse {
        reasoning: String::new(),
        tool_calls: vec![ToolCall {
            name: "just_a_tool_with_no_server".to_string(),
            arguments: serde_json::Value::Null,
        }],
        assistant_text: None,
    };
    let (mock, _h) = MockLoadedModel::new(vec![Ok(response)]);
    let mut session = session.with_loaded_model(Box::new(mock));

    let outcome = session.run_turn("hi").unwrap();
    assert_eq!(outcome.tool_calls.len(), 1);
    assert!(
        matches!(&outcome.tool_calls[0].result, ToolCallResult::Unroutable(_)),
        "{:?}",
        outcome.tool_calls[0].result
    );
}

#[test]
fn run_turn_records_denied_tool_call_without_short_circuiting() {
    // Model picks a tool the manifest doesn't allow — policy denies,
    // ledger gets a Violation, driver records Denied and returns
    // normally.
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let session = boot_session(dir.path(), ca_dir.path());

    let response = InferResponse {
        reasoning: String::new(),
        tool_calls: vec![ToolCall {
            // weather-mcp__delete is not in allowed_tools.
            name: "weather-mcp__delete".to_string(),
            arguments: serde_json::Value::Null,
        }],
        assistant_text: None,
    };
    let (mock, _h) = MockLoadedModel::new(vec![Ok(response)]);
    let mut session = session.with_loaded_model(Box::new(mock));

    let outcome = session.run_turn("delete it").unwrap();
    assert_eq!(outcome.tool_calls.len(), 1);
    match &outcome.tool_calls[0].result {
        ToolCallResult::Denied(reason) => {
            assert!(reason.contains("not in allowed_tools"), "{reason}");
        }
        other => panic!("expected Denied, got {other:?}"),
    }

    let _ = session.shutdown().unwrap();
    let ledger = std::fs::read_to_string(dir.path().join("ledger.jsonl")).unwrap();
    assert!(
        ledger.contains("violation"),
        "ledger should have a Violation entry: {ledger}"
    );
}

// --- #92 native dispatch (filesystem / network / exec) -------------

fn write_manifest_with_native_grants(path: &Path, dir: &Path) {
    // Filesystem read covers the workdir + a denied path used by
    // tests below to demonstrate the deny path. exec_grants is empty
    // — the exec dispatch test uses the manifest's default closed
    // policy. write_grants covers a single file.
    let yaml = format!(
        r#"schemaVersion: "1"
agent: {{ name: "native-dispatch-test", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://session-boot.local/agent/research/inst-001" }}
tools:
  filesystem:
    read:
      - "{dir}"
write_grants:
  - resource: "{dir}/out.txt"
    actions: ["write"]
    duration: "PT1H"
"#,
        dir = dir.display()
    );
    std::fs::write(path, yaml).unwrap();
}

fn boot_session_with_manifest(
    dir: &Path,
    ca_dir: &Path,
    manifest_yaml: &str,
    session_id: &str,
) -> Session {
    LocalCa::init(ca_dir, TRUST_DOMAIN).unwrap();
    let manifest_path = dir.join("manifest.yaml");
    let model_path = dir.join("model.gguf");
    let ledger_path = dir.join("ledger.jsonl");
    std::fs::write(&manifest_path, manifest_yaml).unwrap();
    std::fs::write(&model_path, b"fake-model-bytes-for-test").unwrap();

    let cfg = BootConfig {
        session_id: session_id.to_string(),
        manifest_path,
        model_path,
        config_path: None,
        chat_template_sidecar: None,
        identity_dir: ca_dir.to_path_buf(),
        workload_name: "research".to_string(),
        instance: "inst-001".to_string(),
        ledger_path,
    };
    Session::boot(cfg).unwrap()
}

#[test]
fn run_turn_dispatches_filesystem_read_natively_and_returns_contents() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();

    // File the model will "read" via filesystem__read.
    let target = dir.path().join("notes.txt");
    std::fs::write(&target, b"hello from the native fs read").unwrap();

    let manifest_path = dir.path().join("manifest.yaml");
    write_manifest_with_native_grants(&manifest_path, dir.path());
    let manifest_yaml = std::fs::read_to_string(&manifest_path).unwrap();
    let session =
        boot_session_with_manifest(dir.path(), ca_dir.path(), &manifest_yaml, "session-fs-read");

    let response = InferResponse {
        reasoning: "Reading the notes file.".to_string(),
        tool_calls: vec![ToolCall {
            name: "filesystem__read".to_string(),
            arguments: serde_json::json!({"path": target.to_str().unwrap()}),
        }],
        assistant_text: Some("Reading.".to_string()),
    };
    let (mock, mock_handle) = MockLoadedModel::new(vec![Ok(response)]);
    let mut session = session.with_loaded_model(Box::new(mock));

    let outcome = session.run_turn("read the file").unwrap();
    assert_eq!(outcome.tool_calls.len(), 1);
    match &outcome.tool_calls[0].result {
        ToolCallResult::Success(v) => {
            assert_eq!(v["contents"], "hello from the native fs read");
            assert_eq!(v["bytes"], 29);
        }
        other => panic!("expected Success, got {other:?}"),
    }

    // The catalog the model saw included `filesystem__read` (no MCP entries).
    let captured = mock_handle.captured_requests();
    assert!(
        captured[0]
            .tools
            .iter()
            .any(|t| t.name == "filesystem__read"),
        "catalog should advertise filesystem__read: {:?}",
        captured[0]
            .tools
            .iter()
            .map(|t| &t.name)
            .collect::<Vec<_>>()
    );

    let _ = session.shutdown().unwrap();
    let ledger = std::fs::read_to_string(dir.path().join("ledger.jsonl")).unwrap();
    assert!(
        ledger.contains("\"entryType\":\"access\""),
        "expected Access entry in ledger: {ledger}"
    );
}

#[test]
fn run_turn_records_denied_filesystem_read_outside_allowed_paths() {
    // Path NOT covered by the manifest's tools.filesystem.read.
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let manifest_path = dir.path().join("manifest.yaml");
    write_manifest_with_native_grants(&manifest_path, dir.path());
    let manifest_yaml = std::fs::read_to_string(&manifest_path).unwrap();
    let session =
        boot_session_with_manifest(dir.path(), ca_dir.path(), &manifest_yaml, "session-fs-deny");

    let response = InferResponse {
        reasoning: String::new(),
        tool_calls: vec![ToolCall {
            name: "filesystem__read".to_string(),
            arguments: serde_json::json!({"path": "/etc/passwd"}),
        }],
        assistant_text: None,
    };
    let (mock, _h) = MockLoadedModel::new(vec![Ok(response)]);
    let mut session = session.with_loaded_model(Box::new(mock));

    let outcome = session.run_turn("read /etc/passwd").unwrap();
    assert_eq!(outcome.tool_calls.len(), 1);
    match &outcome.tool_calls[0].result {
        ToolCallResult::Denied(_) => {}
        other => panic!("expected Denied, got {other:?}"),
    }
    let _ = session.shutdown().unwrap();
    let ledger = std::fs::read_to_string(dir.path().join("ledger.jsonl")).unwrap();
    assert!(ledger.contains("violation"), "{ledger}");
}

#[test]
fn run_turn_dispatches_filesystem_write_natively_against_a_write_grant() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let manifest_path = dir.path().join("manifest.yaml");
    write_manifest_with_native_grants(&manifest_path, dir.path());
    let manifest_yaml = std::fs::read_to_string(&manifest_path).unwrap();
    let session = boot_session_with_manifest(
        dir.path(),
        ca_dir.path(),
        &manifest_yaml,
        "session-fs-write",
    );

    let target = dir.path().join("out.txt"); // matches the write_grant fixture
    let response = InferResponse {
        reasoning: String::new(),
        tool_calls: vec![ToolCall {
            name: "filesystem__write".to_string(),
            arguments: serde_json::json!({
                "path": target.to_str().unwrap(),
                "contents": "summary"
            }),
        }],
        assistant_text: None,
    };
    let (mock, _h) = MockLoadedModel::new(vec![Ok(response)]);
    let mut session = session.with_loaded_model(Box::new(mock));

    let outcome = session.run_turn("save the summary").unwrap();
    match &outcome.tool_calls[0].result {
        ToolCallResult::Success(v) => {
            assert_eq!(v["bytes"], 7);
        }
        other => panic!("expected Success, got {other:?}"),
    }
    // File was actually written.
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "summary");
}

#[test]
fn run_turn_marks_native_tool_with_missing_args_as_unroutable() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let manifest_path = dir.path().join("manifest.yaml");
    write_manifest_with_native_grants(&manifest_path, dir.path());
    let manifest_yaml = std::fs::read_to_string(&manifest_path).unwrap();
    let session = boot_session_with_manifest(
        dir.path(),
        ca_dir.path(),
        &manifest_yaml,
        "session-fs-noargs",
    );

    // Missing the `path` argument — driver records Unroutable rather
    // than crashing or silently denying.
    let response = InferResponse {
        reasoning: String::new(),
        tool_calls: vec![ToolCall {
            name: "filesystem__read".to_string(),
            arguments: serde_json::json!({}),
        }],
        assistant_text: None,
    };
    let (mock, _h) = MockLoadedModel::new(vec![Ok(response)]);
    let mut session = session.with_loaded_model(Box::new(mock));

    let outcome = session.run_turn("read").unwrap();
    assert!(
        matches!(&outcome.tool_calls[0].result, ToolCallResult::Unroutable(s) if s.contains("path")),
        "{:?}",
        outcome.tool_calls[0].result
    );
}

#[test]
fn boot_refuses_manifest_with_reserved_mcp_server_name() {
    use aegis_inference_engine::Error;

    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    LocalCa::init(ca_dir.path(), TRUST_DOMAIN).unwrap();

    let manifest_path = dir.path().join("manifest.yaml");
    // server_name "filesystem" collides with the reserved native
    // namespace. boot must refuse before any tool dispatch.
    std::fs::write(
        &manifest_path,
        r#"schemaVersion: "1"
agent: { name: "x", version: "1.0.0" }
identity: { spiffeId: "spiffe://session-boot.local/agent/x/1" }
tools:
  mcp:
    - server_name: "filesystem"
      server_uri: "stdio:/usr/bin/something"
      allowed_tools: ["read"]
"#,
    )
    .unwrap();
    std::fs::write(dir.path().join("model.gguf"), b"x").unwrap();

    let cfg = BootConfig {
        session_id: "session-reserved".to_string(),
        manifest_path,
        model_path: dir.path().join("model.gguf"),
        config_path: None,
        chat_template_sidecar: None,
        identity_dir: ca_dir.path().to_path_buf(),
        workload_name: "research".to_string(),
        instance: "inst-001".to_string(),
        ledger_path: dir.path().join("ledger.jsonl"),
    };
    let err = Session::boot(cfg).unwrap_err();
    assert!(
        matches!(err, Error::ReservedMcpServerName { ref name } if name == "filesystem"),
        "{err:?}"
    );
}
