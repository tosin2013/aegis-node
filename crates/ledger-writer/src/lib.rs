//! Aegis-Node ledger writer.
//!
//! Hash-chained, append-only Trajectory Ledger writer per ADR-011 (F9).
//!
//! Each ledger entry is one line of JSON-LD on disk, conforming to the v1
//! @context at `schemas/ledger/v1/context.jsonld`. The chain is anchored on a
//! 32-byte zero genesis and walks forward via:
//!
//! ```text
//! prev_hash[N+1] = SHA-256( line[N] )           // bytes literally written, no LF
//! ```
//!
//! The serialization is JCS-compatible for our value subset (sorted keys, no
//! whitespace, integers without decimals) — `serde_json::Map` is `BTreeMap`
//! by default, which gives us byte-deterministic output across Rust builds and
//! across reimplementations of a verifier in other languages.
//!
//! The Compatibility Charter freezes these semantics. See
//! `docs/COMPATIBILITY_CHARTER.md`.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub mod error;
pub mod verify;
pub use error::{Error, Result};
pub use verify::{verify_file, verify_reader, VerifyBreak, VerifyError, VerifySummary};

/// JSON-LD `@context` URI for v1 ledger entries.
pub const LEDGER_CONTEXT: &str = "https://aegis-node.dev/schemas/ledger/v1";

/// Genesis prev-hash: 32 zero bytes, used as the previous-hash for the first
/// entry of every session.
pub const GENESIS_PREV_HASH: [u8; 32] = [0u8; 32];

/// Top-level fields the writer owns; payload keys colliding with these are rejected.
const RESERVED_KEYS: &[&str] = &[
    "@context",
    "entryId",
    "sessionId",
    "sequenceNumber",
    "entryType",
    "timestamp",
    "agentIdentityHash",
    "prevHash",
];

/// Kind of ledger entry. Mirrors `aegis.v1.EntryType` in the proto contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryType {
    SessionStart,
    SessionEnd,
    ReasoningStep,
    Access,
    ApprovalRequest,
    ApprovalGranted,
    ApprovalRejected,
    ApprovalTimedOut,
    Violation,
    NetworkAttestation,
}

/// An entry to append. The writer fills in `entryId`, `sequenceNumber`,
/// `prevHash`, and the `@context`. The caller supplies session, type,
/// identity hash, timestamp, and any type-specific payload fields.
///
/// Payload keys are flattened into the top-level JSON-LD object and must
/// not collide with the reserved chain fields.
pub struct Entry {
    pub session_id: String,
    pub entry_type: EntryType,
    pub agent_identity_hash: [u8; 32],
    pub timestamp: DateTime<Utc>,
    pub payload: Map<String, Value>,
}

/// Record of an appended entry: the assigned ID, sequence number, and the
/// SHA-256 of the canonical line written to disk (which becomes the next
/// entry's `prev_hash`, and the chain root once `close` is called).
#[derive(Debug, Clone)]
pub struct EntryRecord {
    pub entry_id: Uuid,
    pub sequence_number: u64,
    pub entry_hash: [u8; 32],
}

type UuidGenerator = Box<dyn FnMut() -> Uuid + Send>;

/// LedgerWriter owns an open ledger file and tracks the running prev_hash.
/// One writer per session — concurrent writes are not supported in v1.
pub struct LedgerWriter {
    file: BufWriter<File>,
    session_id: String,
    next_sequence: u64,
    prev_hash: [u8; 32],
    uuid_generator: UuidGenerator,
}

impl LedgerWriter {
    /// Create a new ledger file at `path`. Errors if the file already exists
    /// — sessions own their ledger; reusing a path would imply tampering.
    pub fn create<P: AsRef<Path>>(path: P, session_id: String) -> Result<Self> {
        Self::create_with_uuid_generator(path, session_id, Box::new(Uuid::now_v7))
    }

    /// Like `create` but with an injectable UUID generator. Used by tests for
    /// deterministic golden fixtures and by future replay tools.
    pub fn create_with_uuid_generator<P: AsRef<Path>>(
        path: P,
        session_id: String,
        uuid_generator: UuidGenerator,
    ) -> Result<Self> {
        let file = OpenOptions::new().create_new(true).write(true).open(path)?;
        Ok(Self {
            file: BufWriter::new(file),
            session_id,
            next_sequence: 0,
            prev_hash: GENESIS_PREV_HASH,
            uuid_generator,
        })
    }

