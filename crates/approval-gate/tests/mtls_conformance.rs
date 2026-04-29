//! Cross-language conformance for the F3 mTLS approval channel
//! (issue #36). Spawns the Go-side `cmd/approver-bot` against the
//! Rust `MtlsApprovalChannel` and asserts the approver SPIFFE ID
//! threads through end-to-end.
//!
//! Skipped (with a stderr note) when `go` isn't on PATH so local
//! `cargo test` runs cleanly without the Go toolchain installed. CI
//! always has Go available in the conformance job.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use aegis_approval_gate::{ApprovalChannel, ApprovalOutcome, ApprovalRequest, MtlsApprovalChannel};
use aegis_identity::{Digest, DigestTriple, LocalCa};

const TRUST_DOMAIN: &str = "mtls-conformance.local";

fn ephemeral_digests() -> DigestTriple {
    DigestTriple {
        model: Digest([1u8; 32]),
        manifest: Digest([2u8; 32]),
        config: Digest([3u8; 32]),
    }
}

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

fn go_available() -> bool {
    Command::new("go")
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[test]
fn go_approver_bot_grants_via_mtls() {
    if !go_available() {
        eprintln!("skipping: `go` not in PATH (CI conformance job has it)");
        return;
    }

    let work = tempfile::tempdir().unwrap();
    let ca_dir = tempfile::tempdir().unwrap();
    let ca = LocalCa::init(ca_dir.path(), TRUST_DOMAIN).unwrap();

    let server_svid = ca
        .issue_svid("research", "inst-1", ephemeral_digests())
        .unwrap();
    let approver_svid = ca
        .issue_svid("approver-bot", "go-side", ephemeral_digests())
        .unwrap();
    let approver_uri = approver_svid.spiffe_id.uri();

    let cert_path = work.path().join("client.crt");
    let key_path = work.path().join("client.key");
    std::fs::write(&cert_path, &approver_svid.cert_pem).unwrap();
    std::fs::write(&key_path, &approver_svid.key_pem).unwrap();

    let mut channel = MtlsApprovalChannel::new(
        "127.0.0.1:0",
        &server_svid.cert_pem,
        &server_svid.key_pem,
        &ca.root_cert_pem(),
        vec![approver_uri.clone()],
    )
    .unwrap();
    let addr = channel.local_addr();

    let bot = Command::new("go")
        .arg("run")
        .arg("./cmd/approver-bot")
        .arg("--addr")
        .arg(addr.to_string())
        .arg("--client-cert")
        .arg(&cert_path)
        .arg("--client-key")
        .arg(&key_path)
        .arg("--decision")
        .arg("granted")
        .current_dir(workspace_root())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn go approver-bot");

    let outcome = channel
        .request_approval(&ApprovalRequest {
            action_summary: "delete /etc/secrets".to_string(),
            resource_uri: "file:///etc/secrets".to_string(),
            access_type: "delete".to_string(),
            session_id: "session-mtls-conformance".to_string(),
            reasoning_step_id: Some("rstep-007".to_string()),
            timeout: Duration::from_secs(20),
        })
        .unwrap();

    let bot_output = bot.wait_with_output().unwrap();
    if !bot_output.status.success() {
        panic!(
            "approver-bot exited {}: stderr={}",
            bot_output.status,
            String::from_utf8_lossy(&bot_output.stderr)
        );
    }

    match outcome {
        ApprovalOutcome::Granted {
            approver_identity, ..
        } => assert_eq!(approver_identity, approver_uri),
        other => panic!("expected Granted, got {other:?}"),
    }
}
