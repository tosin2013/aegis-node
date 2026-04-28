//! End-to-end tests for the filesystem gate.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Read;

use aegis_filesystem_gate::{Error, GateContext};
use aegis_ledger_writer::LedgerWriter;
use aegis_policy::Policy;
use serde_json::Value;

const AGENT_HASH: [u8; 32] = [0xEEu8; 32];

/// Builds a manifest pinned to the given absolute paths so the gate
/// checks resolve deterministically against the temp-dir scratch layout.
fn policy_for(read: &[&str], write: &[&str], write_grants: &[&str]) -> Policy {
    let read_yaml: Vec<String> = read.iter().map(|p| format!("\"{p}\"")).collect();
    let write_yaml: Vec<String> = write.iter().map(|p| format!("\"{p}\"")).collect();
    let grants: Vec<String> = write_grants
        .iter()
        .map(|p| {
            format!("  - resource: \"{p}\"\n    actions: [\"write\", \"delete\", \"create\"]\n")
        })
        .collect();

    let yaml = format!(
        r#"schemaVersion: "1"
agent: {{ name: "x", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://td/agent/x/1" }}
tools:
  filesystem:
    read: [{}]
    write: [{}]
write_grants:
{}"#,
        read_yaml.join(", "),
        write_yaml.join(", "),
        grants.join(""),
    );
    Policy::from_yaml_bytes(yaml.as_bytes()).unwrap()
}

fn fresh_writer(dir: &std::path::Path) -> (LedgerWriter, std::path::PathBuf) {
    let path = dir.join("ledger.jsonl");
    let writer = LedgerWriter::create(&path, "session-fs".to_string()).unwrap();
    (writer, path)
}

fn read_one_entry(ledger_path: &std::path::Path) -> Value {
    let content = std::fs::read_to_string(ledger_path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 1, "expected exactly one violation entry");
    serde_json::from_str(lines[0]).unwrap()
}

#[test]
fn read_inside_grant_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("ok.txt");
    std::fs::write(&target, b"hello").unwrap();

    let p = policy_for(&[dir.path().to_str().unwrap()], &[], &[]);
    let (mut writer, _ledger) = fresh_writer(dir.path());
    let mut gate = GateContext::new(&p, &mut writer, AGENT_HASH);

    let bytes = gate.read(&target).unwrap();
    assert_eq!(bytes, b"hello");
}

#[test]
fn read_outside_grant_denies_and_writes_violation() {
    let dir = tempfile::tempdir().unwrap();
    let outside = dir.path().join("forbidden.txt");
    std::fs::write(&outside, b"secret").unwrap();

    // Policy grants only "/granted-but-empty" — the actual file is outside.
    let p = policy_for(&["/granted-but-empty"], &[], &[]);
    let (mut writer, ledger_path) = fresh_writer(dir.path());
    let mut gate = GateContext::new(&p, &mut writer, AGENT_HASH);

    let err = gate.read(&outside).unwrap_err();
    assert!(matches!(err, Error::Denied { .. }), "got {err:?}");

    drop(gate);
    writer.close().unwrap();

    let v = read_one_entry(&ledger_path);
    assert_eq!(v["entryType"], "violation");
    assert_eq!(v["accessType"], "read");
    assert!(v["resourceUri"].as_str().unwrap().starts_with("file://"));
    assert!(v["violationReason"].as_str().unwrap().contains("read"));
}

#[test]
fn write_inside_grant_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("out.txt");

    let p = policy_for(&[], &[dir.path().to_str().unwrap()], &[]);
    let (mut writer, _) = fresh_writer(dir.path());
    let mut gate = GateContext::new(&p, &mut writer, AGENT_HASH);

    gate.write(&target, b"payload").unwrap();
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "payload");
}

#[test]
fn write_outside_grant_denies_and_writes_violation() {
    let dir = tempfile::tempdir().unwrap();
    let p = policy_for(&[], &["/granted-but-empty"], &[]);
    let (mut writer, ledger_path) = fresh_writer(dir.path());
    let mut gate = GateContext::new(&p, &mut writer, AGENT_HASH);

    let outside = dir.path().join("nope.txt");
    let err = gate.write(&outside, b"x").unwrap_err();
    assert!(matches!(err, Error::Denied { .. }), "got {err:?}");
    drop(gate);
    writer.close().unwrap();

    let v = read_one_entry(&ledger_path);
    assert_eq!(v["accessType"], "write");
}

