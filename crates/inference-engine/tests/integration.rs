//! End-to-end tests for [`Session`] boot/shutdown (issue #24, F0-A).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use aegis_identity::LocalCa;
use aegis_inference_engine::{BootConfig, Error, Session};
use aegis_ledger_writer::{verify_file, GENESIS_PREV_HASH};
use serde_json::Value;

const TRUST_DOMAIN: &str = "session-boot.local";

fn write_minimal_manifest(path: &Path) {
    let yaml = r#"schemaVersion: "1"
agent: { name: "session-boot", version: "1.0.0" }
identity: { spiffeId: "spiffe://session-boot.local/agent/research/inst-001" }
tools:
  filesystem:
    read: ["/data"]
"#;
    std::fs::write(path, yaml).unwrap();
}

fn init_ca(dir: &Path) {
    LocalCa::init(dir, TRUST_DOMAIN).unwrap();
}

fn boot_cfg(dir: &Path, ca_dir: &Path, session_id: &str) -> BootConfig {
    let manifest_path = dir.join("manifest.yaml");
    let model_path = dir.join("model.gguf");
    let ledger_path = dir.join("ledger.jsonl");
    write_minimal_manifest(&manifest_path);
    std::fs::write(&model_path, b"fake-model-bytes-for-test").unwrap();

    BootConfig {
        session_id: session_id.to_string(),
        manifest_path,
        model_path,
        config_path: None,
        chat_template_sidecar: None,
        identity_dir: ca_dir.to_path_buf(),
        workload_name: "research".to_string(),
        instance: "inst-001".to_string(),
        ledger_path,
    }
}

#[test]
fn boot_writes_session_start_then_shutdown_writes_session_end() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    init_ca(ca_dir.path());

    let cfg = boot_cfg(dir.path(), ca_dir.path(), "session-happy");
    let ledger_path = cfg.ledger_path.clone();

    let session = Session::boot(cfg).unwrap();
    let spiffe = session.spiffe_id().uri();
    assert_eq!(
        spiffe,
        "spiffe://session-boot.local/agent/research/inst-001"
    );

    let root = session.shutdown().unwrap();
    assert_ne!(root, GENESIS_PREV_HASH);

    let summary = verify_file(&ledger_path).unwrap();
    // session_start + network_attestation (zero connections) + session_end
    assert_eq!(summary.entry_count, 3);
    assert_eq!(summary.session_id.as_deref(), Some("session-happy"));
    assert_eq!(summary.root_hash_hex, hex::encode(root));

    let content = std::fs::read_to_string(&ledger_path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    let v0: Value = serde_json::from_str(lines[0]).unwrap();
    let v1: Value = serde_json::from_str(lines[1]).unwrap();
    let v2: Value = serde_json::from_str(lines[2]).unwrap();
    assert_eq!(v0["entryType"], "session_start");
    assert_eq!(v1["entryType"], "network_attestation");
    assert_eq!(v2["entryType"], "session_end");
    assert_eq!(v0["spiffeId"].as_str().unwrap(), spiffe);
    assert_eq!(
        v0["modelDigestHex"].as_str().unwrap().len(),
        64,
        "model digest must be 32-byte hex"
    );
    assert_eq!(v0["manifestDigestHex"].as_str().unwrap().len(), 64);
    assert_eq!(v0["configDigestHex"].as_str().unwrap().len(), 64);
}

#[test]
fn missing_model_file_errors_before_ledger_open() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    init_ca(ca_dir.path());

    let mut cfg = boot_cfg(dir.path(), ca_dir.path(), "session-no-model");
    let _ = std::fs::remove_file(&cfg.model_path);
    cfg.model_path = dir.path().join("does-not-exist.gguf");

    let err = Session::boot(cfg).unwrap_err();
    assert!(matches!(err, Error::Io(_)), "got {err:?}");
    // Ledger file must NOT have been created (no partial-state on the
    // disk for an audit run that never started).
    assert!(!dir.path().join("ledger.jsonl").exists());
}

#[test]
fn ca_not_initialized_errors() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    // Note: ca_dir is empty — no init.

    let cfg = boot_cfg(dir.path(), ca_dir.path(), "session-no-ca");
    let err = Session::boot(cfg).unwrap_err();
    assert!(matches!(err, Error::Identity(_)), "got {err:?}");
}