    /// Append an entry. Builds the canonical JSON-LD line, writes it +
    /// trailing newline, fsyncs, and advances the chain.
    pub fn append(&mut self, entry: Entry) -> Result<EntryRecord> {
        if entry.session_id != self.session_id {
            return Err(Error::SessionIdMismatch {
                expected: self.session_id.clone(),
                got: entry.session_id,
            });
        }

        for key in entry.payload.keys() {
            if RESERVED_KEYS.contains(&key.as_str()) {
                return Err(Error::PayloadKeyConflict(key.clone()));
            }
        }

        let entry_id = (self.uuid_generator)();
        let sequence = self.next_sequence;

        let mut obj = Map::new();
        obj.insert(
            "@context".to_string(),
            Value::String(LEDGER_CONTEXT.to_string()),
        );
        obj.insert("entryId".to_string(), Value::String(entry_id.to_string()));
        obj.insert(
            "sessionId".to_string(),
            Value::String(self.session_id.clone()),
        );
        obj.insert("sequenceNumber".to_string(), Value::Number(sequence.into()));
        obj.insert(
            "entryType".to_string(),
            serde_json::to_value(entry.entry_type)?,
        );
        obj.insert(
            "timestamp".to_string(),
            Value::String(entry.timestamp.to_rfc3339_opts(SecondsFormat::Nanos, true)),
        );
        obj.insert(
            "agentIdentityHash".to_string(),
            Value::String(hex::encode(entry.agent_identity_hash)),
        );
        obj.insert(
            "prevHash".to_string(),
            Value::String(hex::encode(self.prev_hash)),
        );

        for (k, v) in entry.payload {
            obj.insert(k, v);
        }

        // serde_json::Map is BTreeMap — sorted keys + no whitespace.
        let line = serde_json::to_string(&Value::Object(obj))?;

        self.file.write_all(line.as_bytes())?;
        self.file.write_all(b"\n")?;
        self.file.flush()?;
        self.file.get_ref().sync_all()?;

        let entry_hash = sha256(line.as_bytes());

        self.prev_hash = entry_hash;
        // u64 overflow at 2^64 entries — practically impossible, fail loud if reached.
        self.next_sequence = sequence + 1;

        Ok(EntryRecord {
            entry_id,
            sequence_number: sequence,
            entry_hash,
        })
    }

    /// Close the writer and return the chain root hash (the last entry's
    /// hash). Returns `GENESIS_PREV_HASH` if no entries were appended.
    pub fn close(mut self) -> Result<[u8; 32]> {
        self.file.flush()?;
        self.file.get_ref().sync_all()?;
        Ok(self.prev_hash)
    }

    /// Current chain head — the would-be `prev_hash` for the next entry.
    pub fn current_head(&self) -> [u8; 32] {
        self.prev_hash
    }

    /// Number of entries written so far.
    pub fn entry_count(&self) -> u64 {
        self.next_sequence
    }

    /// Session ID this writer is bound to. Used by typed event emitters
    /// (access log F4, reasoning step F5, …) to populate `Entry.session_id`
    /// without requiring callers to thread it separately.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(&hasher.finalize());
    out
}

/// Public helper: SHA-256 of arbitrary bytes. Useful for verifiers and
/// integration tests that want to recompute entry hashes from disk.
pub fn hash_line(bytes: &[u8]) -> [u8; 32] {
    sha256(bytes)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn entry_type_serializes_snake_case() {
        let v = serde_json::to_value(EntryType::SessionStart).unwrap();
        assert_eq!(v, Value::String("session_start".to_string()));
        let v = serde_json::to_value(EntryType::ApprovalTimedOut).unwrap();
        assert_eq!(v, Value::String("approval_timed_out".to_string()));
    }

    #[test]
    fn genesis_is_zero() {
        assert_eq!(GENESIS_PREV_HASH, [0u8; 32]);
    }

    #[test]
    fn hash_line_matches_sha256_of_input() {
        let input = b"hello";
        let h = hash_line(input);
        // SHA-256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
        assert_eq!(
            hex::encode(h),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }
}
