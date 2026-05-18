//! Integration tests for the per-turn SVID rebinding lifecycle
//! (ADR-030, issue #186). Drives the multi-turn loop with a v2
//! ledger and asserts each `turn_start` entry carries a fresh,
//! distinct SVID thumbprint plus a correctly-shaped audience claim.
//!
//! The TurnBinding extension itself is unit-tested in
//! `crates/identity/src/svid.rs`. These tests confirm wiring is
//! correct end-to-end: the multi-turn driver actually mints + drops
//! per-turn SVIDs and writes their identities into the ledger where
//! auditors will look.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use aegis_identity::LocalCa;
use aegis_inference_engine::{
    BackendError, BootConfig, InferRequest, InferResponse, LoadedModel, Session, ToolCall,
    TurnLimits,
};
use aegis_ledger_writer::LedgerSchemaVersion;
use serde_json::Value;

const TRUST_DOMAIN: &str = "per-turn-svid-test.local";

struct MockLoadedModel {
    queue: Arc<Mutex<Vec<InferResponse>>>,
}
impl MockLoadedModel {
    fn new(rs: Vec<InferResponse>) -> Self {
        Self {
            queue: Arc::new(Mutex::new(rs)),
        }
    }
}
impl LoadedModel for MockLoadedModel {
    fn infer(&mut self, _r: InferRequest) -> Result<InferResponse, BackendError> {
        Ok(self.queue.lock().unwrap().remove(0))
    }
}

fn boot_v2(dir: &Path, ca_dir: &Path) -> (Session, PathBuf) {
    LocalCa::init(ca_dir, TRUST_DOMAIN).unwrap();
    let manifest_path = dir.join("manifest.yaml");
    let model_path = dir.join("model.gguf");
    let ledger_path = dir.join("ledger.jsonl");
    let yaml = format!(
        r#"schemaVersion: "1"
agent: {{ name: "svid-test", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://per-turn-svid-test.local/agent/loop/inst-001" }}
tools:
  filesystem:
    read: ["{}"]
"#,
        dir.display()
    );
    std::fs::write(&manifest_path, yaml).unwrap();
    std::fs::write(&model_path, b"fake-model-bytes").unwrap();
    let cfg = BootConfig {
        session_id: "session-svid".to_string(),
        manifest_path,
        model_path,
        config_path: None,
        chat_template_sidecar: None,
        identity_dir: ca_dir.to_path_buf(),
        workload_name: "loop".to_string(),
        instance: "inst-001".to_string(),
        ledger_path: ledger_path.clone(),
        ledger_schema: Some(LedgerSchemaVersion::V2),
    };
    (Session::boot(cfg).unwrap(), ledger_path)
}

fn read_entries(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("valid json"))
        .collect()
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

fn final_text_response(text: &str) -> InferResponse {
    InferResponse {
        reasoning: text.to_string(),
        tool_calls: vec![],
        assistant_text: Some(text.to_string()),
        tokens_used: None,
    }
}

/// Two turns produce two distinct `turn_start` entries with distinct
/// `svidThumbprintHex` values — proving per-turn rebinding actually
/// minted two separate SVIDs (not just recorded the same one twice).
#[test]
fn two_turn_session_emits_two_distinct_svid_thumbprints() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("note.txt");
    std::fs::write(&target, b"hi").unwrap();
    let target_str = target.to_string_lossy().to_string();

    let (session, ledger_path) = boot_v2(dir.path(), ca_dir.path());
    let mock = MockLoadedModel::new(vec![
        read_call_response(&target_str),
        final_text_response("done"),
    ]);
    let mut session = session.with_loaded_model(Box::new(mock));
    session
        .run("two turns please", TurnLimits::default())
        .unwrap();
    let _ = session.shutdown();

    let entries = read_entries(&ledger_path);
    let starts: Vec<&Value> = entries
        .iter()
        .filter(|e| e["entryType"] == "turn_start")
        .collect();
    assert_eq!(starts.len(), 2, "two turns means two turn_start entries");

    let tp0 = starts[0]["svidThumbprintHex"].as_str().unwrap();
    let tp1 = starts[1]["svidThumbprintHex"].as_str().unwrap();
    assert_eq!(tp0.len(), 64, "sha256 hex");
    assert!(tp0.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_ne!(tp0, tp1, "each turn mints a fresh SVID");
}

/// Audience claim format matches `aegis-turn://<session>/<turn>` and
/// pins to the correct turn number on each entry.
#[test]
fn audience_claim_format_matches_adr_030() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"x").unwrap();
    let target_str = target.to_string_lossy().to_string();

    let (session, ledger_path) = boot_v2(dir.path(), ca_dir.path());
    let mock = MockLoadedModel::new(vec![
        read_call_response(&target_str),
        final_text_response("ok"),
    ]);
    let mut session = session.with_loaded_model(Box::new(mock));
    session.run("aud check", TurnLimits::default()).unwrap();
    let _ = session.shutdown();

    let entries = read_entries(&ledger_path);
    let starts: Vec<&Value> = entries
        .iter()
        .filter(|e| e["entryType"] == "turn_start")
        .collect();

    assert_eq!(
        starts[0]["spiffeIdAud"].as_str().unwrap(),
        "aegis-turn://session-svid/1"
    );
    assert_eq!(
        starts[1]["spiffeIdAud"].as_str().unwrap(),
        "aegis-turn://session-svid/2"
    );
}

/// v1 ledger doesn't carry the new fields — `turn_start` entries
/// aren't emitted at all on v1, so `svidThumbprintHex` and
/// `spiffeIdAud` never surface there. Pins the back-compat promise.
#[test]
fn v1_ledger_does_not_emit_svid_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    LocalCa::init(ca_dir.path(), TRUST_DOMAIN).unwrap();

    let manifest_path = dir.path().join("manifest.yaml");
    let model_path = dir.path().join("model.gguf");
    let ledger_path = dir.path().join("ledger.jsonl");
    std::fs::write(
        &manifest_path,
        format!(
            r#"schemaVersion: "1"
agent: {{ name: "svid-v1", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://per-turn-svid-test.local/agent/loop/inst-001" }}
tools:
  filesystem:
    read: ["{}"]
"#,
            dir.path().display()
        ),
    )
    .unwrap();
    std::fs::write(&model_path, b"fake-model-bytes").unwrap();

    let cfg = BootConfig {
        session_id: "session-v1-svid".to_string(),
        manifest_path,
        model_path,
        config_path: None,
        chat_template_sidecar: None,
        identity_dir: ca_dir.path().to_path_buf(),
        workload_name: "loop".to_string(),
        instance: "inst-001".to_string(),
        ledger_path: ledger_path.clone(),
        ledger_schema: None, // v1 default
    };
    let session = Session::boot(cfg).unwrap();
    let mock = MockLoadedModel::new(vec![final_text_response("hi")]);
    let mut session = session.with_loaded_model(Box::new(mock));
    session.run("v1 check", TurnLimits::default()).unwrap();
    let _ = session.shutdown();

    let entries = read_entries(&ledger_path);
    for e in &entries {
        assert_ne!(e["entryType"], "turn_start", "no turn_start on v1");
        assert!(
            e.get("svidThumbprintHex").is_none(),
            "no per-turn SVID metadata on v1 entries: {e:?}"
        );
    }
}
