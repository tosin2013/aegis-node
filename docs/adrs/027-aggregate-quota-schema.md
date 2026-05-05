# 27. Per-Session Aggregate Quota Schema for the Permission Manifest

**Status:** Proposed
**Date:** 2026-05-05
**Domain:** Manifest / policy (extends [ADR-004](004-declarative-yaml-permission-manifest.md), supports [ADR-025](025-multi-turn-agent-loop-with-triple-bound-circuit-breaker.md))
**Related research:** [docs/research/multi-turn-agent-loop.md](../research/multi-turn-agent-loop.md) §"Group C"

## Context

The F2 Permission Manifest evaluates each tool call against a static
policy: is *this* file path inside `tools.filesystem.read`? Is *this*
host:port in `tools.network.outbound.allowlist`? Per-call enforcement
is correct but not sufficient under multi-turn execution.

Cumulative effect across turns is the failure mode. Examples observed
in production agent postmortems:

- Filesystem read is allowed for `/data/`. An attacker injects a
  prompt via [ADR-028](028-adversarial-pre-filter-gate.md)'s flagged
  payload, and the agent loops 10,000 times reading one file each
  turn. Every individual call passes the manifest. The aggregate is
  full-directory exfiltration.
- Network outbound allows `api.example.com:443`. Agent loops sending
  small DNS queries that encode bytes from a sensitive bucket as
  subdomains. Every individual call passes. The aggregate is
  bandwidth exfiltration via DNS.
- Amazon Bedrock AgentCore Code Interpreter publicly disclosed
  vulnerability: an agent in "no network" sandbox mode could still
  emit DNS queries; chained over many calls, that became data
  exfiltration. The per-call check said "allowed"; aggregate behavior
  said "exfil."

OWASP Agentic Top 10 names this T10 (excessive agency / over-privilege).
The mitigation requires policy that evaluates **cumulative** state
across the session, not just per-call.

## Decision

**Extend `tools.*` in the manifest to optionally declare per-class
aggregate quotas. The runtime maintains an in-memory per-session
accumulator; every gate dispatch evaluates the call against both the
existing per-call rules AND the cumulative budget. Any cap breach
returns `Denied`, halts the call, and emits an `AggregateCapExceeded`
violation to the F9 ledger.**

### Schema extension

`tools.<class>.quota` is a new optional sub-object. The shapes that
make sense per class:

```yaml
tools:
  filesystem:
    read:
      - "/data/"
    write:
      - "/tmp/output/"
    quota:
      max_calls_per_session: 100
      max_bytes_read_per_session: 50_000_000      # 50 MB
      max_bytes_written_per_session: 10_000_000   # 10 MB

  network:
    outbound:
      allowlist:
        - { host: "api.example.com", port: 443, protocol: https }
    quota:
      max_calls_per_session: 50
      max_bytes_uploaded_per_session: 1_000_000   # 1 MB
      max_bytes_downloaded_per_session: 50_000_000

  mcp:
    - server_name: "fs-mcp"
      server_uri: "stdio:..."
      allowed_tools: [...]
      quota:
        max_calls_per_session: 50
        max_calls_per_tool_per_session:
          read_text_file: 30
          list_directory: 5

  exec:
    grants:
      - { binary: "rg", args_pattern: "..." }
    quota:
      max_calls_per_session: 20
```

`quota` is optional everywhere. A manifest without `quota` blocks
behaves exactly as today's manifests do — per-call enforcement only,
no aggregate cap, no behavior change for existing v0.9.0 examples.

### Runtime accumulator

The `Session` struct grows an `aggregate_state: SessionAggregateState`
field. Every tool dispatch path increments the appropriate counter
**before** dispatch and checks against the manifest's quota:

```rust
// crates/inference-engine/src/session.rs (sketch)
fn check_aggregate_quota(&self, decision: &PolicyDecision) -> Result<()> {
    let quota = self.manifest.quota_for(&decision.tool_class);
    let snapshot = self.aggregate_state.snapshot(&decision.tool_class);
    if let Some(cap) = quota.max_calls_per_session {
        if snapshot.calls >= cap {
            return Err(PolicyError::AggregateCapExceeded {
                tool_class: decision.tool_class.clone(),
                bound: "max_calls_per_session",
                limit: cap,
                observed: snapshot.calls,
            });
        }
    }
    // ... same for max_bytes_*, etc.
    Ok(())
}
```

On `AggregateCapExceeded` the dispatch path writes a `violation`
ledger entry (sequence number consistent with the chain) and returns
`Denied` to the caller. The model sees the denial in its tool result
on the next turn, exactly like any other deny.

### Policy semantics

**`forbid-overrides-permit`**, mirroring AWS Cedar's evaluation
posture. A manifest may grant a tool but the aggregate quota denies
it; the deny wins. There is no "the per-call rule said yes, so let
this through."

The accumulator is **session-scoped**. Each `aegis run` starts a
fresh session with a zeroed accumulator. There is no cross-session
quota state — by design: cross-session limits would require a
persistent state store, which complicates the trust boundary
([ADR-022](022-trust-boundary-format-agnosticism.md)) and is out of
scope for v1.0.0. Operators that need cross-session caps wrap the
runtime in their own scheduler.

