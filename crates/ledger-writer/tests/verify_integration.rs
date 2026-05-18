//! End-to-end tests for the ledger verifier.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::io::Write;

use aegis_ledger_writer::{
    verify_file, verify_reader, Entry, EntryType, LedgerSchemaVersion, LedgerWriter, VerifyBreak,
    VerifyError, GENESIS_PREV_HASH,
};
use chrono::{TimeZone, Utc};
use serde_json::{Map, Value};
use uuid::Uuid;

fn fixed_uuids() -> Box<dyn FnMut() -> Uuid + Send> {
    let mut counter: u128 = 0;
    Box::new(move || {
        counter += 1;
        let mut arr = counter.to_be_bytes();
        arr[6] = (arr[6] & 0x0F) | 0x70;
        arr[8] = (arr[8] & 0x3F) | 0x80;
        Uuid::from_bytes(arr)
    })
}

fn write_three_entries(path: &std::path::Path) -> [u8; 32] {
    let mut writer = LedgerWriter::create_with_uuid_generator(
        path,
        "session-verify".to_string(),
        fixed_uuids(),
        LedgerSchemaVersion::V1,
    )
    .unwrap();
    let ts = Utc.with_ymd_and_hms(2026, 4, 28, 0, 0, 0).unwrap();
    let agent_hash = [0xAAu8; 32];
    for i in 0..3u64 {
        let mut p = Map::new();
        p.insert("idx".to_string(), Value::Number(i.into()));
        writer
            .append(Entry {
                session_id: "session-verify".to_string(),
                entry_type: EntryType::SessionStart,
                agent_identity_hash: agent_hash,
                timestamp: ts,
                payload: p,
            })
            .unwrap();
    }
    writer.close().unwrap()
}

#[test]
fn verify_known_good_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("good.jsonl");
    let root = write_three_entries(&path);

    let summary = verify_file(&path).unwrap();
    assert_eq!(summary.session_id.as_deref(), Some("session-verify"));
    assert_eq!(summary.entry_count, 3);
    assert_eq!(summary.root_hash_hex, hex::encode(root));
    assert!(summary.first_timestamp.is_some());
    assert!(summary.last_timestamp.is_some());
}

#[test]
fn verify_empty_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.jsonl");
    fs::write(&path, b"").unwrap();
    let summary = verify_file(&path).unwrap();
    assert_eq!(summary.entry_count, 0);
    assert_eq!(summary.root_hash_hex, hex::encode(GENESIS_PREV_HASH));
    assert!(summary.session_id.is_none());
}

#[test]
fn detects_payload_tamper_at_next_line() {
    // Tamper a byte in line 1's payload. The tamper changes SHA(line 1),
    // so line 2's prev_hash check fails — that's the position the verifier
    // reports (issue #5 acceptance criterion).
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tampered.jsonl");
    write_three_entries(&path);

    let original = fs::read_to_string(&path).unwrap();
    let mut lines: Vec<String> = original.lines().map(|s| s.to_string()).collect();
    // Flip an "idx":1 to "idx":9 inside line 1's JSON.
    lines[1] = lines[1].replacen("\"idx\":1", "\"idx\":9", 1);
    let mut f = fs::File::create(&path).unwrap();
    for line in &lines {
        f.write_all(line.as_bytes()).unwrap();
        f.write_all(b"\n").unwrap();
    }
    drop(f);

    match verify_file(&path) {
        Err(VerifyError::Break(VerifyBreak::PrevHashMismatch { line, .. })) => {
            assert_eq!(line, 2, "tamper of line 1 must surface at line 2");
        }
        other => panic!("expected PrevHashMismatch at line 2, got {other:?}"),
    }
}

#[test]
fn detects_sequence_skip() {
    // Hand-craft a single line whose sequenceNumber is wrong. We can't go
    // through LedgerWriter to do this, since it always emits monotonic
    // sequences — that's exactly the invariant we're testing.
    let line = serde_json::json!({
        "@context": "https://aegis-node.dev/schemas/ledger/v1",
        "entryId": "00000000-0000-7000-8000-000000000001",
        "sessionId": "s",
        "sequenceNumber": 7u64,
        "entryType": "session_start",
        "timestamp": "2026-04-28T00:00:00.000000000Z",
        "agentIdentityHash": "00".repeat(32),
        "prevHash": hex::encode(GENESIS_PREV_HASH),
    })
    .to_string()
        + "\n";

    let err = verify_reader(line.as_bytes()).unwrap_err();
    match err {
        VerifyError::Break(VerifyBreak::SequenceMismatch {
            line,
            expected,
            got,
        }) => {
            assert_eq!(line, 0);
            assert_eq!(expected, 0);
            assert_eq!(got, 7);
        }
        other => panic!("expected SequenceMismatch, got {other:?}"),
    }
}

