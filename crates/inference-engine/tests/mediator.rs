//! End-to-end mediator tests (issue #25, F0-B).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;

use aegis_identity::LocalCa;
use aegis_inference_engine::{BootConfig, Error, Session};
use aegis_ledger_writer::verify_file;
use aegis_policy::NetworkProto;
use serde_json::Value;

const TRUST_DOMAIN: &str = "mediator.local";

fn write_manifest(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
}

fn init_ca(dir: &Path) {
    LocalCa::init(dir, TRUST_DOMAIN).unwrap();
}

fn boot(
    dir: &Path,
    ca_dir: &Path,
    session_id: &str,
    manifest_yaml: &str,
) -> (Session, PathBuf) {
    let manifest_path = dir.join("manifest.yaml");
    let model_path = dir.join("model.gguf");
    let ledger_path = dir.join("ledger.jsonl");
    write_manifest(&manifest_path, manifest_yaml);
    std::fs::write(&model_path, b"fake-model").unwrap();
    let cfg = BootConfig {
        session_id: session_id.to_string(),
        manifest_path,
        model_path,
        config_path: None,
        identity_dir: ca_dir.to_path_buf(),
        workload_name: "research".to_string(),
        instance: "inst-1".to_string(),
        ledger_path: ledger_path.clone(),
    };
    (Session::boot(cfg).unwrap(), ledger_path)
}

fn read_lines(path: &Path) -> Vec<Value> {
    let s = std::fs::read_to_string(path).unwrap();
    s.lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .collect()
}

#[test]
fn allowed_filesystem_read_emits_one_access_entry() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    init_ca(ca_dir.path());

    let target = dir.path().join("data.txt");
    std::fs::write(&target, b"contents").unwrap();
    let yaml = format!(
        r#"schemaVersion: "1"
agent: {{ name: "m", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://mediator.local/agent/research/inst-1" }}
tools:
  filesystem:
    read: ["{p}"]
"#,
        p = dir.path().to_str().unwrap()
    );
    let (mut s, ledger) = boot(dir.path(), ca_dir.path(), "session-fs-read", &yaml);

    let bytes = s.mediate_filesystem_read(&target, Some("rstep-001")).unwrap();
    assert_eq!(bytes, b"contents");

    let root = s.shutdown().unwrap();
    let summary = verify_file(&ledger).unwrap();
    assert_eq!(summary.entry_count, 3);
    assert_eq!(summary.root_hash_hex, hex::encode(root));

    let entries = read_lines(&ledger);
    assert_eq!(entries[0]["entryType"], "session_start");
    assert_eq!(entries[1]["entryType"], "access");
    assert_eq!(entries[1]["accessType"], "read");
    assert_eq!(entries[1]["bytesAccessed"], 8);
    assert_eq!(entries[1]["reasoningStepId"], "rstep-001");
    assert_eq!(entries[2]["entryType"], "session_end");
}

#[test]
fn denied_filesystem_read_emits_one_violation_no_access() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    init_ca(ca_dir.path());

    let outside = dir.path().join("forbidden.txt");
    std::fs::write(&outside, b"secret").unwrap();
    let yaml = r#"schemaVersion: "1"
agent: { name: "m", version: "1.0.0" }
identity: { spiffeId: "spiffe://mediator.local/agent/research/inst-1" }
tools:
  filesystem:
    read: ["/granted-but-empty"]
"#;
    let (mut s, ledger) = boot(dir.path(), ca_dir.path(), "session-fs-deny", yaml);

    let err = s.mediate_filesystem_read(&outside, None).unwrap_err();
    assert!(matches!(err, Error::Denied { .. }), "got {err:?}");
    s.shutdown().unwrap();

    let entries = read_lines(&ledger);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0]["entryType"], "session_start");
    assert_eq!(entries[1]["entryType"], "violation");
    assert_eq!(entries[1]["accessType"], "read");
    assert_eq!(entries[2]["entryType"], "session_end");
}

#[test]
fn approval_required_does_not_emit_violation() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    init_ca(ca_dir.path());

    let target = dir.path().join("audited.txt");
    let yaml = format!(
        r#"schemaVersion: "1"
agent: {{ name: "m", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://mediator.local/agent/research/inst-1" }}
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
    let (mut s, ledger) = boot(dir.path(), ca_dir.path(), "session-approval", &yaml);

    let err = s.mediate_filesystem_write(&target, b"x", None).unwrap_err();
    assert!(matches!(err, Error::RequireApproval { .. }), "got {err:?}");
    s.shutdown().unwrap();

    let entries = read_lines(&ledger);
    assert_eq!(entries.len(), 2, "no violation, no access");
    assert_eq!(entries[0]["entryType"], "session_start");
    assert_eq!(entries[1]["entryType"], "session_end");
}

#[test]
fn allowed_network_connect_emits_access() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut buf = [0u8; 4];
        let _ = s.read(&mut buf);
        s.write_all(b"ok\n").unwrap();
    });

    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    init_ca(ca_dir.path());

    let yaml = format!(
        r#"schemaVersion: "1"
agent: {{ name: "m", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://mediator.local/agent/research/inst-1" }}
tools:
  network:
    outbound:
      allowlist:
        - host: "127.0.0.1"
          port: {port}
          protocol: "tcp"
"#
    );
    let (mut s, ledger) = boot(dir.path(), ca_dir.path(), "session-net", &yaml);

    let mut stream = s
        .mediate_network_connect("127.0.0.1", port, NetworkProto::Tcp, Some("rstep-net"))
        .unwrap();
    stream.write_all(b"ping").unwrap();
    let mut resp = [0u8; 3];
    stream.read_exact(&mut resp).unwrap();
    assert_eq!(&resp, b"ok\n");
    server.join().unwrap();

    s.shutdown().unwrap();
    let entries = read_lines(&ledger);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[1]["entryType"], "access");
    assert_eq!(entries[1]["accessType"], "network_outbound");
    assert_eq!(
        entries[1]["resourceUri"].as_str().unwrap(),
        format!("tcp://127.0.0.1:{port}")
    );
}

