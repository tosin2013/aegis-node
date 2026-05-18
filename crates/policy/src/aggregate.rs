//! Per-session aggregate quota accumulator (ADR-027 / F2 extension).
//!
//! The F2 [Permission Manifest][crate::Manifest] gates each tool call
//! against static per-call rules. Aggregate quota adds a *cumulative*
//! evaluation across the session: even if every individual call is
//! permitted, the aggregate may deny once the agent has exhausted its
//! budget for a tool class.
//!
//! Failure mode this prevents: OWASP Agentic Top 10 T10
//! (excessive agency / over-privilege). Per the ADR's §"Context",
//! observed exploits include 10,000-iteration filesystem-read loops
//! that look fine per-call but exfiltrate the whole directory in
//! aggregate. Per-call enforcement says "allowed"; aggregate says
//! "exfil."
//!
//! ## Scope (foundation PR)
//!
//! - Tracks `max_calls_per_session` per tool class (Filesystem,
//!   Network, Exec, and *per server name* for MCP).
//! - Returns `AggregateCapExceeded` on the (N+1)th call against a
//!   class whose quota is exhausted. The mediator translates that
//!   to a `Denied` outcome the model sees on its next turn.
//! - Counters reset at session start ([`SessionAggregateState::new`]).
//!   There is no cross-session quota state per ADR-027.
//!
//! ## Deferred (called out in PR #196 body)
//!
//! - Byte-counter quotas (`max_bytes_*_per_session`) — require
//!   threading byte counts back from each mediator.
//! - Per-tool MCP quotas (`max_calls_per_tool_per_session`) — the
//!   foundation only counts per-server, not per-tool.
//! - Go validator parity + cross-language conformance fixtures.
//! - F10 `aegis validate` lint rules ("warn if no quota on a network
//!   outbound allowlist" etc.).
//! - Sliding-window / replenishment quotas (`quota.window_seconds`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::manifest::AggregateQuota;

/// Which tool class a dispatch counts against. MCP is per-server name
/// because each [`crate::manifest::McpServerGrant`] carries its own
/// `quota` — a cap on `fs-mcp` should not limit calls to `search-mcp`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "name")]
pub enum ToolClass {
    Filesystem,
    Network,
    Exec,
    Mcp(String),
}

impl ToolClass {
    /// Stable string label used on ledger payloads and snapshot
    /// rendering. Matches the JSON tag form serde emits.
    pub fn label(&self) -> String {
        match self {
            ToolClass::Filesystem => "filesystem".to_string(),
            ToolClass::Network => "network".to_string(),
            ToolClass::Exec => "exec".to_string(),
            ToolClass::Mcp(server) => format!("mcp:{server}"),
        }
    }
}

/// Returned by [`SessionAggregateState::check_and_increment`] when the
/// caller's call would breach the class's `max_calls_per_session` cap.
/// Carries the observed count (pre-increment) and the cap so the
/// mediator can compose a violation reason string + ledger payload.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error(
    "aggregate cap exceeded: class={} observed={observed} cap={cap}",
    class.label()
)]
pub struct AggregateCapExceeded {
    pub class: ToolClass,
    pub bound: &'static str,
    pub observed: u64,
    pub cap: u64,
}

/// One per-class snapshot — the tuple that lands in
/// `turn_end.quotaSnapshots[]` (ADR-026) so auditors can chart
/// budget consumption across the session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuotaSnapshot {
    pub class: String,
    pub calls: u64,
    /// Cap declared on the manifest for this class, when present.
    /// `None` means "no aggregate cap configured" — the counter still
    /// runs, but no breach is possible.
    pub max_calls_per_session: Option<u64>,
}

/// Session-scoped accumulator. One per [`crate::Policy`] / `Session`.
/// Cheap to clone but the typical wiring keeps it `&mut self`-borrowed
/// inside the `Session`.
#[derive(Debug, Clone, Default)]
pub struct SessionAggregateState {
    counts: HashMap<ToolClass, u64>,
}

impl SessionAggregateState {
    /// Fresh accumulator — zero counts in every class.
    pub fn new() -> Self {
        Self {
            counts: HashMap::new(),
        }
    }

    /// Current observed call count for `class`. Returns 0 if the class
    /// has never been dispatched.
    pub fn snapshot(&self, class: &ToolClass) -> u64 {
        self.counts.get(class).copied().unwrap_or(0)
    }

    /// Check whether one more dispatch in `class` would breach
    /// `quota.max_calls_per_session`. On breach: return
    /// [`AggregateCapExceeded`] (counter NOT incremented — the call is
    /// refused, never dispatched). On pass: increment the counter and
    /// return the new value.
    ///
    /// The "check first, increment if allowed" pattern means a denied
    /// call doesn't burn budget against future ones. ADR-027 §"Runtime
    /// accumulator" says "increments before dispatch"; we read that as
    /// "before the syscall actually runs" which is the same window as
    /// "after the quota gate accepts."
    pub fn check_and_increment(
        &mut self,
        class: ToolClass,
        quota: Option<&AggregateQuota>,
    ) -> Result<u64, AggregateCapExceeded> {
        let current = self.snapshot(&class);
        if let Some(q) = quota {
            if let Some(cap) = q.max_calls_per_session {
                if current >= cap {
                    return Err(AggregateCapExceeded {
                        class,
                        bound: "max_calls_per_session",
                        observed: current,
                        cap,
                    });
                }
            }
        }
        let next = current.saturating_add(1);
        self.counts.insert(class, next);
        Ok(next)
    }

