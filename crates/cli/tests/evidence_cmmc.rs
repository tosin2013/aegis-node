//! Integration tests for `aegis evidence cmmc` (issue #187).
//!
//! Builds a synthetic v2 ledger via the writer directly (so we don't
//! need to spin up a Session), runs the generator, and asserts that
//! the resulting JSON matches the expected control coverage. Pins
//! the most load-bearing mappings: an `access` entry tags AC; a
//! `Violation { violationKind: "AdversarialContent" }` tags SI 3.14.6;
//! `network_attestation` tags SC 3.13.6.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use aegis_cli::evidence::{self, CmmcArgs, EvidencePack};
use aegis_ledger_writer::{Entry, EntryType, LedgerSchemaVersion, LedgerWriter};
use chrono::{TimeZone, Utc};
use serde_json::{Map, Value};

fn write_synthetic_v2_ledger(path: &Path, session_id: &str) {
    let mut w =
        LedgerWriter::create_with_version(path, session_id.to_string(), LedgerSchemaVersion::V2)
            .unwrap();
    let ts = Utc.with_ymd_and_hms(2026, 5, 18, 14, 0, 0).unwrap();
    let agent = [0xAAu8; 32];

    // session_start → 3.4.1, 3.4.3, 3.3.2
    let mut p = Map::new();
    p.insert("spiffeId".into(), Value::String("spiffe://td/x/1".into()));
    w.append(Entry {
        session_id: session_id.into(),
        entry_type: EntryType::SessionStart,
        agent_identity_hash: agent,
        timestamp: ts,
        payload: p,
    })
    .unwrap();

    // turn_start → 3.3.1, 3.3.5, 3.5.4, 3.5.5
    let mut p = Map::new();
    p.insert("turnNumber".into(), Value::Number(1.into()));
    p.insert("modelDigestHex".into(), Value::String("aa".repeat(32)));
    p.insert("contextDigestHex".into(), Value::String("bb".repeat(32)));
    p.insert("svidThumbprintHex".into(), Value::String("cc".repeat(32)));
    p.insert(
        "spiffeIdAud".into(),
        Value::String(format!("aegis-turn://{session_id}/1")),
    );
    w.append(Entry {
        session_id: session_id.into(),
        entry_type: EntryType::TurnStart,
        agent_identity_hash: agent,
        timestamp: ts,
        payload: p,
    })
    .unwrap();

    // access → AC 3.1.1
    let mut p = Map::new();
    p.insert(
        "resourceUri".into(),
        Value::String("file:///tmp/data".into()),
    );
    p.insert("accessType".into(), Value::String("read".into()));
    p.insert("bytesAccessed".into(), Value::Number(10.into()));
    w.append(Entry {
        session_id: session_id.into(),
        entry_type: EntryType::Access,
        agent_identity_hash: agent,
        timestamp: ts,
        payload: p,
    })
    .unwrap();

    // violation (AdversarialContent) → SI 3.14.6
    let mut p = Map::new();
    p.insert(
        "violationKind".into(),
        Value::String("AdversarialContent".into()),
    );
    p.insert("violationReason".into(), Value::String("flagged".into()));
    w.append(Entry {
        session_id: session_id.into(),
        entry_type: EntryType::Violation,
        agent_identity_hash: agent,
        timestamp: ts,
        payload: p,
    })
    .unwrap();

    // network_attestation → SC 3.13.1, 3.13.6
    let mut p = Map::new();
    p.insert("networkConnectionsObserved".into(), Value::Number(0.into()));
    w.append(Entry {
        session_id: session_id.into(),
        entry_type: EntryType::NetworkAttestation,
        agent_identity_hash: agent,
        timestamp: ts,
        payload: p,
    })
    .unwrap();

    // turn_end → 3.3.1, 3.13.4
    let mut p = Map::new();
    p.insert("turnNumber".into(), Value::Number(1.into()));
    p.insert("tokensCumulative".into(), Value::Number(0.into()));
    p.insert("wallclockMsCumulative".into(), Value::Number(0.into()));
    p.insert("quotaSnapshots".into(), Value::Array(Vec::new()));
    w.append(Entry {
        session_id: session_id.into(),
        entry_type: EntryType::TurnEnd,
        agent_identity_hash: agent,
        timestamp: ts,
        payload: p,
    })
    .unwrap();

    // session_end → 3.3.1
    let mut p = Map::new();
    p.insert("spiffeId".into(), Value::String("spiffe://td/x/1".into()));
    w.append(Entry {
        session_id: session_id.into(),
        entry_type: EntryType::SessionEnd,
        agent_identity_hash: agent,
        timestamp: ts,
        payload: p,
    })
    .unwrap();

    w.close().unwrap();
}

