//! Trajectory Ledger verifier — the dual of the writer.
//!
//! Walks every line, recomputes `SHA-256(line[N])`, and checks that
//! `line[N+1].prevHash == that hash` (genesis-zero anchors line 0). Also
//! validates the v1 `@context`, monotonic sequence numbers, consistent
//! `sessionId`, and parseable `timestamp` fields — i.e. the same invariants
//! the writer holds when emitting.
//!
//! Used by the `aegis verify` CLI (issue #5) and by future runtime checks
//! (e.g. F9 self-audit at session close before ledger root is published).

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::{hash_line, GENESIS_PREV_HASH, LEDGER_CONTEXT};

/// Successful verification: chain intact, all entries well-formed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifySummary {
    /// Session ID shared by every entry (None for an empty ledger).
    pub session_id: Option<String>,
    pub entry_count: u64,
    /// Hex-encoded root hash (last entry's hash, or genesis if empty).
    pub root_hash_hex: String,
    pub first_timestamp: Option<DateTime<Utc>>,
    pub last_timestamp: Option<DateTime<Utc>>,
}

/// Concrete reason the chain is broken. `line` is the 0-indexed line in
/// the file at which the problem was first detected.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VerifyBreak {
    InvalidJson {
        line: u64,
        error: String,
    },
    MissingField {
        line: u64,
        field: String,
    },
    BadContext {
        line: u64,
        got: String,
    },
    SequenceMismatch {
        line: u64,
        expected: u64,
        got: u64,
    },
    PrevHashMismatch {
        line: u64,
        expected_hex: String,
        got_hex: String,
    },
    SessionIdMismatch {
        line: u64,
        expected: String,
        got: String,
    },
    BadTimestamp {
        line: u64,
        value: String,
    },
    BadHexField {
        line: u64,
        field: String,
        value: String,
    },
}

/// Top-level verify error: I/O failure (file unreadable) vs a chain break.
#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("ledger break at line {}", break_line(.0))]
    Break(VerifyBreak),
}

fn break_line(b: &VerifyBreak) -> u64 {
    match b {
        VerifyBreak::InvalidJson { line, .. }
        | VerifyBreak::MissingField { line, .. }
        | VerifyBreak::BadContext { line, .. }
        | VerifyBreak::SequenceMismatch { line, .. }
        | VerifyBreak::PrevHashMismatch { line, .. }
        | VerifyBreak::SessionIdMismatch { line, .. }
        | VerifyBreak::BadTimestamp { line, .. }
        | VerifyBreak::BadHexField { line, .. } => *line,
    }
}

/// Verify a ledger file. See [`verify_reader`] for invariants checked.
pub fn verify_file<P: AsRef<Path>>(path: P) -> Result<VerifySummary, VerifyError> {
    let f = File::open(path)?;
    verify_reader(BufReader::new(f))
}

/// Verify a ledger from any [`BufRead`]. Streams line-by-line; suitable
/// for very large ledgers without loading everything into memory.
pub fn verify_reader<R: BufRead>(mut reader: R) -> Result<VerifySummary, VerifyError> {
    let mut line_buf = String::new();
    let mut prev_hash = GENESIS_PREV_HASH;
    let mut next_seq: u64 = 0;
    let mut session_id: Option<String> = None;
    let mut first_ts: Option<DateTime<Utc>> = None;
    let mut last_ts: Option<DateTime<Utc>> = None;

    loop {
        line_buf.clear();
        let n = reader.read_line(&mut line_buf)?;
        if n == 0 {
            break;
        }
        let line_idx = next_seq;
        // The writer writes `line + b"\n"`; the bytes that were hashed are
        // exactly `line` (without the trailing LF).
        let line = line_buf.strip_suffix('\n').unwrap_or(&line_buf);

        let v: Value = serde_json::from_str(line).map_err(|e| {
            VerifyError::Break(VerifyBreak::InvalidJson {
                line: line_idx,
                error: e.to_string(),
            })
        })?;

        let ctx = required_str(&v, "@context", line_idx)?;
        if ctx != LEDGER_CONTEXT {
            return Err(VerifyError::Break(VerifyBreak::BadContext {
                line: line_idx,
                got: ctx.to_string(),
            }));
        }

        let sid = required_str(&v, "sessionId", line_idx)?;
        match &session_id {
            None => session_id = Some(sid.to_string()),
            Some(existing) if existing != sid => {
                return Err(VerifyError::Break(VerifyBreak::SessionIdMismatch {
                    line: line_idx,
                    expected: existing.clone(),
                    got: sid.to_string(),
                }));
            }
            _ => {}
        }

        let seq = v
            .get("sequenceNumber")
            .and_then(|x| x.as_u64())
            .ok_or_else(|| {
                VerifyError::Break(VerifyBreak::MissingField {
                    line: line_idx,
                    field: "sequenceNumber".to_string(),
                })
            })?;
        if seq != next_seq {
            return Err(VerifyError::Break(VerifyBreak::SequenceMismatch {
                line: line_idx,
                expected: next_seq,
                got: seq,
            }));
        }

        let prev_hex = required_str(&v, "prevHash", line_idx)?;
        let prev_bytes = hex::decode(prev_hex).map_err(|_| {
            VerifyError::Break(VerifyBreak::BadHexField {
                line: line_idx,
                field: "prevHash".to_string(),
                value: prev_hex.to_string(),
            })
        })?;
        if prev_bytes.as_slice() != prev_hash {
            return Err(VerifyError::Break(VerifyBreak::PrevHashMismatch {
                line: line_idx,
                expected_hex: hex::encode(prev_hash),
                got_hex: prev_hex.to_string(),
            }));
        }

        let ts_str = required_str(&v, "timestamp", line_idx)?;
        let ts: DateTime<Utc> = ts_str.parse::<DateTime<Utc>>().map_err(|_| {
            VerifyError::Break(VerifyBreak::BadTimestamp {
                line: line_idx,
                value: ts_str.to_string(),
            })
        })?;
        if first_ts.is_none() {
            first_ts = Some(ts);
        }
        last_ts = Some(ts);

        prev_hash = hash_line(line.as_bytes());
        next_seq = next_seq.saturating_add(1);
    }

    Ok(VerifySummary {
        session_id,
        entry_count: next_seq,
        root_hash_hex: hex::encode(prev_hash),
        first_timestamp: first_ts,
        last_timestamp: last_ts,
    })
}

fn required_str<'a>(v: &'a Value, field: &str, line: u64) -> Result<&'a str, VerifyError> {
    v.get(field)
        .and_then(|x| x.as_str())
        .ok_or_else(|| {
            VerifyError::Break(VerifyBreak::MissingField {
                line,
                field: field.to_string(),
            })
        })
}