#[test]
fn exec_denied_in_v1_without_grant() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    init_ca(ca_dir.path());

    let yaml = r#"schemaVersion: "1"
agent: { name: "m", version: "1.0.0" }
identity: { spiffeId: "spiffe://mediator.local/agent/research/inst-1" }
tools: {}
"#;
    let (mut s, ledger) = boot(dir.path(), ca_dir.path(), "session-exec-deny", yaml);

    let err = s
        .mediate_exec(Path::new("/bin/true"), &[], None)
        .unwrap_err();
    assert!(matches!(err, Error::Denied { .. }));
    s.shutdown().unwrap();

    let entries = read_lines(&ledger);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[1]["entryType"], "violation");
    assert_eq!(entries[1]["accessType"], "exec");
}

#[test]
fn rebind_violation_when_model_bytes_change_mid_session() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    init_ca(ca_dir.path());

    let target = dir.path().join("ok.txt");
    std::fs::write(&target, b"data").unwrap();
    let yaml = format!(
        r#"schemaVersion: "1"
agent: {{ name: "m", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://mediator.local/agent/research/inst-1" }}
tools:
  filesystem:
    read: ["{p}"]
"#,
        p = dir.path().to_str().unwrap()
    );
    let (mut s, ledger) = boot(dir.path(), ca_dir.path(), "session-rebind", &yaml);

    // Tamper the model file mid-session: rebind on the next mediate
    // must spot the digest drift.
    std::fs::write(dir.path().join("model.gguf"), b"tampered-model").unwrap();

    let err = s.mediate_filesystem_read(&target, None).unwrap_err();
    assert!(matches!(err, Error::Policy(_)), "got {err:?}");
    s.shutdown().unwrap();

    // session_start, then the rebind violation, then session_end.
    let entries = read_lines(&ledger);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[1]["entryType"], "violation");
    assert!(entries[1]["violationReason"]
        .as_str()
        .unwrap()
        .contains("model"));
}

#[test]
fn golden_sequence_of_six_tool_calls() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    init_ca(ca_dir.path());

    let read_target = dir.path().join("input.txt");
    std::fs::write(&read_target, b"hello").unwrap();
    let write_target = dir.path().join("output.txt");
    let delete_target = dir.path().join("scratch.txt");
    std::fs::write(&delete_target, b"x").unwrap();
    let denied_target = dir.path().join("forbidden.txt");
    std::fs::write(&denied_target, b"nope").unwrap();

    let yaml = format!(
        r#"schemaVersion: "1"
agent: {{ name: "m", version: "1.0.0" }}
identity: {{ spiffeId: "spiffe://mediator.local/agent/research/inst-1" }}
tools:
  filesystem:
    read: ["{r}"]
    write: ["{w}"]
write_grants:
  - resource: "{d}"
    actions: ["delete"]
"#,
        r = read_target.to_str().unwrap(),
        w = write_target.to_str().unwrap(),
        d = delete_target.to_str().unwrap(),
    );
    let (mut s, ledger) = boot(dir.path(), ca_dir.path(), "session-golden", &yaml);

    s.mediate_filesystem_read(&read_target, Some("r1")).unwrap();
    s.mediate_filesystem_write(&write_target, b"out1", Some("r2"))
        .unwrap();
    s.mediate_filesystem_delete(&delete_target, Some("r3"))
        .unwrap();
    let denied = s
        .mediate_filesystem_read(&denied_target, Some("r4"))
        .unwrap_err();
    assert!(matches!(denied, Error::Denied { .. }));
    s.mediate_filesystem_write(&write_target, b"out2", Some("r5"))
        .unwrap();
    s.shutdown().unwrap();

    let entries = read_lines(&ledger);
    let kinds: Vec<&str> = entries
        .iter()
        .map(|e| e["entryType"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        vec![
            "session_start",
            "access",    // r1: read
            "access",    // r2: write
            "access",    // r3: delete
            "violation", // r4: denied read
            "access",    // r5: second write
            "session_end",
        ]
    );
    let summary = verify_file(&ledger).unwrap();
    assert_eq!(summary.entry_count as usize, kinds.len());
}
