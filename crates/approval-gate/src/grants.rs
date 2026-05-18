//! Task-scoped ephemeral approval grants (ADR-029).
//!
//! When an operator approves a tool dispatch via the F3 gate, the
//! runtime issues an `ApprovalGrant` bound to the **exact**
//! `(tool_name, sha256(canonical_args))` tuple for a TTL. Identical
//! retries within that window auto-consume the grant without
//! re-prompting — the auditor's view is "one approval, N identical
//! calls" rather than "approval fatigue from N prompts."
//!
//! Argument drift voids the match: `database.execute("UPDATE users
//! SET tier='gold' WHERE id=42")` and the same SQL without the WHERE
//! clause hash differently, so the second call surfaces a fresh
//! prompt.
//!
//! ## Scope (foundation PR)
//!
//! - Auto-consume on identical `(tool_name, arg_hash)` within TTL.
//! - Grants vaporize at session end (in-memory only, per ADR-029
//!   §"Grant token shape").
//! - `Decision::Allow` and `Decision::Deny` grants both recognized;
//!   denied grants short-circuit subsequent identical retries with
//!   the original deny reason.
//!
//! ## Deferred (called out in PR body)
//!
//! - Cryptographic signature over the grant (ADR-029's `signature` field).
//!   Foundation stores the grant in-memory only; signing is needed
//!   when grants persist across processes (pause/resume work).
//! - Grant revocation (`aegis revoke <session-id> <grant-id>`).
//! - F8 replay viewer rendering of grant lineage.

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// One approval grant in the in-memory session grant table. Carries
/// enough state to reproduce the approval decision on every
/// auto-consume — both the bound tuple and the decision payload (so
/// denied grants short-circuit identical retries without re-asking).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalGrant {
    /// UUIDv7 — referenced from `approval_decision` ledger entries
    /// (`source_grant` field) so auditors can correlate the original
    /// prompt with every retry it covered.
    pub grant_id: Uuid,
    /// Wallclock at which the operator decided. TTL accounting uses
    /// this anchor; replays of the ledger reconstruct the same
    /// decision history.
    pub issued_at: SystemTime,
    /// How long the grant remains valid. Configurable per tool class
    /// via the manifest (ADR-029 §"Risk-tiered approval scopes").
    pub ttl: Duration,
    /// Bound tool name in `<namespace>__<tool>` form, exactly as the
    /// model emitted it. Argument drift on retries is caught by the
    /// `arg_hash` field; tool drift is caught here.
    pub bound_tool_name: String,
    /// SHA-256 of the canonical-JSON serialization of the args the
    /// model passed at the time of the original prompt. Auto-consume
    /// requires an exact match; any drift forces a fresh prompt.
    pub bound_arg_hash: [u8; 32],
    /// What the operator (or policy) decided.
    pub decision: GrantDecision,
}

/// Decision payload on a grant. Mirrors `Decision::Allow` /
/// `Decision::Deny` from `aegis-policy` but kept local to the
/// approval-gate crate to avoid a circular dependency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GrantDecision {
    /// Operator (or policy tier) approved the action. Identical
    /// retries within TTL dispatch silently.
    Allow,
    /// Operator (or policy tier) rejected the action. Identical
    /// retries within TTL short-circuit with the cached reason rather
    /// than re-asking — defends against "ask until you get a yes."
    Deny { reason: String },
}

impl ApprovalGrant {
    /// Build an `Allow` grant for `(tool_name, args)` valid for `ttl`
    /// from now. Computes the canonical arg hash internally.
    pub fn allow(tool_name: impl Into<String>, args: &serde_json::Value, ttl: Duration) -> Self {
        Self {
            grant_id: Uuid::now_v7(),
            issued_at: SystemTime::now(),
            ttl,
            bound_tool_name: tool_name.into(),
            bound_arg_hash: canonical_arg_hash(args),
            decision: GrantDecision::Allow,
        }
    }

    /// Build a `Deny` grant. Useful when an operator explicitly
    /// rejects — subsequent identical retries within TTL are
    /// short-circuited without re-asking.
    pub fn deny(
        tool_name: impl Into<String>,
        args: &serde_json::Value,
        ttl: Duration,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            grant_id: Uuid::now_v7(),
            issued_at: SystemTime::now(),
            ttl,
            bound_tool_name: tool_name.into(),
            bound_arg_hash: canonical_arg_hash(args),
            decision: GrantDecision::Deny {
                reason: reason.into(),
            },
        }
    }

    /// `true` when `now < issued_at + ttl`. Wallclock-anchored — a
    /// hung process or paused session won't extend the grant's life.
    pub fn is_live_at(&self, now: SystemTime) -> bool {
        match now.duration_since(self.issued_at) {
            Ok(elapsed) => elapsed < self.ttl,
            Err(_) => true, // clock went backward; treat as still-fresh
        }
    }

    /// Hex-encoded arg hash for ledger emission convenience.
    pub fn arg_hash_hex(&self) -> String {
        hex::encode(self.bound_arg_hash)
    }
}