    /// Render the accumulator state for ledger emission. The order is
    /// stable across calls (sorted by class label) so two consecutive
    /// `turn_end` entries that report the same counts produce
    /// byte-identical `quotaSnapshots[]` arrays — important for F8
    /// replay determinism.
    pub fn snapshots(&self, manifest: &crate::Manifest) -> Vec<QuotaSnapshot> {
        let mut classes: Vec<ToolClass> = self.counts.keys().cloned().collect();
        // Include classes that have a quota declared but haven't been
        // dispatched yet — auditors want to see the budget even when
        // utilization is zero.
        for declared in declared_classes(manifest) {
            if !classes.contains(&declared) {
                classes.push(declared);
            }
        }
        classes.sort_by_key(|c| c.label());
        classes
            .into_iter()
            .map(|c| QuotaSnapshot {
                calls: self.snapshot(&c),
                max_calls_per_session: quota_for(manifest, &c)
                    .and_then(|q| q.max_calls_per_session),
                class: c.label(),
            })
            .collect()
    }
}

/// Resolve the `AggregateQuota` block for a given tool class from a
/// parsed manifest. Returns `None` when the class has no quota
/// declared (in which case no aggregate cap applies).
pub fn quota_for<'m>(
    manifest: &'m crate::Manifest,
    class: &ToolClass,
) -> Option<&'m AggregateQuota> {
    match class {
        ToolClass::Filesystem => manifest.tools.filesystem.as_ref()?.quota.as_ref(),
        ToolClass::Network => manifest.tools.network.as_ref()?.quota.as_ref(),
        ToolClass::Exec => manifest.tools.exec.as_ref()?.quota.as_ref(),
        ToolClass::Mcp(server_name) => manifest
            .tools
            .mcp
            .iter()
            .find(|g| &g.server_name == server_name)?
            .quota
            .as_ref(),
    }
}

/// Every tool class that has *any* quota field declared on the
/// manifest. Used by [`SessionAggregateState::snapshots`] so auditors
/// see declared budgets even before any call lands.
fn declared_classes(manifest: &crate::Manifest) -> Vec<ToolClass> {
    let mut out = Vec::new();
    if manifest
        .tools
        .filesystem
        .as_ref()
        .and_then(|f| f.quota.as_ref())
        .is_some()
    {
        out.push(ToolClass::Filesystem);
    }
    if manifest
        .tools
        .network
        .as_ref()
        .and_then(|n| n.quota.as_ref())
        .is_some()
    {
        out.push(ToolClass::Network);
    }
    if manifest
        .tools
        .exec
        .as_ref()
        .and_then(|e| e.quota.as_ref())
        .is_some()
    {
        out.push(ToolClass::Exec);
    }
    for grant in &manifest.tools.mcp {
        if grant.quota.is_some() {
            out.push(ToolClass::Mcp(grant.server_name.clone()));
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::manifest::AggregateQuota;

    fn cap(n: u64) -> AggregateQuota {
        AggregateQuota {
            max_calls_per_session: Some(n),
        }
    }

    #[test]
    fn check_and_increment_passes_under_cap() {
        let mut s = SessionAggregateState::new();
        let q = cap(3);
        assert_eq!(
            s.check_and_increment(ToolClass::Filesystem, Some(&q))
                .unwrap(),
            1
        );
        assert_eq!(
            s.check_and_increment(ToolClass::Filesystem, Some(&q))
                .unwrap(),
            2
        );
        assert_eq!(
            s.check_and_increment(ToolClass::Filesystem, Some(&q))
                .unwrap(),
            3
        );
        assert_eq!(s.snapshot(&ToolClass::Filesystem), 3);
    }

    #[test]
    fn check_and_increment_denies_over_cap() {
        let mut s = SessionAggregateState::new();
        let q = cap(2);
        s.check_and_increment(ToolClass::Network, Some(&q)).unwrap();
        s.check_and_increment(ToolClass::Network, Some(&q)).unwrap();
        let err = s
            .check_and_increment(ToolClass::Network, Some(&q))
            .unwrap_err();
        assert_eq!(err.class, ToolClass::Network);
        assert_eq!(err.bound, "max_calls_per_session");
        assert_eq!(err.observed, 2);
        assert_eq!(err.cap, 2);
        // Denied call did NOT advance the counter — it's still 2.
        assert_eq!(s.snapshot(&ToolClass::Network), 2);
    }

    #[test]
    fn absent_quota_never_denies() {
        let mut s = SessionAggregateState::new();
        for _ in 0..10_000 {
            s.check_and_increment(ToolClass::Filesystem, None).unwrap();
        }
        assert_eq!(s.snapshot(&ToolClass::Filesystem), 10_000);
    }

    #[test]
    fn classes_are_independent() {
        let mut s = SessionAggregateState::new();
        let q = cap(1);
        s.check_and_increment(ToolClass::Filesystem, Some(&q))
            .unwrap();
        // Network class has its own counter — not blocked by filesystem.
        s.check_and_increment(ToolClass::Network, Some(&q)).unwrap();
        // But a *second* filesystem call IS blocked.
        let err = s
            .check_and_increment(ToolClass::Filesystem, Some(&q))
            .unwrap_err();
        assert!(matches!(err.class, ToolClass::Filesystem));
    }

    #[test]
    fn mcp_classes_are_per_server() {
        let mut s = SessionAggregateState::new();
        let q = cap(1);
        s.check_and_increment(ToolClass::Mcp("fs-mcp".into()), Some(&q))
            .unwrap();
        // Cap on fs-mcp doesn't limit calls to search-mcp.
        s.check_and_increment(ToolClass::Mcp("search-mcp".into()), Some(&q))
            .unwrap();
        // Second fs-mcp call IS blocked.
        let err = s
            .check_and_increment(ToolClass::Mcp("fs-mcp".into()), Some(&q))
            .unwrap_err();
        assert_eq!(err.class, ToolClass::Mcp("fs-mcp".into()));
    }
}