#[test]
fn manifest_with_extends_is_rejected_in_phase_1a() {
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    init_ca(ca_dir.path());

    let manifest_path = dir.path().join("manifest.yaml");
    let yaml = r#"schemaVersion: "1"
agent: { name: "x", version: "1.0.0" }
identity: { spiffeId: "spiffe://session-boot.local/agent/x/1" }
extends: ["base.yaml"]
tools: {}
"#;
    std::fs::write(&manifest_path, yaml).unwrap();
    std::fs::write(dir.path().join("model.gguf"), b"x").unwrap();

    let cfg = BootConfig {
        session_id: "session-extends".to_string(),
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
    assert!(matches!(err, Error::Policy(_)), "got {err:?}");
}

#[test]
fn boot_with_chat_template_sidecar_binds_into_svid_and_ledger() {
    // Per ADR-022 / OCI-B (b): when the caller supplies a
    // `chat_template.sha256.txt` sidecar (produced by `aegis pull`),
    // boot reads the hex SHA-256, binds it into the SVID via the
    // CHAT_TEMPLATE_BINDING_OID extension, and writes a
    // `chatTemplateDigestHex` field into SessionStart.
    use aegis_identity::{extract_chat_template_from_pem, Digest};

    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    init_ca(ca_dir.path());

    let template_hex = "d5495a1e5db0611132a97e46a65dbb64a642a499421228b9c8b93229097fa9a4";
    let sidecar = dir.path().join("chat_template.sha256.txt");
    std::fs::write(&sidecar, template_hex).unwrap();

    let mut cfg = boot_cfg(dir.path(), ca_dir.path(), "session-template-bound");
    cfg.chat_template_sidecar = Some(sidecar);
    let ledger_path = cfg.ledger_path.clone();

    let session = Session::boot(cfg).unwrap();

    // SVID carries the chat-template extension with the same digest.
    let bound = session.bound_chat_template().copied().unwrap();
    assert_eq!(bound, Digest::from_hex(template_hex).unwrap());
    let extracted = extract_chat_template_from_pem(session.cert_pem()).unwrap();
    assert_eq!(extracted, Some(bound));

    // SessionStart entry surfaces the digest for audit consumption.
    let _ = session.shutdown().unwrap();
    let content = std::fs::read_to_string(&ledger_path).unwrap();
    let v0: Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    assert_eq!(v0["entryType"], "session_start");
    assert_eq!(v0["chatTemplateDigestHex"].as_str().unwrap(), template_hex);
}

#[test]
fn boot_refuses_when_chat_template_sidecar_is_malformed() {
    // Sidecar present but its contents aren't a 64-char hex SHA-256
    // (e.g., truncated, mid-write tampering). The fail-closed pull-side
    // gate from OCI-B (a) is mirrored at boot — refuse rather than bind
    // a placeholder digest.
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    init_ca(ca_dir.path());

    let sidecar = dir.path().join("chat_template.sha256.txt");
    std::fs::write(&sidecar, "not-hex-and-too-short").unwrap();

    let mut cfg = boot_cfg(dir.path(), ca_dir.path(), "session-bad-sidecar");
    cfg.chat_template_sidecar = Some(sidecar);
    let ledger_path = cfg.ledger_path.clone();

    let err = Session::boot(cfg).unwrap_err();
    assert!(
        matches!(err, Error::ChatTemplateSidecar { .. }),
        "got {err:?}"
    );
    // No partial ledger left behind.
    assert!(!ledger_path.exists());
}

#[test]
fn boot_refuses_when_chat_template_sidecar_is_missing() {
    // Caller pointed at a sidecar that doesn't exist. Treat as a
    // configuration error (the sidecar is the binding's input — its
    // absence means the operator pinned a path that isn't there).
    let dir = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    init_ca(ca_dir.path());

    let mut cfg = boot_cfg(dir.path(), ca_dir.path(), "session-absent-sidecar");
    cfg.chat_template_sidecar = Some(dir.path().join("does-not-exist.txt"));
    let ledger_path = cfg.ledger_path.clone();

    let err = Session::boot(cfg).unwrap_err();
    assert!(
        matches!(err, Error::ChatTemplateSidecar { .. }),
        "got {err:?}"
    );
    assert!(!ledger_path.exists());
}

#[test]
fn config_path_changes_config_digest() {
    let dir_a = tempfile::tempdir().unwrap();
    let ca_dir_a = tempfile::tempdir().unwrap();
    init_ca(ca_dir_a.path());

    let mut cfg_a = boot_cfg(dir_a.path(), ca_dir_a.path(), "session-a");
    let cfg_path_a = dir_a.path().join("config.toml");
    std::fs::write(&cfg_path_a, b"[runtime]\nx = 1\n").unwrap();
    cfg_a.config_path = Some(cfg_path_a);

    let session_a = Session::boot(cfg_a).unwrap();
    let digest_a = *session_a.bound_digests();
    session_a.shutdown().unwrap();

    let dir_b = tempfile::tempdir().unwrap();
    let ca_dir_b = tempfile::tempdir().unwrap();
    init_ca(ca_dir_b.path());

    let mut cfg_b = boot_cfg(dir_b.path(), ca_dir_b.path(), "session-b");
    let cfg_path_b = dir_b.path().join("config.toml");
    std::fs::write(&cfg_path_b, b"[runtime]\nx = 2\n").unwrap();
    cfg_b.config_path = Some(cfg_path_b);

    let session_b = Session::boot(cfg_b).unwrap();
    let digest_b = *session_b.bound_digests();
    session_b.shutdown().unwrap();

    assert_eq!(digest_a.model, digest_b.model, "model bytes are identical");
    assert_ne!(
        digest_a.config, digest_b.config,
        "different config files must produce different config_digest"
    );
}
