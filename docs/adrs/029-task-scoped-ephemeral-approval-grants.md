# 29. Task-Scoped Ephemeral Approval Grants (F3 Evolution for Multi-Turn)

**Status:** Proposed
**Date:** 2026-05-05
**Domain:** Approval gate (extends [ADR-005](005-human-approval-gate-for-sensitive-actions.md), supports [ADR-025](025-multi-turn-agent-loop-with-triple-bound-circuit-breaker.md))
**Related research:** [docs/research/multi-turn-agent-loop.md](../research/multi-turn-agent-loop.md) §"Group E"

## Context

ADR-005 established F3's human approval gate (TTY / file / web /
mTLS+SPIFFE channels) for sensitive actions in single-pass execution.
The model says "I want to call $TOOL with $ARGS"; the operator says
yes or no; the dispatch happens or doesn't.

Multi-turn execution exposes failure modes:

- **Approval fatigue.** A coding agent doing a refactor might issue
  40 sequential shell commands. Prompting for approval 40 times
  causes operators to mechanically click "approve" without reading
  payloads. Studies of human-AI teaming consistently show that
  high-frequency approval requests degrade into rubber-stamping
  within ~10 minutes of session start. The system *appears* safe in
  logs but provides zero actual oversight.
- **Implicit grant inflation.** "I approved a database write at
  turn 2; do later turns also have write authority?" If yes, an
  attacker who hijacks the agent later in the session inherits
  unlimited write privileges. If no, the operator is back to
  approving every call.
- **Argument drift.** An operator approves
  `database.execute("UPDATE users SET tier='gold' WHERE id=42")` at
  turn 2. Turn 5 tries `database.execute("UPDATE users SET tier='gold'")`
  (no WHERE). Same tool, very different impact.
- **Headless asynchronous flows.** The mTLS+SPIFFE channel is
  designed for automated approvers. A multi-turn loop that hits the
  approval gate must serialize state, await an external decision,
  and resume cleanly — without leaking secrets if the run host is
  compromised mid-suspension.

[ADR-007](007-pre-execution-reasoning-trajectory.md)'s reasoning step
already captures *why* the agent wants to do something. ADR-029
addresses *how* the resulting human (or policy-driven) approval is
scoped, expires, and re-prompts.

## Decision

**Issue Task-Scoped Ephemeral Grants on every approval. A grant
binds a cryptographic token to the exact `(tool_name, arg_hash)`
tuple for a configurable TTL. Identical retries within the TTL
auto-consume the grant without prompting; any argument drift voids
the grant and forces re-approval. Approval scope is tiered
(Advisory / Validating / Blocking / Escalating) to match risk.**

### Grant token shape

```rust
pub struct ApprovalGrant {
    grant_id: Uuid,                  // unique per grant; logged in F9
    issued_at: SystemTime,           // for TTL accounting
    ttl: Duration,                   // configurable; default 5 minutes
    bound_tool_class: ToolClass,     // filesystem | network | mcp | exec
    bound_tool_name: String,         // e.g. "database.execute"
    bound_arg_hash: [u8; 32],        // sha256 of canonical arg JSON
    decided_by: ApprovalIdentity,    // SPIFFE ID + channel
    decision: Decision,              // Allow | Deny | Escalate
    signature: Signature,            // F1-issued signature over the above
}
```

Grants are stored in the `Session`'s in-memory grant table; on
session end they vaporize. They're recorded in the F9 ledger as
`approval_decision` entries (per [ADR-026](026-hierarchical-per-turn-ledger-protocol.md))
so the chain of who-approved-what is auditable forever even though
the grant itself doesn't outlive the session.

### Auto-consumption rules

When a tool dispatch reaches the F3 gate at turn N:

1. Compute `arg_hash = sha256(canonical_json(args))`.
2. Query the grant table for `(tool_name, arg_hash)` with `now <
   issued_at + ttl`.
3. If a matching `Allow` grant exists: consume silently (no operator
   prompt), emit an `approval_decision` ledger entry with
   `decision: AutoConsumed, source_grant: <grant_id>`, dispatch.
4. If a matching `Deny` grant exists within TTL: short-circuit deny
   with the original deny reason cached on the grant.
5. Otherwise: surface the approval prompt on the configured channel,
   await decision, issue a fresh grant, dispatch (or deny).

Argument drift voids the match: `database.execute("UPDATE foo SET
x=1 WHERE id=42")` and `database.execute("UPDATE foo SET x=1")` hash
differently, so the second call surfaces a fresh approval prompt
even within TTL.

### Risk-tiered approval scopes

The manifest's existing approval section grows a per-tool tier:

```yaml
approval:
  channel: "tty"   # or "file", "web", "mtls"
  tools:
    filesystem.write:
      tier: validating         # human sign-off required
      grant_ttl_seconds: 300
    network.outbound:
      tier: advisory           # log only, don't block (read-only services)
      grant_ttl_seconds: 0
    exec:
      tier: blocking           # halt on policy failure; no approval path
    database.execute:
      tier: escalating         # route to a higher-authority queue
      grant_ttl_seconds: 60
      escalation_channel: "mtls+webhook"
```