#[test]
fn cmmc_evidence_pack_covers_expected_controls() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("session.jsonl");
    write_synthetic_v2_ledger(&ledger, "session-evidence-test");

    let out_dir = dir.path().join("pack");
    let args = CmmcArgs {
        ledger: ledger.clone(),
        out: out_dir.clone(),
        since: None,
        until: None,
    };
    evidence::execute(args).unwrap();

    let json_path = out_dir.join("evidence-pack.json");
    let md_path = out_dir.join("evidence-pack.md");
    assert!(json_path.exists());
    assert!(md_path.exists());

    let raw = std::fs::read_to_string(&json_path).unwrap();
    let pack: EvidencePack = serde_json::from_str(&raw).unwrap();
    assert_eq!(pack.schema_version, "1");
    assert_eq!(pack.session_id.as_deref(), Some("session-evidence-test"));
    assert_eq!(pack.ledger_schema_version, "v2");
    assert_eq!(pack.entry_count, 7);

    // Controls expected to be covered from the synthetic ledger above.
    for cid in [
        // AC — access entry + approval-decision territory not hit here
        "3.1.1", "3.1.2",
        // AU — session_start + reasoning isn't present, but session_start
        // covers 3.3.2; turn_start + access + violation all cover 3.3.1.
        "3.3.1", "3.3.2", // CM — session_start covers 3.4.1 + 3.4.3
        "3.4.1", "3.4.3", // IA — turn_start covers 3.5.4 + 3.5.5
        "3.5.4", "3.5.5", // SC — network_attestation + turn_end
        "3.13.1", "3.13.4", "3.13.6", // SI — AdversarialContent violation
        "3.14.1", "3.14.6",
    ] {
        assert!(
            pack.controls.contains_key(cid),
            "expected control {cid} in pack; got {:?}",
            pack.controls.keys().collect::<Vec<_>>(),
        );
    }
}

#[test]
fn cmmc_evidence_pack_respects_date_filter() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("session.jsonl");
    write_synthetic_v2_ledger(&ledger, "session-filter");

    let out_dir = dir.path().join("pack");
    // All entries are timestamped at 2026-05-18 14:00:00 UTC; pick a
    // since filter past that point so the walk inspects zero entries.
    let args = CmmcArgs {
        ledger: ledger.clone(),
        out: out_dir.clone(),
        since: Some(Utc.with_ymd_and_hms(2026, 5, 19, 0, 0, 0).unwrap()),
        until: None,
    };
    evidence::execute(args).unwrap();

    let raw = std::fs::read_to_string(out_dir.join("evidence-pack.json")).unwrap();
    let pack: EvidencePack = serde_json::from_str(&raw).unwrap();
    assert_eq!(pack.entry_count, 0);
    assert_eq!(pack.controls_covered, 0);
}

#[test]
fn cmmc_evidence_pack_json_validates_against_schema() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("session.jsonl");
    write_synthetic_v2_ledger(&ledger, "session-schema-validate");

    let out_dir = dir.path().join("pack");
    evidence::execute(CmmcArgs {
        ledger,
        out: out_dir.clone(),
        since: None,
        until: None,
    })
    .unwrap();

    // Light schema check — confirm every required top-level field is
    // present in the JSON without a full JSON-schema runtime (we'd
    // pull a heavy dep for one assertion). The schema itself is
    // tested in CI by the schemas.yml workflow which runs ajv-cli.
    let raw = std::fs::read_to_string(out_dir.join("evidence-pack.json")).unwrap();
    let v: Value = serde_json::from_str(&raw).unwrap();
    for required in [
        "schemaVersion",
        "ledgerRootHex",
        "ledgerSchemaVersion",
        "generatedAt",
        "entry_count",
        "controls",
        "controls_covered",
    ] {
        assert!(
            v.get(required).is_some(),
            "missing required field {required}",
        );
    }
    // ledgerRootHex must be 64 lowercase hex chars.
    let root = v["ledgerRootHex"].as_str().unwrap();
    assert_eq!(root.len(), 64);
    assert!(root.bytes().all(|b| b.is_ascii_hexdigit()));
}
