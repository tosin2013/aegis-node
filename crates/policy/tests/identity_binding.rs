//! F1 conformance: substituting any of the three bound artifacts after
//! identity issuance triggers a halt (Error::IdentityRebind) AND writes
//! a Violation entry naming the offending field.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use aegis_identity::{Digest, DigestTriple, LocalCa};
use aegis_ledger_writer::{EntryType, LedgerWriter};
use aegis_policy::{check_identity_binding, Error};
use chrono::{TimeZone, Utc};
use serde_json::Value;

const TRUST_DOMAIN: &str = "rebind-test.local";
const AGENT_HASH: [u8; 32] = [0x42u8; 32];

fn issue_with(triple: DigestTriple) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let ca = LocalCa::init(dir.path(), TRUST_DOMAIN).unwrap();
    let svid = ca.issue_svid("research", "inst-rebind", triple).unwrap();
    (dir, svid.cert_pem)
}

fn fresh_writer() -> (tempfile::TempDir, LedgerWriter, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rebind.jsonl");
    let writer = LedgerWriter::create(&path, "session-rebind".to_string()).unwrap();
    (dir, writer, path)
}

fn ts() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 28, 12, 0, 0).unwrap()
}

fn baseline_triple() -> DigestTriple {
    DigestTriple {
        model: Digest([0xAAu8; 32]),
        manifest: Digest([0xBBu8; 32]),
        config: Digest([0xCCu8; 32]),
    }
}

#[test]
fn matching_triple_passes_silently() {
    let bound = baseline_triple();
    let (_ca_dir, cert_pem) = issue_with(bound);
    let (_w_dir, mut writer, _) = fresh_writer();

    check_identity_binding(&mut writer, AGENT_HASH, &cert_pem, &bound, ts()).unwrap();
    assert_eq!(writer.entry_count(), 0, "no violation should be written");
}

#[test]
fn model_swap_triggers_halt_and_logs_violation() {
    let bound = baseline_triple();
    let (_ca_dir, cert_pem) = issue_with(bound);
    let (_w_dir, mut writer, path) = fresh_writer();

    let live = DigestTriple {
        model: Digest([0xDEu8; 32]),
        ..bound
    };
    let err = check_identity_binding(&mut writer, AGENT_HASH, &cert_pem, &live, ts()).unwrap_err();
    let mismatch = match err {
        Error::IdentityRebind(m) => m,
        other => panic!("expected IdentityRebind, got {other:?}"),
    };
    assert_eq!(mismatch.field.name(), "model");
    assert_eq!(mismatch.bound, bound.model);
    assert_eq!(mismatch.live, live.model);

    writer.close().unwrap();
    assert_violation_written(&path, "model");
}

#[test]
fn manifest_swap_triggers_halt_and_logs_violation() {
    let bound = baseline_triple();
    let (_ca_dir, cert_pem) = issue_with(bound);
    let (_w_dir, mut writer, path) = fresh_writer();

    let live = DigestTriple {
        manifest: Digest([0x77u8; 32]),
        ..bound
    };
    let err = check_identity_binding(&mut writer, AGENT_HASH, &cert_pem, &live, ts()).unwrap_err();
    assert!(matches!(err, Error::IdentityRebind(ref m) if m.field.name() == "manifest"));
    writer.close().unwrap();
    assert_violation_written(&path, "manifest");
}

#[test]
fn config_swap_triggers_halt_and_logs_violation() {
    let bound = baseline_triple();
    let (_ca_dir, cert_pem) = issue_with(bound);
    let (_w_dir, mut writer, path) = fresh_writer();

    let live = DigestTriple {
        config: Digest([0x33u8; 32]),
        ..bound
    };
    let err = check_identity_binding(&mut writer, AGENT_HASH, &cert_pem, &live, ts()).unwrap_err();
    assert!(matches!(err, Error::IdentityRebind(ref m) if m.field.name() == "config"));
    writer.close().unwrap();
    assert_violation_written(&path, "config");
}

#[test]
fn first_mismatch_wins_when_two_artifacts_change() {
    // Order of detection is fixed (model → manifest → config) so the
    // violation is deterministic and replay-driven audit can match.
    let bound = baseline_triple();
    let (_ca_dir, cert_pem) = issue_with(bound);
    let (_w_dir, mut writer, _) = fresh_writer();

    let live = DigestTriple {
        model: Digest([0x11u8; 32]),
        manifest: Digest([0x22u8; 32]),
        config: bound.config,
    };
    let err = check_identity_binding(&mut writer, AGENT_HASH, &cert_pem, &live, ts()).unwrap_err();
    assert!(matches!(err, Error::IdentityRebind(ref m) if m.field.name() == "model"));
}

#[test]
fn malformed_cert_returns_identity_error_without_writing_violation() {
    let (_w_dir, mut writer, _) = fresh_writer();
    let live = baseline_triple();
    let err = check_identity_binding(&mut writer, AGENT_HASH, "not a PEM block", &live, ts())
        .unwrap_err();
    // A bad cert is not a digest mismatch — we don't pollute the ledger
    // with a violation we can't actually attribute to a real artifact swap.
    assert!(matches!(err, Error::Identity(_)), "got {err:?}");
    assert_eq!(writer.entry_count(), 0);
}

fn assert_violation_written(path: &std::path::Path, expected_field: &str) {
    let content = std::fs::read_to_string(path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 1, "exactly one violation entry expected");
    let v: Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(v["entryType"], "violation");
    assert_eq!(
        v["resourceUri"],
        format!("digest-binding://{expected_field}")
    );
    let reason = v["violationReason"].as_str().unwrap();
    assert!(
        reason.contains(expected_field),
        "reason {reason:?} should mention field {expected_field}"
    );

    let _ = EntryType::Violation; // sanity reference
}
