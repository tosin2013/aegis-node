//! Integration tests for the v2 hierarchical per-turn ledger
//! protocol (ADR-026, issue #182). Drives the multi-turn loop with a
//! `Session` booted under [`LedgerSchemaVersion::V2`] and asserts the
//! emitted entry stream — order, types, payload fields, and chain
//! integrity via `verify_file`.
//!
//! The v1 path is exercised by the existing `run_multi_turn.rs` suite;
//! these tests only cover what v2 *adds*. Behaviour the two share
//! (Triple-Bound Circuit Breaker, adversarial pre-filter, message
//! accumulation) is not duplicated here.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::sync::{Arc, Mutex};

use aegis_identity::LocalCa;
use aegis_inference_engine::{
    BackendError, BootConfig, InferRequest, InferResponse, LoadedModel, Session, ToolCall,
    TurnLimits,
};
use aegis_ledger_writer::{verify_file, LedgerSchemaVersion, LEDGER_CONTEXT_V2};
use serde_json::Value;

const TRUST_DOMAIN: &str = "v2-ledger-test.local";

struct MockLoadedModel {
    queue: Arc<Mutex<Vec<Result<InferResponse, BackendError>>>>,
}

impl MockLoadedModel {
    fn new(responses: Vec<InferResponse>) -> Self {
        let queue = Arc::new(Mutex::new(
            responses.into_iter().map(Ok).collect::<Vec<_>>(),
        ));
        Self { queue }
    }
}

impl LoadedModel for MockLoadedModel {
    fn infer(&mut self, _request: InferRequest) -> Result<InferResponse, BackendError> {
        self.queue.lock().unwrap().remove(0)
    }
}

fn write_manifest_with_read_grant(path: &Path) {
    let yaml = format!(
        r#"schemaVersion: "1"
agent: {{ name: "v2-test", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://v2-ledger-test.local/agent/loop/inst-001" }}
tools:
  filesystem:
    read: ["{}"]
"#,
        path.parent().unwrap().display()
    );
    std::fs::write(path, yaml).unwrap();
}

fn boot_session(
    dir: &Path,
    ca_dir: &Path,
    schema: Option<LedgerSchemaVersion>,
) -> (Session, std::path::PathBuf) {
    LocalCa::init(ca_dir, TRUST_DOMAIN).unwrap();

    let manifest_path = dir.join("manifest.yaml");
    let model_path = dir.join("model.gguf");
    let ledger_path = dir.join("ledger.jsonl");
    write_manifest_with_read_grant(&manifest_path);
    std::fs::write(&model_path, b"fake-model-bytes").unwrap();

    let cfg = BootConfig {
        session_id: "session-v2".to_string(),
        manifest_path,
        model_path,
        config_path: None,
        chat_template_sidecar: None,
        identity_dir: ca_dir.to_path_buf(),
        workload_name: "loop".to_string(),
        instance: "inst-001".to_string(),
        ledger_path: ledger_path.clone(),
        ledger_schema: schema,
    };
    (Session::boot(cfg).unwrap(), ledger_path)
}

fn final_text_response(text: &str) -> InferResponse {
    InferResponse {
        reasoning: text.to_string(),
        tool_calls: vec![],
        assistant_text: Some(text.to_string()),
        tokens_used: None,
    }
}

fn read_call_response(path: &str) -> InferResponse {
    InferResponse {
        reasoning: format!("reading {path}"),
        tool_calls: vec![ToolCall {
            name: "filesystem__read".to_string(),
            arguments: serde_json::json!({"path": path}),
        }],
        assistant_text: None,
        tokens_used: None,
    }
}

fn read_entries(path: &Path) -> Vec<Value> {
    let content = std::fs::read_to_string(path).unwrap();
    content
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("valid jsonl"))
        .collect()
}

/// Every entry the writer emits under V2 carries the v2 `@context`
/// URL, and `verify_file` reports the same version back. Establishes
/// the baseline: opt-in is wired through the boot path.
#[test]
fn v2_ledger_stamps_v2_context_on_every_entry() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let (session, ledger_path) =
        boot_session(dir.path(), ca_dir.path(), Some(LedgerSchemaVersion::V2));

    let mock = MockLoadedModel::new(vec![final_text_response("hi")]);
    let mut session = session.with_loaded_model(Box::new(mock));
    session
        .run("greet me", TurnLimits::default())
        .expect("clean termination");
    let _ = session.shutdown();

    let entries = read_entries(&ledger_path);
    assert!(!entries.is_empty(), "ledger has at least one entry");
    for (i, e) in entries.iter().enumerate() {
        assert_eq!(
            e["@context"].as_str().unwrap(),
            LEDGER_CONTEXT_V2,
            "entry {i} carries v2 context"
        );
    }

    let summary = verify_file(&ledger_path).expect("v2 ledger verifies clean");
    assert_eq!(summary.schema_version, Some(LedgerSchemaVersion::V2));
}