#[test]
fn delete_via_write_grant_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("byebye.txt");
    std::fs::write(&target, b"data").unwrap();

    let p = policy_for(&[], &[], &[target.to_str().unwrap()]);
    let (mut writer, _) = fresh_writer(dir.path());
    let mut gate = GateContext::new(&p, &mut writer, AGENT_HASH);

    gate.remove_file(&target).unwrap();
    assert!(!target.exists());
}

#[test]
fn delete_without_grant_denies_and_writes_violation() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("byebye.txt");
    std::fs::write(&target, b"data").unwrap();

    // Wide read+write grants but no write_grant entry for delete.
    let p = policy_for(
        &[dir.path().to_str().unwrap()],
        &[dir.path().to_str().unwrap()],
        &[],
    );
    let (mut writer, ledger_path) = fresh_writer(dir.path());
    let mut gate = GateContext::new(&p, &mut writer, AGENT_HASH);

    let err = gate.remove_file(&target).unwrap_err();
    assert!(matches!(err, Error::Denied { .. }));
    assert!(target.exists(), "file must not be deleted on Deny");

    drop(gate);
    writer.close().unwrap();

    let v = read_one_entry(&ledger_path);
    assert_eq!(v["accessType"], "delete");
}

#[test]
fn rename_requires_delete_on_src_and_write_on_dst() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("a.txt");
    let dst = dir.path().join("b.txt");
    std::fs::write(&src, b"x").unwrap();

    // Has write on dst's dir but no delete grant for src → should fail
    // before any syscall (file at src must remain).
    let p = policy_for(&[], &[dir.path().to_str().unwrap()], &[]);
    let (mut writer, ledger_path) = fresh_writer(dir.path());
    let mut gate = GateContext::new(&p, &mut writer, AGENT_HASH);

    let err = gate.rename(&src, &dst).unwrap_err();
    assert!(matches!(err, Error::Denied { .. }));
    assert!(src.exists());
    assert!(!dst.exists());

    drop(gate);
    writer.close().unwrap();

    let v = read_one_entry(&ledger_path);
    assert_eq!(v["accessType"], "delete", "src delete must fail first");
}

#[test]
fn rename_succeeds_when_both_endpoints_granted() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("a.txt");
    let dst = dir.path().join("b.txt");
    std::fs::write(&src, b"x").unwrap();

    let p = policy_for(
        &[],
        &[dir.path().to_str().unwrap()],
        &[src.to_str().unwrap()],
    );
    let (mut writer, _) = fresh_writer(dir.path());
    let mut gate = GateContext::new(&p, &mut writer, AGENT_HASH);

    gate.rename(&src, &dst).unwrap();
    assert!(!src.exists());
    assert!(dst.exists());
}

#[test]
fn require_approval_does_not_emit_violation() {
    // When the policy returns RequireApproval (e.g. write_grants entry
    // with approval_required: true), the gate must not pollute the ledger
    // with a Violation — approval is a flow, not a violation.
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("audited.txt");

    let yaml = format!(
        r#"schemaVersion: "1"
agent: {{ name: "x", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://td/agent/x/1" }}
tools:
  filesystem:
    write: ["{p}"]
write_grants:
  - resource: "{f}"
    actions: ["write"]
    approval_required: true
"#,
        p = dir.path().to_str().unwrap(),
        f = target.to_str().unwrap(),
    );
    let policy = Policy::from_yaml_bytes(yaml.as_bytes()).unwrap();
    let (mut writer, ledger_path) = fresh_writer(dir.path());
    let mut gate = GateContext::new(&policy, &mut writer, AGENT_HASH);

    let err = gate.write(&target, b"x").unwrap_err();
    assert!(matches!(err, Error::RequireApproval { .. }), "got {err:?}");
    drop(gate);
    writer.close().unwrap();

    // Empty ledger — no violation written.
    assert!(std::fs::metadata(&ledger_path).unwrap().len() == 0);
}

#[test]
fn open_read_returns_a_real_file_handle() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("data.txt");
    std::fs::write(&target, b"contents").unwrap();

    let p = policy_for(&[dir.path().to_str().unwrap()], &[], &[]);
    let (mut writer, _) = fresh_writer(dir.path());
    let mut gate = GateContext::new(&p, &mut writer, AGENT_HASH);

    let mut f = gate.open_read(&target).unwrap();
    let mut buf = String::new();
    f.read_to_string(&mut buf).unwrap();
    assert_eq!(buf, "contents");
}
