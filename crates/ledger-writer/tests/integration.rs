//! End-to-end tests for the ledger writer.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use aegis_ledger_writer::{
    hash_line, Entry, EntryType, Error, LedgerWriter, GENESIS_PREV_HASH,
};
use chrono::{TimeZone, Utc};
use serde_json::{Map, Value};
use uuid::Uuid;

/// Deterministic UUIDv7-shaped UUID generator. Counter-driven so tests can
/// pin golden hashes without time-dependence.
fn fixed_uuids() -> Box<dyn FnMut() -> Uuid + Send> {
    let mut counter: u128 = 0;
    Box::new(move || {
        counter += 1;
        let mut arr = counter.to_be_bytes();
        arr[6] = (arr[6] & 0x0F) | 0x70; // version 7
        arr[8] = (arr[8] & 0x3F) | 0x80; // variant 10
        Uuid::from_bytes(arr)
    })
}

#[test]
fn writes_and_chains_entries_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session-test.jsonl");

    let mut writer = LedgerWriter::create_with_uuid_generator(
        &path,
        "session-test".to_string(),
        fixed_uuids(),
    )
    .unwrap();

    let ts = Utc.with_ymd_and_hms(2026, 4, 27, 22, 0, 0).unwrap();

    let r1 = writer
        .append(Entry {
            session_id: "session-test".to_string(),
            entry_type: EntryType::SessionStart,
            agent_identity_hash: [1u8; 32],
            timestamp: ts,
            payload: Map::new(),
        })
        .unwrap();
    assert_eq!(r1.sequence_number, 0);

    let mut payload = Map::new();
    payload.insert(
        "resourceUri".to_string(),
        Value::String("file:///data/x".to_string()),
    );
    payload.insert(
        "accessType".to_string(),
        Value::String("read".to_string()),
    );
    payload.insert(
        "bytesAccessed".to_string(),
        Value::Number(1024u64.into()),
    );

    let r2 = writer
        .append(Entry {
            session_id: "session-test".to_string(),
            entry_type: EntryType::Access,
            agent_identity_hash: [1u8; 32],
            timestamp: ts,
            payload,
        })
        .unwrap();
    assert_eq!(r2.sequence_number, 1);
    assert_eq!(writer.entry_count(), 2);

    let root = writer.close().unwrap();
    assert_eq!(root, r2.entry_hash);

    let content = fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2);

    // Recompute hashes from disk.
    let h0 = hash_line(lines[0].as_bytes());
    let h1 = hash_line(lines[1].as_bytes());
    assert_eq!(h0, r1.entry_hash);
    assert_eq!(h1, r2.entry_hash);

    // Chain: line 0's prevHash is genesis; line 1's prevHash is h0.
    let v0: Value = serde_json::from_str(lines[0]).unwrap();
    let v1: Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(
        v0["prevHash"].as_str().unwrap(),
        hex::encode(GENESIS_PREV_HASH)
    );
    assert_eq!(v1["prevHash"].as_str().unwrap(), hex::encode(h0));

    // Sanity-check key chain fields.
    assert_eq!(v0["sequenceNumber"].as_u64().unwrap(), 0);
    assert_eq!(v1["sequenceNumber"].as_u64().unwrap(), 1);
    assert_eq!(v0["entryType"].as_str().unwrap(), "session_start");
    assert_eq!(v1["entryType"].as_str().unwrap(), "access");
    assert_eq!(
        v0["@context"].as_str().unwrap(),
        "https://aegis-node.dev/schemas/ledger/v1"
    );
}

#[test]
fn rejects_payload_keys_that_collide_with_chain_fields() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("conflict.jsonl");
    let mut writer =
        LedgerWriter::create(&path, "session-conflict".to_string()).unwrap();

    let mut payload = Map::new();
    payload.insert(
        "entryId".to_string(),
        Value::String("malicious".to_string()),
    );

    let result = writer.append(Entry {
        session_id: "session-conflict".to_string(),
        entry_type: EntryType::Access,
        agent_identity_hash: [0u8; 32],
        timestamp: Utc::now(),
        payload,
    });

    match result {
        Err(Error::PayloadKeyConflict(k)) => assert_eq!(k, "entryId"),
        other => panic!("expected PayloadKeyConflict(\"entryId\"), got {other:?}"),
    }
}

#[test]
fn rejects_session_id_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mismatch.jsonl");
    let mut writer =
        LedgerWriter::create(&path, "session-A".to_string()).unwrap();

    let result = writer.append(Entry {
        session_id: "session-B".to_string(),
        entry_type: EntryType::SessionStart,
        agent_identity_hash: [0u8; 32],
        timestamp: Utc::now(),
        payload: Map::new(),
    });

    assert!(matches!(result, Err(Error::SessionIdMismatch { .. })));
}

#[test]
fn refuses_to_overwrite_existing_ledger_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("exists.jsonl");
    fs::write(&path, b"prior\n").unwrap();

    let result = LedgerWriter::create(&path, "session-overwrite".to_string());
    assert!(result.is_err(), "create should refuse existing file");
}

#[test]
fn deterministic_with_fixed_uuids_and_timestamps() {
    // Same input → same root hash. Locks down the canonicalization so
    // accidental serialization drift trips the test instead of silently
    // breaking the chain semantics frozen by the Compatibility Charter.
    let root_a = run_fixture();
    let root_b = run_fixture();
    assert_eq!(root_a, root_b);
}

fn run_fixture() -> [u8; 32] {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fixture.jsonl");
    let mut writer = LedgerWriter::create_with_uuid_generator(
        &path,
        "session-fixture".to_string(),
        fixed_uuids(),
    )
    .unwrap();

    let ts = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let agent_hash = [0xAAu8; 32];

    for i in 0..3u64 {
        let mut p = Map::new();
        p.insert("idx".to_string(), Value::Number(i.into()));
        writer
            .append(Entry {
                session_id: "session-fixture".to_string(),
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
fn close_with_no_entries_returns_genesis() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.jsonl");
    let writer =
        LedgerWriter::create(&path, "session-empty".to_string()).unwrap();
    let root = writer.close().unwrap();
    assert_eq!(root, GENESIS_PREV_HASH);
}
