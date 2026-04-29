//! End-to-end tests for the F4 access-log emitter.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use aegis_access_log::{
    emit_access, emit_reasoning_step, AccessEvent, AccessType, Error, ReasoningStepEvent,
};
use aegis_ledger_writer::{EntryType, LedgerWriter};
use chrono::{TimeZone, Utc};
use serde_json::Value;
use uuid::Uuid;

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
        (
            "tcp://api.example.com:443",
            AccessType::NetworkOutbound,
            512,
        ),
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

// ---------- F5 reasoning-step emitter tests (issue #26) ----------

fn fixed_step_id() -> Uuid {
    // Fixed UUIDv7 for golden-style assertions; the version + variant
    // bits land in the right places so it parses identically on both
    // engines.
    Uuid::parse_str("01977a85-1234-7000-8000-aabbccddeeff").unwrap()
}

#[test]
fn reasoning_step_carries_all_jsonld_terms() {
    let (_dir, mut writer, path) = open_writer();

    let record = emit_reasoning_step(
        &mut writer,
        AGENT_HASH,
        ReasoningStepEvent {
            step_id: fixed_step_id(),
            input: "user asked: summarize Q1 results".to_string(),
            reasoning: "I need to read the source CSV before summarizing.".to_string(),
            tools_considered: vec![
                "filesystem.read".to_string(),
                "search_index".to_string(),
            ],
            tool_selected: Some("filesystem.read".to_string()),
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
    assert_eq!(v["entryType"], "reasoning_step");
    assert_eq!(v["reasoningStepId"], fixed_step_id().to_string());
    assert_eq!(v["input"], "user asked: summarize Q1 results");
    assert_eq!(v["reasoning"], "I need to read the source CSV before summarizing.");
    assert_eq!(v["toolsConsidered"][0], "filesystem.read");
    assert_eq!(v["toolsConsidered"][1], "search_index");
    assert_eq!(v["toolSelected"], "filesystem.read");
}

#[test]
fn reasoning_step_omits_tool_selected_when_none() {
    let (_dir, mut writer, path) = open_writer();
    emit_reasoning_step(
        &mut writer,
        AGENT_HASH,
        ReasoningStepEvent {
            step_id: fixed_step_id(),
            input: "user prompt".to_string(),
            reasoning: "no tool needed".to_string(),
            tools_considered: vec![],
            tool_selected: None,
            timestamp: ts(),
        },
    )
    .unwrap();
    writer.close().unwrap();

    let content = fs::read_to_string(&path).unwrap();
    let v: Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    assert!(
        v.get("toolSelected").is_none(),
        "toolSelected must be absent when not provided"
    );
    assert_eq!(v["toolsConsidered"].as_array().unwrap().len(), 0);
}

#[test]
fn rejects_empty_reasoning_input() {
    let (_dir, mut writer, _) = open_writer();
    let err = emit_reasoning_step(
        &mut writer,
        AGENT_HASH,
        ReasoningStepEvent {
            step_id: fixed_step_id(),
            input: String::new(),
            reasoning: "rationale".to_string(),
            tools_considered: vec![],
            tool_selected: None,
            timestamp: ts(),
        },
    )
    .unwrap_err();
    assert!(matches!(err, Error::EmptyReasoningInput));
    assert_eq!(writer.entry_count(), 0);
}

#[test]
fn reasoning_step_then_access_correlate_one_to_one() {
    // The F5 audit invariant: every Access entry's reasoningStepId
    // resolves to a preceding ReasoningStep entry whose stepId matches.
    // Tested here at the emitter level (the runtime mediator wires the
    // same flow per-tool-call in inference-engine tests).
    let (_dir, mut writer, path) = open_writer();
    let step_id = fixed_step_id();

    emit_reasoning_step(
        &mut writer,
        AGENT_HASH,
        ReasoningStepEvent {
            step_id,
            input: "user prompt".to_string(),
            reasoning: "reading the report file".to_string(),
            tools_considered: vec!["filesystem.read".to_string()],
            tool_selected: Some("filesystem.read".to_string()),
            timestamp: ts(),
        },
    )
    .unwrap();
    emit_access(
        &mut writer,
        AGENT_HASH,
        AccessEvent {
            resource_uri: "file:///data/report.md".to_string(),
            access_type: AccessType::Read,
            bytes_accessed: 4096,
            reasoning_step_id: Some(step_id.to_string()),
            timestamp: ts(),
        },
    )
    .unwrap();
    writer.close().unwrap();

    let content = fs::read_to_string(&path).unwrap();
    let entries: Vec<Value> = content
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["entryType"], "reasoning_step");
    assert_eq!(entries[1]["entryType"], "access");
    assert_eq!(
        entries[0]["reasoningStepId"], entries[1]["reasoningStepId"],
        "Access entry's reasoningStepId must match preceding ReasoningStep's stepId"
    );

    let _ = EntryType::ReasoningStep; // sanity reference
}
