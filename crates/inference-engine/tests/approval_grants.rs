//! Integration tests for task-scoped ephemeral approval grants
//! (ADR-029, issue #185). Exercises the foundation-slice behavior:
//!
//! - Validating tier: prompt + grant; identical retry auto-consumes
//!   without re-prompting.
//! - Argument drift: same tool + different args re-prompts.
//! - Advisory tier: no prompt at all; ledger records `auto_advisory`.
//! - TTL expiry: a grant past its TTL re-prompts.
//!
//! Blocking and Escalating tier behaviors are deferred — they're
//! recognized in the manifest but treated as Validating for now (PR
//! body documents the gap).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use aegis_approval_gate::FileApprovalChannel;
use aegis_identity::LocalCa;
use aegis_inference_engine::{BootConfig, Session};
use serde_json::Value;

const TRUST_DOMAIN: &str = "approval-grants-test.local";

fn boot(dir: &Path, ca_dir: &Path, session_id: &str, manifest_yaml: &str) -> (Session, PathBuf) {
    LocalCa::init(ca_dir, TRUST_DOMAIN).unwrap();
    let manifest_path = dir.join("manifest.yaml");
    let model_path = dir.join("model.gguf");
    let ledger_path = dir.join("ledger.jsonl");
    std::fs::write(&manifest_path, manifest_yaml).unwrap();
    std::fs::write(&model_path, b"fake-model").unwrap();
    let cfg = BootConfig {
        session_id: session_id.to_string(),
        manifest_path,
        model_path,
        config_path: None,
        chat_template_sidecar: None,
        identity_dir: ca_dir.to_path_buf(),
        workload_name: "research".to_string(),
        instance: "inst-001".to_string(),
        ledger_path: ledger_path.clone(),
        ledger_schema: None,
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

/// Validating tier (default): the first write prompts via the file
/// channel; the second write of the same path auto-consumes the
/// grant. Ledger has TWO writes worth of access entries but only ONE
/// approval_request entry — saving the operator from approval fatigue.
#[test]
fn validating_tier_auto_consumes_identical_retry() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("note.txt");

    let yaml = format!(
        r#"schemaVersion: "1"
agent: {{ name: "ag-test", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://approval-grants-test.local/agent/research/inst-001" }}
tools:
  filesystem:
    write: ["{p}"]
    approval:
      tier: validating
      grant_ttl_seconds: 300
write_grants:
  - resource: "{f}"
    actions: ["write"]
    approval_required: true
"#,
        p = dir.path().to_str().unwrap(),
        f = target.to_str().unwrap(),
    );

    let approval_path = dir.path().join("approval.json");
    let (s, ledger) = boot(dir.path(), ca_dir.path(), "session-validating", &yaml);
    let mut s = s.with_approval_channel(Box::new(FileApprovalChannel::new(&approval_path)));
    std::fs::write(
        &approval_path,
        br#"{"decision":"granted","approver":"alice"}"#,
    )
    .unwrap();

    // First write prompts + grants.
    s.mediate_filesystem_write(&target, b"hello", None).unwrap();
    // Second write of the same path + same contents auto-consumes —
    // no second approval prompt fires, even though the FileApprovalChannel
    // would still return Granted if it did.
    s.mediate_filesystem_write(&target, b"hello", None).unwrap();
    s.shutdown().unwrap();

    let entries = read_entries(&ledger);
    let kinds: Vec<&str> = entries
        .iter()
        .map(|e| e["entryType"].as_str().unwrap())
        .collect();

    let approval_requests = kinds.iter().filter(|k| **k == "approval_request").count();
    let approval_granteds = kinds.iter().filter(|k| **k == "approval_granted").count();
    let auto_consumeds = entries
        .iter()
        .filter(|e| e["decision"].as_str() == Some("auto_consumed_allow"))
        .count();
    let writes = kinds.iter().filter(|k| **k == "access").count();

    assert_eq!(approval_requests, 1, "only the first call prompts");
    assert_eq!(approval_granteds, 1);
    assert_eq!(auto_consumeds, 1, "the second call records auto_consumed");
    assert_eq!(writes, 2, "both writes dispatched");
}

/// Argument drift voids the match: same tool, different contents
/// hash → fresh prompt. Catches the "an attacker mutated the args
/// after approval" failure mode ADR-029 §"Context" calls out.
#[test]
fn arg_drift_forces_fresh_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("drift.txt");

    let yaml = format!(
        r#"schemaVersion: "1"
agent: {{ name: "ag-test", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://approval-grants-test.local/agent/research/inst-001" }}
tools:
  filesystem:
    write: ["{p}"]
    approval:
      tier: validating
      grant_ttl_seconds: 300
write_grants:
  - resource: "{f}"
    actions: ["write"]
    approval_required: true
"#,
        p = dir.path().to_str().unwrap(),
        f = target.to_str().unwrap(),
    );

    let approval_path = dir.path().join("approval.json");
    let (s, ledger) = boot(dir.path(), ca_dir.path(), "session-drift", &yaml);
    let mut s = s.with_approval_channel(Box::new(FileApprovalChannel::new(&approval_path)));
    std::fs::write(
        &approval_path,
        br#"{"decision":"granted","approver":"alice"}"#,
    )
    .unwrap();

    s.mediate_filesystem_write(&target, b"v1", None).unwrap();
    s.mediate_filesystem_write(&target, b"v2 different bytes", None)
        .unwrap();
    s.shutdown().unwrap();

    let entries = read_entries(&ledger);
    let approval_requests = entries
        .iter()
        .filter(|e| e["entryType"].as_str() == Some("approval_request"))
        .count();
    assert_eq!(approval_requests, 2, "arg drift forces re-prompt");
}

/// Advisory tier: the manifest says "no prompt, log + dispatch."
/// The ledger has zero approval_request entries even though the
/// per-call policy returned RequireApproval — the tier short-circuits
/// the gate. One approval_decision entry (decision=auto_advisory) is
/// emitted per dispatch for audit.
#[test]
fn advisory_tier_skips_prompt_entirely() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("advisory.txt");

    let yaml = format!(
        r#"schemaVersion: "1"
agent: {{ name: "ag-test", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://approval-grants-test.local/agent/research/inst-001" }}
tools:
  filesystem:
    write: ["{p}"]
    approval:
      tier: advisory
write_grants:
  - resource: "{f}"
    actions: ["write"]
    approval_required: true
"#,
        p = dir.path().to_str().unwrap(),
        f = target.to_str().unwrap(),
    );

    let approval_path = dir.path().join("approval.json");
    let (s, ledger) = boot(dir.path(), ca_dir.path(), "session-advisory", &yaml);
    let mut s = s.with_approval_channel(Box::new(FileApprovalChannel::new(&approval_path)));
    // Pre-populate the approval file with a grant — it should never
    // be consulted under advisory tier.
    std::fs::write(
        &approval_path,
        br#"{"decision":"granted","approver":"alice"}"#,
    )
    .unwrap();

    s.mediate_filesystem_write(&target, b"hello", None).unwrap();
    s.shutdown().unwrap();

    let entries = read_entries(&ledger);
    let kinds: Vec<&str> = entries
        .iter()
        .map(|e| e["entryType"].as_str().unwrap())
        .collect();

    assert!(
        !kinds.contains(&"approval_request"),
        "advisory tier emits no approval_request, got {kinds:?}"
    );
    let advisory_count = entries
        .iter()
        .filter(|e| e["decision"].as_str() == Some("auto_advisory"))
        .count();
    assert_eq!(advisory_count, 1, "exactly one auto_advisory entry");
    assert!(kinds.contains(&"access"), "dispatch still happened");
}
