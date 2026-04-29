//! End-to-end tests for the file-channel approval (issue #27).
//!
//! TTY channel is exercised manually (driving an interactive
//! stdin/stderr pair from a unit test is fragile in CI). The file
//! channel is the canonical automation surface and the one the
//! conformance harness will use.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use aegis_approval_gate::{
    ApprovalChannel, ApprovalOutcome, ApprovalRequest, FileApprovalChannel,
};

fn req(timeout_ms: u64) -> ApprovalRequest {
    ApprovalRequest {
        action_summary: "test action".to_string(),
        resource_uri: "file:///data/x".to_string(),
        access_type: "write".to_string(),
        session_id: "session-test".to_string(),
        reasoning_step_id: None,
        timeout: Duration::from_millis(timeout_ms),
    }
}

#[test]
fn file_channel_grants_when_file_present_with_granted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("approval.json");
    std::fs::write(&path, br#"{"decision":"granted","approver":"alice"}"#).unwrap();

    let mut channel = FileApprovalChannel::new(&path);
    let outcome = channel.request_approval(&req(2000)).unwrap();
    match outcome {
        ApprovalOutcome::Granted {
            approver_identity, ..
        } => assert_eq!(approver_identity, "alice"),
        other => panic!("expected Granted, got {other:?}"),
    }
}

#[test]
fn file_channel_rejects_when_file_says_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("approval.json");
    std::fs::write(
        &path,
        br#"{"decision":"rejected","reason":"scope is too broad"}"#,
    )
    .unwrap();

    let mut channel = FileApprovalChannel::new(&path);
    let outcome = channel.request_approval(&req(2000)).unwrap();
    match outcome {
        ApprovalOutcome::Rejected { reason, .. } => {
            assert_eq!(reason, "scope is too broad");
        }
        other => panic!("expected Rejected, got {other:?}"),
    }
}

#[test]
fn file_channel_times_out_when_file_never_appears() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("approval.json"); // never written

    let mut channel = FileApprovalChannel::new(&path);
    let started = std::time::Instant::now();
    let outcome = channel.request_approval(&req(300)).unwrap();
    let elapsed = started.elapsed();

    assert!(
        matches!(outcome, ApprovalOutcome::TimedOut { .. }),
        "got {outcome:?}"
    );
    assert!(
        elapsed >= Duration::from_millis(250) && elapsed < Duration::from_secs(3),
        "elapsed should respect the 300ms timeout: got {elapsed:?}"
    );
}

#[test]
fn file_channel_grants_after_late_arrival() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("approval.json");
    let path_clone = path.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        std::fs::write(&path_clone, br#"{"decision":"granted"}"#).unwrap();
    });

    let mut channel = FileApprovalChannel::new(&path);
    let outcome = channel.request_approval(&req(2000)).unwrap();
    assert!(matches!(outcome, ApprovalOutcome::Granted { .. }));
}

#[test]
fn file_channel_errors_on_malformed_decision() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("approval.json");
    std::fs::write(&path, br#"{"decision":"maybe"}"#).unwrap();

    let mut channel = FileApprovalChannel::new(&path);
    let err = channel.request_approval(&req(1000)).unwrap_err();
    assert!(format!("{err}").contains("malformed"), "got {err:?}");
}