| Tier | Behavior |
|---|---|
| `advisory` | Log to ledger, dispatch immediately. No prompt. For low-risk read-only actions. |
| `validating` | Standard approval prompt on the configured channel. Required default for write/exec. |
| `blocking` | Hard deny with no approval path. For tools the manifest reaches but the operator never wants the agent to use under any circumstances. (Why declare them at all? So the agent can *see* the tool exists in its catalog and explain why it can't use it, instead of hallucinating capabilities.) |
| `escalating` | Routes the approval prompt to a secondary channel (e.g., mTLS+webhook to a senior engineer) with the originating context. For high-financial-impact or schema-mutating actions. |

### Session pause + resume for headless approvals

The mTLS+SPIFFE channel approves asynchronously. When a turn hits an
approval prompt on that channel:

1. The session **suspends** — its state (manifest, model digest,
   context window, accumulator, partial ledger) is serialized to a
   per-session sidecar file.
2. The CLI exits with a `PendingApproval` status code and the path
   to the sidecar.
3. The approver (out-of-band) signs an approval token and writes it
   to a configured filesystem location (or POSTs it to the runtime's
   listening webhook in long-running deployments).
4. The CLI is resumed with `aegis run --resume <session-id>`. The
   sidecar is loaded, the approval token is verified against the
   F1-trusted CA, the grant is issued, and the loop continues at the
   next turn boundary.

This is opt-in via the manifest's
`approval.channel: "mtls"` setting. TTY / file / web channels remain
synchronous and don't suspend.

### Visibility into aggregate state

When the gate prompts the operator, the prompt includes the current
[ADR-027](027-aggregate-quota-schema.md) accumulator state. Example
TTY prompt:

```text
APPROVAL REQUIRED: filesystem.write
  path: /tmp/output/report.md
  bytes: 4096

Session budget so far:
  filesystem.write — 2/5 calls, 8192/100000 bytes
  network.outbound — 0/50 calls

Approve? [y/N/e=escalate]
```

The operator can intelligently consent in context rather than blindly
clicking yes.

## Why not the alternatives

- **Per-call approval forever, no grants.** What we have today.
  Documented above as the approval-fatigue failure mode. Rejected.
- **Session-wide grants (one approval covers all calls of a tool).**
  Inverts the security posture — one yes at turn 2 grants unlimited
  rights through turn 50. Rejected.
- **Per-tool grants but no argument hashing.** An attacker who
  hijacks the agent at turn 3 can mutate the args ("delete *" instead
  of "delete /tmp/foo") and inherit the original turn-2 grant.
  Rejected.
- **Time-window only (no argument hash).** Simpler but same problem:
  the TTL becomes an attacker's window of opportunity for argument
  drift. The argument hash is the load-bearing scope.
- **Cryptographic capability tokens (object capabilities) instead of
  grants.** Object-capability runtimes (e.g., the original E
  language) are the formally-correct solution but require redesigning
  the entire dispatch path around capability passing. Out of scope
  for v1.0.0; the grant table is a pragmatic step in that direction.

## Implementation tracking

- Crate: `crates/approval-gate/src/grants.rs` (new — grant table + 
  TTL logic), `crates/approval-gate/src/channels.rs` (extend with
  pause/resume), `crates/cli/src/run.rs` (resume mode handling),
  `crates/policy/src/manifest.rs` (tier schema).
- Cross-language conformance: Go validator must understand the new
  `tier` field; Rust enforcer must agree on grant matching semantics.
- F8 replay viewer: render `approval_decision` entries with grant
  visibility (which calls fired without a re-prompt because they
  matched an earlier grant).
- Tracking issue: see v1.0.0 milestone tracker.

## Open questions for follow-up

- **Sidecar serialization format.** Sessions in pause hold context
  windows, which may include raw tool-result payloads (e.g., file
  contents). Should the sidecar encrypt at rest using a key
  bound to the runtime's SPIFFE identity? Lean yes — protects
  pending approvals on a multi-tenant host.
- **Parallel tool calls during pending approval.** If a turn emits
  three tool calls and call #2 needs approval, can calls #1 and #3
  proceed in parallel? Lean toward "no, sequence the turn" for
  simplicity; revisit in v1.x when the inference engine grows
  parallel dispatch primitives.
- **Grant revocation.** Does an operator have a way to revoke an
  outstanding grant before its TTL expires? Useful for "I just
  realized that approval was wrong." Lean yes — `aegis revoke
  <session-id> <grant-id>` writes a `grant_revoked` ledger entry
  that the dispatch path checks at lookup time.

## References

- Research brief: [docs/research/multi-turn-agent-loop.md](../research/multi-turn-agent-loop.md) §"Group E — Approval gate scope across turns"
- [ADR-005](005-human-approval-gate-for-sensitive-actions.md) F3 baseline
- HashiCorp Vault dynamic-secrets lease model
- AWS STS short-lived session credentials
- Object-capability literature (E language, Amber, Pony)
- Human-AI teaming research on approval fatigue
