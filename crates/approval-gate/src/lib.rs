//! Aegis-Node F3 human approval gate (per ADR-005, issue #27).
//!
//! Phase 1a ships two channels:
//!
//! - **TTY**: when stdin is attached, prompt the operator and read y/n.
//!   The prompt has a configurable timeout; reading runs on a worker
//!   thread so the main runtime can enforce the deadline.
//! - **File**: poll a JSON file at `AEGIS_APPROVAL_FILE` (or any path)
//!   for a `{"decision": "granted"|"rejected", ...}` blob. Used by
//!   conformance harnesses and by automation that doesn't have a tty.
//!
//! The localhost web UI ([#35](https://github.com/tosin2013/aegis-node/issues/35))
//! and signed-API mTLS channel ([#36](https://github.com/tosin2013/aegis-node/issues/36))
//! plug in as additional `ApprovalChannel` implementations later.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod file;
pub mod mtls;
pub mod tty;
pub mod web;

pub use file::FileApprovalChannel;
pub use mtls::MtlsApprovalChannel;
pub use tty::TtyApprovalChannel;
pub use web::WebApprovalChannel;

/// One approval request the runtime hands to a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// Human-readable summary the channel shows the approver.
    pub action_summary: String,
    /// The resource the agent wants to act on.
    pub resource_uri: String,
    /// `read` / `write` / `delete` / `network_outbound` / `exec` / etc.
    pub access_type: String,
    /// Session identifier for cross-referencing the ledger.
    pub session_id: String,
    /// Optional reasoning-step ID for cross-referencing F5 entries.
    #[serde(default)]
    pub reasoning_step_id: Option<String>,
    /// How long the channel may wait before declaring a timeout.
    pub timeout: Duration,
}

/// What the channel returned. Maps directly onto the proto's
/// `ApprovalResponse` shape (per ADR-005 / aegis.v1.proto).
#[derive(Debug, Clone)]
pub enum ApprovalOutcome {
    Granted {
        approver_identity: String,
        decided_at: DateTime<Utc>,
    },
    Rejected {
        reason: String,
        decided_at: DateTime<Utc>,
    },
    TimedOut {
        expired_at: DateTime<Utc>,
    },
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("malformed approval response: {0}")]
    Malformed(String),

    #[error("serialization: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("channel: {0}")]
    Channel(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Approval channel trait. Each call is request/response — no streaming.
/// Implementations are responsible for honoring `req.timeout` and
/// returning `ApprovalOutcome::TimedOut` when it elapses.
pub trait ApprovalChannel: Send {
    fn request_approval(&mut self, req: &ApprovalRequest) -> Result<ApprovalOutcome>;
}

/// Default approval timeout per ADR-005's "Default approval timeout
/// (120 s) and refuse-on-timeout semantics" — though Phase 1a callers
/// can configure via `ApprovalRequest::timeout`. 60s is the issue
/// description's default; 120s the broader ADR target. We default
/// to 60s here and let the runtime override.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
