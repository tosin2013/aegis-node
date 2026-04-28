//! Decision: the answer to a single permission check.

use serde::{Deserialize, Serialize};

/// Result of a [`crate::Policy`] check. Allow lets the operation proceed;
/// Deny halts and (in the runtime) emits an `EntryType::Violation`;
/// RequireApproval routes through the F3 Human Approval Gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum Decision {
    Allow,
    Deny { reason: String },
    RequireApproval { reason: String },
}

impl Decision {
    pub fn is_allow(&self) -> bool {
        matches!(self, Decision::Allow)
    }

    pub fn is_deny(&self) -> bool {
        matches!(self, Decision::Deny { .. })
    }

    pub fn is_approval(&self) -> bool {
        matches!(self, Decision::RequireApproval { .. })
    }

    pub fn deny<S: Into<String>>(reason: S) -> Self {
        Decision::Deny {
            reason: reason.into(),
        }
    }

    pub fn approval<S: Into<String>>(reason: S) -> Self {
        Decision::RequireApproval {
            reason: reason.into(),
        }
    }
}

/// Network protocol kind, used by [`crate::Policy::check_network_outbound`].
/// Mirrors the schema's `networkPolicy.allowlist[].protocol` enum plus
/// "any" for callers that don't yet know the wire protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkProto {
    Http,
    Https,
    Tcp,
    Udp,
    Any,
}