/// SHA-256 of the canonical JSON serialization of `args`. Since
/// `serde_json::Map` is `BTreeMap`-backed (the workspace does not enable
/// `preserve_order`), `serde_json::to_string` produces sorted-keys
/// no-whitespace output — byte-deterministic across runs and across
/// reimplementations of the hash in other languages.
pub fn canonical_arg_hash(args: &serde_json::Value) -> [u8; 32] {
    let canonical = serde_json::to_string(args).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&hasher.finalize());
    out
}

/// Session-scoped grant table. Keyed by `(tool_name, arg_hash)` so a
/// lookup is O(1) and only requires hashing the incoming args once.
/// Reset on session start; grants vaporize when the session ends.
#[derive(Debug, Default)]
pub struct SessionGrantTable {
    grants: HashMap<(String, [u8; 32]), ApprovalGrant>,
}

impl SessionGrantTable {
    pub fn new() -> Self {
        Self {
            grants: HashMap::new(),
        }
    }

    /// Look up a live grant for `(tool_name, args)`. Returns
    /// `Some(&ApprovalGrant)` only when a grant exists AND its TTL is
    /// not yet exhausted at `now`. Expired grants are left in the
    /// table for the next [`Self::insert`] to overwrite — pruning
    /// during lookup would require `&mut self`, and that's friction
    /// the mediator doesn't need.
    pub fn lookup(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
        now: SystemTime,
    ) -> Option<&ApprovalGrant> {
        let key = (tool_name.to_string(), canonical_arg_hash(args));
        self.grants.get(&key).filter(|g| g.is_live_at(now))
    }

    /// Store a freshly-issued grant. Overwrites any prior grant for
    /// the same `(tool_name, arg_hash)` — a subsequent operator
    /// decision supersedes a stale one in the same window.
    pub fn insert(&mut self, grant: ApprovalGrant) {
        let key = (grant.bound_tool_name.clone(), grant.bound_arg_hash);
        self.grants.insert(key, grant);
    }

    /// Number of grants currently in the table. Used by tests and
    /// future `aegis ledger inspect` tooling.
    pub fn len(&self) -> usize {
        self.grants.len()
    }

    /// Whether the table holds zero grants.
    pub fn is_empty(&self) -> bool {
        self.grants.is_empty()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn canonical_hash_stable_across_key_orders() {
        // serde_json::Map is BTreeMap-backed, so re-orderings of input
        // keys produce identical canonical JSON.
        let a = serde_json::json!({"path": "/tmp/a", "bytes": 4});
        let b = serde_json::json!({"bytes": 4, "path": "/tmp/a"});
        assert_eq!(canonical_arg_hash(&a), canonical_arg_hash(&b));
    }

    #[test]
    fn arg_drift_changes_hash() {
        let a = serde_json::json!({"path": "/tmp/a"});
        let b = serde_json::json!({"path": "/tmp/b"});
        assert_ne!(canonical_arg_hash(&a), canonical_arg_hash(&b));
    }

    #[test]
    fn lookup_returns_live_grant() {
        let mut t = SessionGrantTable::new();
        let args = serde_json::json!({"path": "/tmp/x"});
        let grant = ApprovalGrant::allow("filesystem__read", &args, Duration::from_secs(60));
        t.insert(grant);

        let now = SystemTime::now();
        let hit = t.lookup("filesystem__read", &args, now);
        assert!(hit.is_some(), "live grant should be returned");
        assert!(matches!(hit.unwrap().decision, GrantDecision::Allow));
    }

    #[test]
    fn lookup_misses_on_arg_drift() {
        let mut t = SessionGrantTable::new();
        let args = serde_json::json!({"path": "/tmp/x"});
        t.insert(ApprovalGrant::allow(
            "filesystem__read",
            &args,
            Duration::from_secs(60),
        ));
        let drifted = serde_json::json!({"path": "/tmp/y"});
        assert!(t
            .lookup("filesystem__read", &drifted, SystemTime::now())
            .is_none());
    }

    #[test]
    fn lookup_misses_on_tool_drift() {
        let mut t = SessionGrantTable::new();
        let args = serde_json::json!({"path": "/tmp/x"});
        t.insert(ApprovalGrant::allow(
            "filesystem__read",
            &args,
            Duration::from_secs(60),
        ));
        assert!(t
            .lookup("filesystem__write", &args, SystemTime::now())
            .is_none());
    }

    #[test]
    fn expired_grant_is_not_returned() {
        let mut t = SessionGrantTable::new();
        let args = serde_json::json!({"path": "/tmp/x"});
        let mut grant = ApprovalGrant::allow("filesystem__read", &args, Duration::from_millis(1));
        // Backdate so the grant is already expired.
        grant.issued_at = SystemTime::now() - Duration::from_secs(60);
        t.insert(grant);

        let now = SystemTime::now();
        assert!(t.lookup("filesystem__read", &args, now).is_none());
    }

    #[test]
    fn deny_grant_short_circuits_with_reason() {
        let args = serde_json::json!({"program": "rm"});
        let grant = ApprovalGrant::deny(
            "exec__run",
            &args,
            Duration::from_secs(60),
            "operator declined",
        );
        match &grant.decision {
            GrantDecision::Deny { reason } => assert_eq!(reason, "operator declined"),
            _ => panic!("expected Deny"),
        }
    }
}
