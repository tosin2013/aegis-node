//! `EntryType::Violation` emit helper.
//!
//! When the runtime sees a `Decision::Deny` (or a fatal mismatch like F1's
//! digest-rebind), it MUST record a Violation entry in the ledger before
//! halting. This module owns that single responsibility — emit only.
//!
//! Halt is left to the caller: a library that calls `process::exit` makes
//! the runtime un-testable and forces every consumer to use a sandbox to
//! exercise the path. The runtime decides; this crate writes.

use aegis_ledger_writer::{Entry, EntryRecord, EntryType, LedgerWriter};
use chrono::{DateTime, Utc};
use serde_json::{Map, Value};

use crate::decision::NetworkProto;
use crate::error::Result;

/// One violation event. Kept narrow on purpose — the audit trail wants
/// reasoning, resource, and access kind, all of which the JSON-LD `v1`
/// `@context` already names.
#[derive(Debug, Clone)]
pub struct ViolationEvent {
    pub reason: String,
    /// Optional resource URI for context (file path, network address, …).
    pub resource_uri: Option<String>,
    /// Optional access type (read/write/network_outbound/exec/…) — a
    /// snake_case string matching the F4 `accessType` term.
    pub access_type: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl ViolationEvent {
    /// Build a Violation describing a denied network connect attempt.
    pub fn for_network(
        host: &str,
        port: u16,
        proto: NetworkProto,
        reason: impl Into<String>,
        timestamp: DateTime<Utc>,
    ) -> Self {
        let scheme = match proto {
            NetworkProto::Http => "http",
            NetworkProto::Https => "https",
            NetworkProto::Udp => "udp",
            NetworkProto::Tcp | NetworkProto::Any => "tcp",
        };
        Self {
            reason: reason.into(),
            resource_uri: Some(format!("{scheme}://{host}:{port}")),
            access_type: Some("network_outbound".to_string()),
            timestamp,
        }
    }
}

/// Append exactly one `EntryType::Violation` ledger entry. Returns the
/// writer's record so the runtime can correlate the violation with the
/// halt it's about to perform.
pub fn emit_violation(
    writer: &mut LedgerWriter,
    agent_identity_hash: [u8; 32],
    event: ViolationEvent,
) -> Result<EntryRecord> {
    let mut payload = Map::new();
    payload.insert("violationReason".to_string(), Value::String(event.reason));
    if let Some(uri) = event.resource_uri {
        payload.insert("resourceUri".to_string(), Value::String(uri));
    }
    if let Some(at) = event.access_type {
        payload.insert("accessType".to_string(), Value::String(at));
    }

    let session_id = writer.session_id().to_string();
    let record = writer.append(Entry {
        session_id,
        entry_type: EntryType::Violation,
        agent_identity_hash,
        timestamp: event.timestamp,
        payload,
    })?;
    Ok(record)
}
