//! Aegis-Node structured access-log emitter (F4 per ADR-006).
//!
//! Every I/O operation the runtime mediates — filesystem read/write/delete,
//! network connect, exec — produces exactly one access-log entry in the
//! Trajectory Ledger. The entry shape is JSON-LD per
//! `schemas/ledger/v1/context.jsonld`; the payload keys
//! `resourceUri` / `accessType` / `bytesAccessed` / `reasoningStepId` are
//! frozen by the v1 `@context` and the Compatibility Charter.
//!
//! This crate is the *typed event surface* for F4. The chain primitive
//! (append, fsync, hash chain) lives in `aegis-ledger-writer`; the syscall
//! interception layer that *generates* events lives in `aegis-network-gate`
//! and the future filesystem-sandbox crate (issue #7). Atomicity is
//! inherited from the underlying writer (write_all + sync_all per entry).

use aegis_ledger_writer::{Entry, EntryRecord, EntryType, LedgerWriter};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

pub mod error;
pub use error::{Error, Result};

/// Kinds of access the runtime mediates. Serializes as snake_case to match
/// the proto enum (`ACCESS_TYPE_*`) and the JSON-LD `accessType` term.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessType {
    Read,
    Write,
    Delete,
    NetworkOutbound,
    NetworkInbound,
    Exec,
}

/// One access event. The runtime constructs this at the syscall boundary
/// and hands it to [`emit_access`].
#[derive(Debug, Clone)]
pub struct AccessEvent {
    /// URI of the resource accessed (e.g. `file:///etc/passwd`,
    /// `tcp://10.0.0.1:443`). Must be non-empty.
    pub resource_uri: String,
    pub access_type: AccessType,
    /// Bytes read/written/transferred. Use 0 for delete and exec.
    pub bytes_accessed: u64,
    /// Reasoning-step ID this access correlates to (F5). Optional in
    /// Phase 1a — populated once the F5 emitter lands.
    pub reasoning_step_id: Option<String>,
    /// When the access happened. Caller supplies a real wall-clock value;
    /// `Utc::now()` is fine for the runtime, fixture timestamps are fine
    /// for tests/replay.
    pub timestamp: DateTime<Utc>,
}

/// One reasoning step emitted before a tool call (F5 per ADR-007). The
/// step ID is what later access entries' `reasoning_step_id` field points
/// at, giving auditors a reverse lookup from "what happened" to "why".
///
/// Phase 1a accepts pre-computed text — the LLM-driven runtime that
/// generates `input` / `reasoning` / `tools_considered` / `tool_selected`
/// arrives in Phase 2 (ADR-014's llama.cpp Backend). Until then, callers
/// supply these fields directly (e.g., test fixtures, external graders).
#[derive(Debug, Clone)]
pub struct ReasoningStepEvent {
    /// Step identifier embedded into downstream `EntryType::Access`
    /// entries. Default-construct via [`ReasoningStepEvent::new_v7_id`]
    /// for the typical caller.
    pub step_id: Uuid,
    /// What the agent received (user prompt, system input, prior tool
    /// result that led to this reasoning).
    pub input: String,
    /// The agent's free-text rationale for choosing the next tool.
    pub reasoning: String,
    /// Tools the agent considered, in the order it considered them.
    pub tools_considered: Vec<String>,
    /// The tool the agent selected. None if the agent decided not to
    /// invoke any tool (the reasoning is still recorded for audit).
    pub tool_selected: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl ReasoningStepEvent {
    /// Convenience: generate a UUIDv7 for `step_id`.
    pub fn new_v7_id() -> Uuid {
        Uuid::now_v7()
    }
}

/// Append exactly one `EntryType::ReasoningStep` ledger entry. Returns
/// the writer's record; callers thread `event.step_id` into the
/// `reasoning_step_id` field of the next [`emit_access`] call so an
/// auditor can correlate "agent did X" back to "agent reasoned Y".
///
/// Per the F5 contract (ADR-007): every Access entry SHOULD have a
/// preceding ReasoningStep entry whose `reasoningStepId` matches.
/// Enforcing the SHOULD is a runtime concern (the mediator's caller);
/// this function is the typed event surface.
pub fn emit_reasoning_step(
    writer: &mut LedgerWriter,
    agent_identity_hash: [u8; 32],
    event: ReasoningStepEvent,
) -> Result<EntryRecord> {
    if event.input.is_empty() {
        return Err(Error::EmptyReasoningInput);
    }

    let mut payload = Map::new();
    payload.insert(
        "reasoningStepId".to_string(),
        Value::String(event.step_id.to_string()),
    );
    payload.insert("input".to_string(), Value::String(event.input));
    payload.insert("reasoning".to_string(), Value::String(event.reasoning));
    payload.insert(
        "toolsConsidered".to_string(),
        Value::Array(
            event
                .tools_considered
                .into_iter()
                .map(Value::String)
                .collect(),
        ),
    );
    if let Some(tool) = event.tool_selected {
        payload.insert("toolSelected".to_string(), Value::String(tool));
    }

    let session_id = writer.session_id().to_string();
    let record = writer.append(Entry {
        session_id,
        entry_type: EntryType::ReasoningStep,
        agent_identity_hash,
        timestamp: event.timestamp,
        payload,
    })?;
    Ok(record)
}

/// Append exactly one `EntryType::Access` ledger entry for `event`.
/// The chain advances by one and the new entry's hash becomes the next
/// `prev_hash`. Returns the writer's record so the caller can correlate
/// the access with downstream work (e.g. cross-language test harness).
pub fn emit_access(
    writer: &mut LedgerWriter,
    agent_identity_hash: [u8; 32],
    event: AccessEvent,
) -> Result<EntryRecord> {
    if event.resource_uri.is_empty() {
        return Err(Error::EmptyResourceUri);
    }

    let mut payload = Map::new();
    payload.insert("resourceUri".to_string(), Value::String(event.resource_uri));
    payload.insert(
        "accessType".to_string(),
        serde_json::to_value(event.access_type)?,
    );
    payload.insert(
        "bytesAccessed".to_string(),
        Value::Number(event.bytes_accessed.into()),
    );
    if let Some(rsid) = event.reasoning_step_id {
        payload.insert("reasoningStepId".to_string(), Value::String(rsid));
    }

    let session_id = writer.session_id().to_string();
    let record = writer.append(Entry {
        session_id,
        entry_type: EntryType::Access,
        agent_identity_hash,
        timestamp: event.timestamp,
        payload,
    })?;
    Ok(record)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn access_type_serializes_snake_case() {
        let v = serde_json::to_value(AccessType::NetworkOutbound).unwrap();
        assert_eq!(v, Value::String("network_outbound".to_string()));
        let v = serde_json::to_value(AccessType::Read).unwrap();
        assert_eq!(v, Value::String("read".to_string()));
    }
}
