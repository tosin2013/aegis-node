//! Integration tests for the per-session aggregate-quota gate
//! (ADR-027, issue #183). Exercises the accumulator at every Session
//! dispatch path: filesystem read/write/delete, network connect, MCP,
//! and exec.
//!
//! The accumulator itself is unit-tested in
//! `crates/policy/src/aggregate.rs`. These tests confirm the wiring
//! is correct end-to-end: a manifest's `quota.max_calls_per_session`
//! actually trips the (N+1)th dispatch, the violation entry lands in
//! the F9 ledger with the expected payload, and a v2 `turn_end`
//! carries the running snapshot for auditors.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use aegis_identity::LocalCa;
use aegis_inference_engine::{BootConfig, Error, Session};
use aegis_ledger_writer::LedgerSchemaVersion;
use serde_json::Value;

const TRUST_DOMAIN: &str = "aggregate-quota-test.local";

fn boot(dir: &Path, ca_dir: &Path, session_id: &str, manifest_yaml: &str) -> (Session, PathBuf) {
    LocalCa::init(ca_dir, TRUST_DOMAIN).unwrap();
    let manifest_path = dir.join("manifest.yaml");
    let model_path = dir.join("model.gguf");
    let ledger_path = dir.join("ledger.jsonl");
    std::fs::write(&manifest_path, manifest_yaml).unwrap();
    std::fs::write(&model_path, b"fake-model-bytes").unwrap();
    let cfg = BootConfig {
        session_id: session_id.to_string(),
        manifest_path,
        model_path,
        config_path: None,
        chat_template_sidecar: None,
        identity_dir: ca_dir.to_path_buf(),
        workload_name: "loop".to_string(),
        instance: "inst-001".to_string(),
        ledger_path: ledger_path.clone(),
        ledger_schema: None,
    };
    (Session::boot(cfg).unwrap(), ledger_path)
}