### Default detection vs. prevention

Aggregate-cap violations **prevent** (halt + deny + violation entry),
not just **detect** (log + continue). NIST SP 800-171 §3.13 (System
and Communications Protection) and the NIST AI RMF Manage function
both call for active protection of CUI; passive detection is
insufficient when the violation is high-consequence (data exfil, mass
write).

### Visibility in the ledger

Each `turn_end` entry (per [ADR-026](026-hierarchical-per-turn-ledger-protocol.md))
carries a `quotaSnapshots[]` field listing the cumulative state per
tool class at turn boundary. Auditors and the F8 replay viewer can
chart "how much of each budget did the agent burn through, and when?"
The `AggregateCapExceeded` violation entry then has obvious context.

## Why not the alternatives

- **OPA / Rego policy backend.** Rego is more expressive (cumulative
  set comprehensions, time-series predicates) but requires a remote
  policy server or local-CLI sidecar to maintain state across calls.
  That's a new dependency, a new attack surface, and a new failure
  mode (policy server unavailable → fail open or fail closed?).
  Aegis-Node's posture is that policy lives in the manifest, in
  static YAML, validated by `aegis validate`. We may add a Rego
  bridge in v1.x (operator opt-in via WASM-embedded OPA) but v1.0.0
  ships static manifest quotas only. See "Open questions" below.
- **Cedar policy embedding.** Cedar's `forbid-overrides-permit`
  philosophy is the right shape; we're borrowing the *evaluation
  model*, not the language. Embedding a Cedar engine adds a Rust
  dependency and shifts policy authoring out of the manifest into a
  separate `.cedar` file, which fragments the operator surface.
  Static YAML quotas express 90% of what operators need without the
  language churn.
- **Detection only (log, don't halt).** Some teams prefer "alert,
  don't break the agent." We reject this for v1.0.0: the
  defense-first posture and CMMC 2.0's protection-of-CUI requirement
  both call for active prevention on high-consequence breaches. A
  future ADR could add a `quota.mode: warn` opt-in for low-
  consequence experimentation.
- **Quotas in `inference.*` instead of `tools.*`.** The Triple-Bound
  Circuit Breaker ([ADR-025](025-multi-turn-agent-loop-with-triple-bound-circuit-breaker.md))
  lives in inference settings (turns, tokens, wallclock — properties
  of the model invocation). Tool-class quotas live in the tool
  policy section because they're properties of what the agent can do
  with each gated subsystem. Splitting them this way keeps the
  manifest structure mirror what's actually being bounded.

## Implementation tracking

- Schema: `schemas/manifest/v1/manifest.schema.json` extension (new
  `quota` sub-objects per tool class). Rust + Go validators
  ([ADR-002](002-split-language-architecture-rust-and-go.md)) both
  parse and enforce. Cross-language conformance harness gains
  aggregate-quota cases.
- Crate changes: `crates/policy/src/manifest.rs` (schema), 
  `crates/policy/src/aggregate.rs` (new — accumulator state),
  `crates/inference-engine/src/session.rs` (wire the accumulator into
  every dispatch path), `crates/inference-engine/src/turn.rs`
  (emit `quotaSnapshots[]` at `turn_end`).
- F10 `aegis validate` ([ADR-012](012-policy-as-code-validation.md))
  gains lint rules for aggregate quotas: warn if a quota is below the
  per-call grant size (unreachable), warn if no quota declared on a
  network outbound allowlist (over-privileged by default).
- Tracking issue: see v1.0.0 milestone tracker.

## Open questions for follow-up

- **WASM-embedded OPA for v1.x.** Some enterprise operators have
  existing Rego policy investment. Should v1.x ship an opt-in WASM
  runtime that consumes Rego policy as a *supplement* to manifest
  quotas (manifest is the floor; Rego can add but not subtract)?
  This is a v1.x decision, not v1.0.0.
- **Approval gate visibility into aggregate state.** When an F3
  approval ([ADR-029](029-task-scoped-ephemeral-approval-grants.md))
  fires mid-session, should the approver see the current accumulator
  state ("you've used 47/100 read calls, 12 MB / 50 MB read bytes")?
  Adds context but increases approval-prompt complexity. Probably
  yes — included in F3 evolution scope.
- **Quota replenishment / sliding window.** Pure session-scoped
  monotonic counters miss workloads with legitimate burst patterns.
  v1.x may add a `quota.window_seconds` for sliding-window
  enforcement; v1.0.0 ships monotonic only.

## References

- Research brief: [docs/research/multi-turn-agent-loop.md](../research/multi-turn-agent-loop.md) §"Group C — Cumulative privilege escalation"
- OWASP Agentic Top 10 §"T10 over-privilege"
- AWS Cedar evaluation model (`forbid-overrides-permit`)
- Amazon Bedrock AgentCore Code Interpreter DNS-exfil disclosure
- NIST SP 800-171 §3.13 + NIST AI RMF Manage function
