//! End-to-end tests for the F4 access-log emitter.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use aegis_access_log::{emit_access, AccessEvent, AccessType, Error};
use aegis_ledger_writer::{EntryType, LedgerWriter};
use chrono::{TimeZone, Utc};
use serde_json::Value;

const SESSION: &str = "session-access-log";
const AGENT_HASH: [u8; 32] = [0x11u8; 32];

fn open_writer() -> (tempfile::TempDir, LedgerWriter, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.jsonl");
    let writer = LedgerWriter::create(&path, SESSION.to_string()).unwrap();
    (dir, writer, path)
}

fn ts() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 28, 18, 0, 0).unwrap()
}

#[test]
fn emit_writes_one_access_entry() {
    let (_dir, mut writer, path) = open_writer();

    let record = emit_access(
        &mut writer,
        AGENT_HASH,
        AccessEvent {
            resource_uri: "file:///etc/aegis/config.yaml".to_string(),
            access_type: AccessType::Read,
            bytes_accessed: 4096,
            reasoning_step_id: Some("rstep-001".to_string()),
            timestamp: ts(),
        },
    )
    .unwrap();
    assert_eq!(record.sequence_number, 0);

    writer.close().unwrap();
    let content = fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 1);

    let v: Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(v["entryType"], "access");
    assert_eq!(v["sessionId"], SESSION);
    assert_eq!(v["sequenceNumber"], 0);
    assert_eq!(v["resourceUri"], "file:///etc/aegis/config.yaml");
    assert_eq!(v["accessType"], "read");
    assert_eq!(v["bytesAccessed"], 4096);
    assert_eq!(v["reasoningStepId"], "rstep-001");
    assert_eq!(
        v["agentIdentityHash"].as_str().unwrap(),
        hex::encode(AGENT_HASH)
    );
    assert_eq!(
        v["@context"].as_str().unwrap(),
        "https://aegis-node.dev/schemas/ledger/v1"
    );
}

#[test]
fn multi_tool_run_correlates_one_to_one() {
    // Conformance per issue #3 acceptance criteria: a multi-tool run
    // produces an access log such that every tool result correlates to
    // exactly one access entry. Encode that as: N inputs → N entries,
    // monotonic sequence, content preserved per index.
    let (_dir, mut writer, path) = open_writer();

    let events = vec![
        ("file:///data/in.csv", AccessType::Read, 1024),
        ("file:///data/out.csv", AccessType::Write, 2048),
        ("tcp://api.example.com:443", AccessType::NetworkOutbound, 512),
        ("file:///tmp/scratch", AccessType::Delete, 0),
        ("/usr/bin/ffmpeg", AccessType::Exec, 0),
    ];
    for (uri, kind, bytes) in &events {
        emit_access(
            &mut writer,
            AGENT_HASH,
            AccessEvent {
                resource_uri: (*uri).to_string(),
                access_type: *kind,
                bytes_accessed: *bytes,
                reasoning_step_id: None,
                timestamp: ts(),
            },
        )
        .unwrap();
    }
    assert_eq!(writer.entry_count() as usize, events.len());

    writer.close().unwrap();
    let content = fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), events.len());

    for (i, line) in lines.iter().enumerate() {
        let v: Value = serde_json::from_str(line).unwrap();
        assert_eq!(v["sequenceNumber"], i as u64);
        assert_eq!(v["entryType"], "access");
        assert_eq!(v["resourceUri"].as_str().unwrap(), events[i].0);
    }
}

#[test]
fn omits_reasoning_step_id_when_unset() {
    let (_dir, mut writer, path) = open_writer();
    emit_access(
        &mut writer,
        AGENT_HASH,
        AccessEvent {
            resource_uri: "file:///x".to_string(),
            access_type: AccessType::Read,
            bytes_accessed: 1,
            reasoning_step_id: None,
            timestamp: ts(),
        },
    )
    .unwrap();
    writer.close().unwrap();

    let content = fs::read_to_string(&path).unwrap();
    let v: Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    assert!(
        v.get("reasoningStepId").is_none(),
        "reasoningStepId must be absent when not provided"
    );
}

#[test]
fn rejects_empty_resource_uri() {
    let (_dir, mut writer, _) = open_writer();
    let err = emit_access(
        &mut writer,
        AGENT_HASH,
        AccessEvent {
            resource_uri: String::new(),
            access_type: AccessType::Read,
            bytes_accessed: 0,
            reasoning_step_id: None,
            timestamp: ts(),
        },
    )
    .unwrap_err();
    assert!(matches!(err, Error::EmptyResourceUri));
    // Failed emit must not advance the chain.
    assert_eq!(writer.entry_count(), 0);
}

#[test]
fn chain_continues_across_access_entries() {
    let (_dir, mut writer, _) = open_writer();
    let r0 = emit_access(
        &mut writer,
        AGENT_HASH,
        AccessEvent {
            resource_uri: "file:///a".to_string(),
            access_type: AccessType::Read,
            bytes_accessed: 1,
            reasoning_step_id: None,
            timestamp: ts(),
        },
    )
    .unwrap();
    let r1 = emit_access(
        &mut writer,
        AGENT_HASH,
        AccessEvent {
            resource_uri: "file:///b".to_string(),
            access_type: AccessType::Write,
            bytes_accessed: 2,
            reasoning_step_id: None,
            timestamp: ts(),
        },
    )
    .unwrap();

    assert_eq!(r0.sequence_number, 0);
    assert_eq!(r1.sequence_number, 1);
    assert_eq!(writer.current_head(), r1.entry_hash);
    assert_ne!(r0.entry_hash, r1.entry_hash);
}

#[test]
fn each_access_type_serializes_via_payload() {
    let (_dir, mut writer, path) = open_writer();
    let kinds = [
        (AccessType::Read, "read"),
        (AccessType::Write, "write"),
        (AccessType::Delete, "delete"),
        (AccessType::NetworkOutbound, "network_outbound"),
        (AccessType::NetworkInbound, "network_inbound"),
        (AccessType::Exec, "exec"),
    ];
    for (k, _) in &kinds {
        emit_access(
            &mut writer,
            AGENT_HASH,
            AccessEvent {
                resource_uri: format!("file:///{k:?}"),
                access_type: *k,
                bytes_accessed: 0,
                reasoning_step_id: None,
                timestamp: ts(),
            },
        )
        .unwrap();
    }
    writer.close().unwrap();

    let content = fs::read_to_string(&path).unwrap();
    for (line, (_, expected)) in content.lines().zip(kinds.iter()) {
        let v: Value = serde_json::from_str(line).unwrap();
        assert_eq!(v["accessType"].as_str().unwrap(), *expected);
        // Always EntryType::Access at the top level — the *kind* of access
        // is the payload's accessType, not the EntryType.
        assert_eq!(v["entryType"].as_str().unwrap(), "access");
    }
    let _ = EntryType::Access; // sanity reference
}
