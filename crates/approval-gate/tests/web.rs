//! End-to-end tests for the localhost web UI approval channel
//! (issue #35, F3).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use aegis_approval_gate::{ApprovalChannel, ApprovalOutcome, ApprovalRequest, WebApprovalChannel};

fn req(timeout_ms: u64) -> ApprovalRequest {
    ApprovalRequest {
        action_summary: "delete /etc/secrets".to_string(),
        resource_uri: "file:///etc/secrets".to_string(),
        access_type: "delete".to_string(),
        session_id: "session-web-test".to_string(),
        reasoning_step_id: Some("rstep-007".to_string()),
        timeout: Duration::from_millis(timeout_ms),
    }
}

fn http_post(url: &str, token: &str, body: &str) -> u16 {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(2))
        .timeout_read(Duration::from_secs(2))
        .build();
    match agent
        .post(url)
        .set("Authorization", &format!("Bearer {token}"))
        .send_string(body)
    {
        Ok(r) => r.status(),
        Err(ureq::Error::Status(code, _)) => code,
        Err(e) => panic!("request {url}: {e}"),
    }
}

fn http_get(url: &str, token: &str) -> (u16, String) {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(2))
        .timeout_read(Duration::from_secs(2))
        .build();
    match agent
        .get(url)
        .set("Authorization", &format!("Bearer {token}"))
        .call()
    {
        Ok(r) => {
            let code = r.status();
            let body = r.into_string().unwrap();
            (code, body)
        }
        Err(ureq::Error::Status(code, r)) => (code, r.into_string().unwrap_or_default()),
        Err(e) => panic!("request {url}: {e}"),
    }
}

#[test]
fn refuses_non_loopback_bind() {
    let err = WebApprovalChannel::new("0.0.0.0:0").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("non-loopback"), "{msg}");
}

#[test]
fn missing_token_returns_401() {
    let mut channel = WebApprovalChannel::new("127.0.0.1:0").unwrap();
    let url = format!("http://{}/approvals", channel.local_addr());
    // Empty token still must not match the channel's real one.
    let (status, _) = http_get(&url, "");
    assert_eq!(status, 401);
    drop(channel);
}

#[test]
fn grant_via_http_resolves_with_approver_in_outcome() {
    let mut channel = WebApprovalChannel::new("127.0.0.1:0").unwrap();
    let addr = channel.local_addr();
    let token = channel.token().to_string();

    // Drive a grant from a separate thread once the request is pending.
    std::thread::spawn(move || {
        // small delay so the agent thread enqueues first
        std::thread::sleep(Duration::from_millis(80));
        let list_url = format!("http://{addr}/approvals");
        let (status, body) = http_get(&list_url, &token);
        assert_eq!(status, 200);
        let entries: Vec<serde_json::Value> = serde_json::from_str(&body).unwrap();
        assert_eq!(entries.len(), 1, "list body: {body}");
        let id = entries[0]["request_id"].as_str().unwrap().to_string();
        let grant_url = format!("http://{addr}/approvals/{id}/grant");
        let status = http_post(&grant_url, &token, r#"{"approver":"alice"}"#);
        assert_eq!(status, 200);
    });

    let outcome = channel.request_approval(&req(5_000)).unwrap();
    match outcome {
        ApprovalOutcome::Granted {
            approver_identity, ..
        } => assert_eq!(approver_identity, "alice"),
        other => panic!("expected Granted, got {other:?}"),
    }
}

#[test]
fn reject_via_http_resolves_with_reason() {
    let mut channel = WebApprovalChannel::new("127.0.0.1:0").unwrap();
    let addr = channel.local_addr();
    let token = channel.token().to_string();

    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(80));
        let list_url = format!("http://{addr}/approvals");
        let (_, body) = http_get(&list_url, &token);
        let entries: Vec<serde_json::Value> = serde_json::from_str(&body).unwrap();
        let id = entries[0]["request_id"].as_str().unwrap().to_string();
        let reject_url = format!("http://{addr}/approvals/{id}/reject");
        let status = http_post(&reject_url, &token, r#"{"reason":"out of scope"}"#);
        assert_eq!(status, 200);
    });

    let outcome = channel.request_approval(&req(5_000)).unwrap();
    match outcome {
        ApprovalOutcome::Rejected { reason, .. } => assert_eq!(reason, "out of scope"),
        other => panic!("expected Rejected, got {other:?}"),
    }
}

#[test]
fn timeout_when_no_decision_arrives() {
    let mut channel = WebApprovalChannel::new("127.0.0.1:0").unwrap();
    let started = std::time::Instant::now();
    let outcome = channel.request_approval(&req(300)).unwrap();
    let elapsed = started.elapsed();
    assert!(
        matches!(outcome, ApprovalOutcome::TimedOut { .. }),
        "got {outcome:?}"
    );
    assert!(elapsed >= Duration::from_millis(250));
}

#[test]
fn token_rotates_per_channel_instance() {
    let a = WebApprovalChannel::new("127.0.0.1:0").unwrap();
    let b = WebApprovalChannel::new("127.0.0.1:0").unwrap();
    assert_ne!(a.token(), b.token());
}