#[test]
fn detects_bad_context() {
    let line = serde_json::json!({
        "@context": "https://example.com/wrong",
        "entryId": "00000000-0000-7000-8000-000000000001",
        "sessionId": "s",
        "sequenceNumber": 0u64,
        "entryType": "session_start",
        "timestamp": "2026-04-28T00:00:00.000000000Z",
        "agentIdentityHash": "00".repeat(32),
        "prevHash": hex::encode(GENESIS_PREV_HASH),
    })
    .to_string()
        + "\n";
    let err = verify_reader(line.as_bytes()).unwrap_err();
    assert!(matches!(
        err,
        VerifyError::Break(VerifyBreak::BadContext { line: 0, .. })
    ));
}

#[test]
fn detects_invalid_json() {
    let bytes = b"{not valid json}\n";
    let err = verify_reader(&bytes[..]).unwrap_err();
    assert!(matches!(
        err,
        VerifyError::Break(VerifyBreak::InvalidJson { line: 0, .. })
    ));
}

/// ADR-026 §"Schema-version bump policy" — readers detect the
/// version on line 0 and pin; any subsequent line carrying a
/// different `@context` is a chain-break.
#[test]
fn mixing_v1_and_v2_contexts_within_one_ledger_is_a_break() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mixed.jsonl");
    let _ = write_three_entries(&path);

    let v1_url = "https://aegis-node.dev/schemas/ledger/v1";
    let v2_url = "https://aegis-node.dev/schemas/ledger/v2";

    let original = fs::read_to_string(&path).unwrap();
    let mut lines: Vec<String> = original.lines().map(|s| s.to_string()).collect();
    // Swap the second line's @context to v2 — chain integrity stays
    // intact (we don't touch prevHash because the line bytes change,
    // which also breaks the hash chain; that's fine — the BadContext
    // break should fire first because it's checked before prevHash).
    lines[1] = lines[1].replacen(v1_url, v2_url, 1);
    let mut f = fs::File::create(&path).unwrap();
    for line in &lines {
        f.write_all(line.as_bytes()).unwrap();
        f.write_all(b"\n").unwrap();
    }
    drop(f);

    let err = verify_file(&path).unwrap_err();
    let break_ = match err {
        VerifyError::Break(b) => b,
        other => panic!("expected Break, got {other:?}"),
    };
    assert!(
        matches!(break_, VerifyBreak::BadContext { line: 1, .. }),
        "expected BadContext at line 1, got {break_:?}"
    );
}

/// A v2 ledger verifies clean and reports `schema_version: Some(V2)`
/// on the summary. Catches plumbing regressions on the verifier's
/// version-detection path.
#[test]
fn v2_ledger_verifies_and_reports_schema_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v2.jsonl");
    let mut writer = LedgerWriter::create_with_uuid_generator(
        &path,
        "session-v2".to_string(),
        fixed_uuids(),
        LedgerSchemaVersion::V2,
    )
    .unwrap();
    let ts = Utc.with_ymd_and_hms(2026, 5, 18, 0, 0, 0).unwrap();
    let agent_hash = [0xCCu8; 32];
    let mut p = Map::new();
    p.insert("idx".to_string(), Value::Number(0.into()));
    writer
        .append(Entry {
            session_id: "session-v2".to_string(),
            entry_type: EntryType::TurnStart,
            agent_identity_hash: agent_hash,
            timestamp: ts,
            payload: p,
        })
        .unwrap();
    let _ = writer.close().unwrap();

    let summary = verify_file(&path).expect("v2 verifies");
    assert_eq!(summary.schema_version, Some(LedgerSchemaVersion::V2));
    assert_eq!(summary.entry_count, 1);
}

#[test]
fn last_entry_tamper_changes_root_only() {
    // Note: tampering the *last* entry changes its hash but there's no
    // line N+1 to expose the chain break — detection requires comparing
    // to a known-pinned root (out of scope for `aegis verify`'s self-check).
    // This test documents that behavior: verify still returns Ok, just
    // with a different root_hash_hex than the original.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("last.jsonl");
    let original_root = write_three_entries(&path);

    let original = fs::read_to_string(&path).unwrap();
    let mut lines: Vec<String> = original.lines().map(|s| s.to_string()).collect();
    let last_idx = lines.len() - 1;
    lines[last_idx] = lines[last_idx].replacen("\"idx\":2", "\"idx\":99", 1);
    let mut f = fs::File::create(&path).unwrap();
    for line in &lines {
        f.write_all(line.as_bytes()).unwrap();
        f.write_all(b"\n").unwrap();
    }
    drop(f);

    let summary = verify_file(&path).unwrap();
    assert_eq!(summary.entry_count, 3);
    assert_ne!(summary.root_hash_hex, hex::encode(original_root));
}