/// One full turn that issues a `filesystem__read` tool call should
/// emit the canonical v2 entry sequence: turn_start → reasoning_step →
/// tool_call → tool_result → ... → turn_end, with `tool_call.toolCallId`
/// matching `tool_result.toolCallId` and both pinned to `turn 1`.
#[test]
fn v2_emits_turn_start_tool_call_tool_result_turn_end_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let (session, ledger_path) =
        boot_session(dir.path(), ca_dir.path(), Some(LedgerSchemaVersion::V2));

    // Create a real file the read mediator will accept (manifest grants
    // the temp-dir prefix).
    let target = dir.path().join("greeting.txt");
    std::fs::write(&target, b"hello").unwrap();
    let target_str = target.to_string_lossy().to_string();

    let mock = MockLoadedModel::new(vec![
        read_call_response(&target_str),
        final_text_response("done"),
    ]);
    let mut session = session.with_loaded_model(Box::new(mock));
    session
        .run("read the file", TurnLimits::default())
        .expect("clean termination");
    let _ = session.shutdown();

    let entries = read_entries(&ledger_path);
    let types: Vec<&str> = entries
        .iter()
        .map(|e| e["entryType"].as_str().unwrap())
        .collect();

    // Find the first turn's bracket markers and assert the expected
    // ordering of children between them. We don't pin absolute indices
    // because adversarial / access entries can interleave; we only
    // assert the *partial order* the v2 protocol promises.
    let first_turn_start = types
        .iter()
        .position(|t| *t == "turn_start")
        .expect("turn_start present");
    let first_turn_end = types
        .iter()
        .position(|t| *t == "turn_end")
        .expect("turn_end present");
    assert!(first_turn_end > first_turn_start);

    let between = &types[first_turn_start + 1..first_turn_end];
    let reasoning_at = between
        .iter()
        .position(|t| *t == "reasoning_step")
        .expect("reasoning_step inside first turn");
    let tool_call_at = between
        .iter()
        .position(|t| *t == "tool_call")
        .expect("tool_call inside first turn");
    let tool_result_at = between
        .iter()
        .position(|t| *t == "tool_result")
        .expect("tool_result inside first turn");

    assert!(reasoning_at < tool_call_at, "reasoning precedes tool_call");
    assert!(
        tool_call_at < tool_result_at,
        "tool_call precedes tool_result"
    );

    // tool_call and tool_result share the same toolCallId, and both
    // carry turnNumber: 1.
    let tool_call_entry = entries
        .iter()
        .find(|e| e["entryType"] == "tool_call")
        .unwrap();
    let tool_result_entry = entries
        .iter()
        .find(|e| e["entryType"] == "tool_result")
        .unwrap();
    assert_eq!(tool_call_entry["turnNumber"].as_u64(), Some(1));
    assert_eq!(tool_result_entry["turnNumber"].as_u64(), Some(1));
    assert_eq!(
        tool_call_entry["toolCallId"], tool_result_entry["toolCallId"],
        "tool_call and tool_result share toolCallId"
    );

    // turn_start carries the model digest hex (lowercase 64-char hex).
    let turn_start_entry = entries
        .iter()
        .find(|e| e["entryType"] == "turn_start")
        .unwrap();
    let model_hex = turn_start_entry["modelDigestHex"].as_str().unwrap();
    assert_eq!(model_hex.len(), 64);
    assert!(model_hex.bytes().all(|b| b.is_ascii_hexdigit()));

    // Chain integrity holds across the new entry types.
    let summary = verify_file(&ledger_path).expect("chain intact");
    assert_eq!(summary.schema_version, Some(LedgerSchemaVersion::V2));
}

/// Default schema (None on BootConfig) still emits v1. The whole
/// existing test corpus passes against v1 — this test exists only to
/// pin the default behavior in one canonical place so a future
/// regression in the unwrap_or_default plumbing is loud.
#[test]
fn default_schema_emits_v1_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let (session, ledger_path) = boot_session(dir.path(), ca_dir.path(), None);

    let mock = MockLoadedModel::new(vec![final_text_response("hi")]);
    let mut session = session.with_loaded_model(Box::new(mock));
    session
        .run("greet me", TurnLimits::default())
        .expect("clean termination");
    let _ = session.shutdown();

    let summary = verify_file(&ledger_path).expect("v1 ledger verifies clean");
    assert_eq!(summary.schema_version, Some(LedgerSchemaVersion::V1));

    let entries = read_entries(&ledger_path);
    for e in &entries {
        assert!(
            !e["@context"].as_str().unwrap().ends_with("/v2"),
            "no v2 entries on default schema"
        );
    }

    // No v2-specific entry kinds present.
    let v2_kinds = ["turn_start", "turn_end", "tool_call", "tool_result"];
    for e in &entries {
        let et = e["entryType"].as_str().unwrap();
        assert!(!v2_kinds.contains(&et), "v1 ledger contains v2 kind {et:?}");
    }
}