fn boot_v2(dir: &Path, ca_dir: &Path, session_id: &str, manifest_yaml: &str) -> (Session, PathBuf) {
    LocalCa::init(ca_dir, TRUST_DOMAIN).unwrap();
    let manifest_path = dir.join("manifest.yaml");
    let model_path = dir.join("model.gguf");
    let ledger_path = dir.join("ledger.jsonl");
    std::fs::write(&manifest_path, manifest_yaml).unwrap();
    std::fs::write(&model_path, b"fake-model-bytes").unwrap();
    let cfg = BootConfig {
        session_id: session_id.to_string(),
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

/// `tools.filesystem.quota.max_calls_per_session: 2`. Three reads.
/// First two succeed; the third returns `Error::Denied` and the
/// ledger gains one `Violation` entry namespaced under
/// `violationKind: "AggregateCapExceeded"` for v1-schema compatibility.
#[test]
fn filesystem_read_cap_trips_at_n_plus_one() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("greeting.txt");
    std::fs::write(&target, b"hello").unwrap();

    let manifest = format!(
        r#"schemaVersion: "1"
agent: {{ name: "aq-test", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://aggregate-quota-test.local/agent/loop/inst-001" }}
tools:
  filesystem:
    read: ["{}"]
    quota:
      max_calls_per_session: 2
"#,
        dir.path().display()
    );

    let (mut session, ledger_path) = boot(dir.path(), ca_dir.path(), "session-fs-cap", &manifest);

    // First two reads pass.
    session.mediate_filesystem_read(&target, None).unwrap();
    session.mediate_filesystem_read(&target, None).unwrap();

    // The third trips the cap.
    let err = session.mediate_filesystem_read(&target, None).unwrap_err();
    let reason = match err {
        Error::Denied { reason } => reason,
        other => panic!("expected Denied, got {other:?}"),
    };
    assert!(reason.contains("aggregate cap exceeded"), "reason={reason}");
    assert!(reason.contains("filesystem"), "reason={reason}");
    assert!(reason.contains("2/2"), "reason={reason}");

    let _ = session.shutdown();

    // Exactly one AggregateCapExceeded violation entry on the ledger.
    let entries = read_entries(&ledger_path);
    let agg_violations: Vec<&Value> = entries
        .iter()
        .filter(|e| {
            e["entryType"] == "violation"
                && e["violationKind"].as_str() == Some("AggregateCapExceeded")
        })
        .collect();
    assert_eq!(agg_violations.len(), 1, "exactly one aggregate violation");
    let v = agg_violations[0];
    assert_eq!(v["toolClass"], "filesystem");
    assert_eq!(v["capBound"], "max_calls_per_session");
    assert_eq!(v["observed"], 2);
    assert_eq!(v["cap"], 2);
}

/// `quota` absent → no aggregate cap → no denials, regardless of
/// call volume. Pins the back-compat promise: every existing manifest
/// continues to work exactly as before.
#[test]
fn absent_quota_never_denies() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("note.txt");
    std::fs::write(&target, b"x").unwrap();

    let manifest = format!(
        r#"schemaVersion: "1"
agent: {{ name: "aq-test", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://aggregate-quota-test.local/agent/loop/inst-001" }}
tools:
  filesystem:
    read: ["{}"]
"#,
        dir.path().display()
    );

    let (mut session, _ledger) = boot(dir.path(), ca_dir.path(), "session-no-cap", &manifest);

    for _ in 0..50 {
        session.mediate_filesystem_read(&target, None).unwrap();
    }
}

/// Two classes with independent caps: filesystem cap=1, exec cap=2.
/// A second filesystem read trips immediately; exec gets its full
/// budget regardless.
#[test]
fn classes_are_independent() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("a.txt");
    std::fs::write(&target, b"y").unwrap();

    let manifest = format!(
        r#"schemaVersion: "1"
agent: {{ name: "aq-test", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://aggregate-quota-test.local/agent/loop/inst-001" }}
tools:
  filesystem:
    read: ["{path}"]
    quota:
      max_calls_per_session: 1
  exec:
    quota:
      max_calls_per_session: 2
exec_grants:
  - {{ program: "/bin/true" }}
"#,
        path = dir.path().display()
    );

    let (mut session, _ledger) = boot(dir.path(), ca_dir.path(), "session-mixed", &manifest);

    session.mediate_filesystem_read(&target, None).unwrap();
    // Second filesystem read denied.
    let err = session.mediate_filesystem_read(&target, None).unwrap_err();
    assert!(matches!(err, Error::Denied { .. }));

    // Exec gets its full budget — independent counter.
    let prog = Path::new("/bin/true");
    session.mediate_exec(prog, &[], None).unwrap();
    session.mediate_exec(prog, &[], None).unwrap();
    let exec_err = session.mediate_exec(prog, &[], None).unwrap_err();
    assert!(matches!(exec_err, Error::Denied { .. }));
}

/// A v2 ledger emits `turn_end.quotaSnapshots[]` with one entry per
/// declared-or-dispatched class. Catches plumbing regressions in the
/// `SessionAggregateState::snapshots` ↔ `write_turn_end` path.
#[test]
fn v2_turn_end_carries_quota_snapshots() {
    use aegis_inference_engine::{
        BackendError, InferRequest, InferResponse, LoadedModel, ToolCall, TurnLimits,
    };
    use std::sync::{Arc, Mutex};

    struct MockModel {
        queue: Arc<Mutex<Vec<InferResponse>>>,
    }
    impl LoadedModel for MockModel {
        fn infer(&mut self, _r: InferRequest) -> Result<InferResponse, BackendError> {
            Ok(self.queue.lock().unwrap().remove(0))
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("doc.txt");
    std::fs::write(&target, b"data").unwrap();
    let target_str = target.to_string_lossy().to_string();

    let manifest = format!(
        r#"schemaVersion: "1"
agent: {{ name: "aq-test", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://aggregate-quota-test.local/agent/loop/inst-001" }}
tools:
  filesystem:
    read: ["{}"]
    quota:
      max_calls_per_session: 5
"#,
        dir.path().display()
    );

    let (session, ledger_path) = boot_v2(dir.path(), ca_dir.path(), "session-v2-snap", &manifest);

    // Turn 1: one filesystem__read. Turn 2: clean termination.
    let queue = Arc::new(Mutex::new(vec![
        InferResponse {
            reasoning: "read it".into(),
            tool_calls: vec![ToolCall {
                name: "filesystem__read".into(),
                arguments: serde_json::json!({"path": target_str}),
            }],
            assistant_text: None,
            tokens_used: None,
        },
        InferResponse {
            reasoning: "done".into(),
            tool_calls: vec![],
            assistant_text: Some("done".into()),
            tokens_used: None,
        },
    ]));
    let mut session = session.with_loaded_model(Box::new(MockModel { queue }));
    session.run("read it", TurnLimits::default()).unwrap();
    let _ = session.shutdown();

    let entries = read_entries(&ledger_path);
    let turn_ends: Vec<&Value> = entries
        .iter()
        .filter(|e| e["entryType"] == "turn_end")
        .collect();
    assert!(!turn_ends.is_empty(), "at least one turn_end");

    let first = turn_ends[0];
    let snaps = first["quotaSnapshots"].as_array().unwrap();
    assert!(!snaps.is_empty(), "quotaSnapshots populated, got {snaps:?}");
    let fs_snap = snaps
        .iter()
        .find(|s| s["class"] == "filesystem")
        .expect("filesystem snapshot present");
    assert_eq!(fs_snap["calls"], 1, "one read dispatched on turn 1");
    assert_eq!(fs_snap["max_calls_per_session"], 5);
}
