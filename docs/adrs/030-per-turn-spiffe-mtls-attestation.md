# 30. Per-Turn SPIFFE / mTLS Workload Attestation

**Status:** Proposed
**Date:** 2026-05-05
**Domain:** Identity (extends [ADR-003](003-cryptographic-workload-identity-spiffe-spire.md), supports [ADR-025](025-multi-turn-agent-loop-with-triple-bound-circuit-breaker.md))
**Related research:** [docs/research/multi-turn-agent-loop.md](../research/multi-turn-agent-loop.md) §"Group F"

## Context

ADR-003 established F1: every tool call rebinds workload identity to
the (model, manifest, config) digest triple, mints a short-lived
X.509-SVID via the local SPIFFE-compatible CA, and signs the access
log entry. In the single-pass model this fires once per session — one
batch of dispatch, one identity binding lifecycle.

Multi-turn execution (per [ADR-025](025-multi-turn-agent-loop-with-triple-bound-circuit-breaker.md))
introduces inter-turn gaps — moments when the runtime is between
generating one batch of tool calls and re-invoking the model with the
results. Several risks emerge in those gaps:

- **Long-lived credential exposure.** If a single SVID is minted at
  session_start and reused across all turns, an attacker who achieves
  code execution mid-session exfiltrates a credential valid for the
  full remainder of the session — potentially many turns × many tool
  calls.
- **Tool-result tampering between dispatch and re-injection.** A
  compromised host could rewrite a tool's response between when it's
  written to the ledger and when it re-enters the next turn's prompt.
  Per-turn rebinding tied to a content-addressable context-window
  digest detects the tampering.
- **Cross-turn replay of dispatch tokens.** If dispatch authentication
  is identity-bearer-based, an attacker could replay turn 2's auth
  token at turn 7 to invoke a tool the agent didn't ask for in turn 7.

Identity rebinding in single-pass execution gives us protection
against post-session credential theft. Multi-turn requires the same
property at turn boundaries.

## Decision

**Rebind workload identity at every turn boundary, even when the turn
emits zero tool calls. Each turn receives a fresh ephemeral SVID; all
tool dispatches in that turn carry that SVID; on `turn_end` the SVID
is destroyed. The SVID's `aud` claim binds to the per-turn context-
window digest (per [ADR-026](026-hierarchical-per-turn-ledger-protocol.md))
so a stolen SVID can't be replayed against a different turn.**

### Per-turn rebinding lifecycle

```text
turn_start:
  - hash the input context window → context_digest_hex
  - rebind identity:
      attest workload (process binary digest, manifest digest,
        model digest, context_digest_hex)
      → mint SVID with aud="aegis-turn://<session_id>/<turn_n>"
      → SVID TTL = max(--max-seconds remaining for this turn, 60s)
  - record svid_thumbprint in turn_start ledger entry

for each tool_call in this turn:
  - dispatch carries the turn's SVID in mTLS handshake (for MCP
    over a TCP transport) or signs the access entry directly (for
    in-process gates).

turn_end:
  - destroy the SVID (zero memory, drop key material)
  - ledger entry binds turn_end to svid_thumbprint and final
    context_digest_hex (which feeds into the next turn_start).
```

### Identity claim shape

Each per-turn SVID carries:

| Claim | Value |
|---|---|
| `spiffe_id` | `spiffe://aegis-node.local/agent/<workload>/<instance>` (unchanged from ADR-003) |
| `aud` | `aegis-turn://<session_id>/<turn_number>` (NEW — limits replay scope) |
| `manifest_digest_hex` | Manifest sha256 (unchanged) |
| `model_digest_hex` | Model artifact sha256 (unchanged) |
| `context_digest_hex` | Hash of the context window at turn_start (NEW — detects tampering) |
| `not_before` / `not_after` | Turn boundaries (NEW — short-lived; replaces session-long TTLs) |

A stolen turn-3 SVID cannot be used at turn 7 because the `aud`
mismatches and the runtime rejects on dispatch.

### Trust-boundary tightening for out-of-process MCP

[ADR-022](022-trust-boundary-format-agnosticism.md) anticipates MCP
servers running out-of-process or across the network. ADR-030
mandates that all such transports use mTLS where the agent's per-turn
SVID is the client cert and the MCP server presents its own SVID
(operator-issued, per [ADR-018](018-adopt-mcp-protocol-for-agent-tool-boundary.md)).

For in-process MCP servers (the v0.9.0 default — `stdio:` transport
launching local subprocesses), mTLS is unnecessary: the trust
boundary is process boundary on the same host. The per-turn SVID
still signs the access log entries, providing the same audit
guarantees.

### Interaction with the F9 ledger

[ADR-026](026-hierarchical-per-turn-ledger-protocol.md)'s `turn_start`
entry already carries `modelDigestHex` and `contextDigestHex`.
ADR-030 adds:

```yaml
svidThumbprintHex: "..."     # sha256 of per-turn cert DER
spiffeIdAud: "aegis-turn://<session_id>/<turn_number>"
attestationSelectors:
  - "process:binary_digest=..."
  - "process:manifest_digest=..."
  - "process:model_digest=..."
  - "process:context_digest=..."
```

`aegis verify` cross-checks each turn's `svidThumbprintHex` against
the access entries from that turn — every access entry must bind to
the turn's SVID, not the prior turn's.

## Why not the alternatives

- **One SVID per session.** Faster (no per-turn attestation
  overhead), but: stolen SVID at any turn → attacker has agent's
  identity for the remaining session length. Defeats the
  "credentials are short-lived" property.
- **One SVID per tool call (today's behavior in single-pass).**
  Doesn't address the "turn boundary tampering" attack — between
  dispatching call N and minting the SVID for call N+1, there's
  still a gap where an attacker can mutate context. Per-turn binding
  with a context_digest_hex claim closes that gap.
- **No per-turn rebinding; rely on context_digest_hex in the ledger
  alone.** The hash is recorded, but a stolen session-long SVID can
  still authenticate dispatches that would normally fail. The point
  isn't just "detect tampering after the fact" — it's "make stolen
  credentials useless." Both layers are required.
- **Defer to v1.x; v1.0.0 stays on session-long SVIDs.** Tempting
  given the implementation cost (every turn pays the attestation
  latency tax). But CMMC 2.0 §"identification and authentication"
  family requires that authenticators be "bound to the resource and
  function being accessed" — multi-turn changes the resource at each
  turn boundary, so the authenticator must change too. v1.0.0
  includes per-turn rebinding by design.

## Implementation tracking

- Crate: `crates/identity/src/svid.rs` (per-turn SVID minting),
  `crates/identity/src/attestor.rs` (attestation selector pipeline
  for the new `process:context_digest` selector),
  `crates/inference-engine/src/turn.rs` (rebind at turn_start /
  destroy at turn_end), `crates/cli/src/verify.rs` (cross-check
  dispatch SVIDs against turn-bracket SVIDs).
- For MCP-over-network transports (deferred from v0.9.0), the mTLS
  client side adopts the per-turn SVID in
  `crates/mcp-client/src/transport/`.
- Performance budget: per-turn attestation must complete in <50ms on
  commodity x86_64 (target measured against Aegis-Node's local CA).
  If empirical latency exceeds the budget, switch to a per-turn
  derived key (HKDF from the session SVID + context_digest_hex)
  rather than a fresh attestation pass — protects against replay
  (the `aud` claim still scopes per-turn) without paying the full
  attestation cost. Decided during implementation.
- Tracking issue: see v1.0.0 milestone tracker.

## Open questions for follow-up

- **Latency budget.** Empirically measure per-turn rebinding cost.
  If the local CA + attestor pipeline runs in <20ms, full per-turn
  attestation. If 20–50ms, full attestation but with caching of
  static selectors. If >50ms, fall back to HKDF-derived per-turn
  keys with the session-long SVID as the parent.
- **SPIRE integration on Kubernetes (v2.0.0).** Today's local CA
  ([ADR-003](003-cryptographic-workload-identity-spiffe-spire.md))
  becomes a SPIRE deployment in v2.0.0 (per the
  [release roadmap](../../RELEASE_PLAN.md)). The per-turn rebinding
  shape should compose with SPIRE's workload API; no manifest
  changes anticipated.
- **Approval channel cross-binding.** When [ADR-029](029-task-scoped-ephemeral-approval-grants.md)
  pauses a session for an mTLS approval and resumes it later, the
  paused session's last SVID has expired. Resume must re-attest
  before the next turn fires. Implementation note for ADR-029
  resume code.

## References

- Research brief: [docs/research/multi-turn-agent-loop.md](../research/multi-turn-agent-loop.md) §"Group F — Trust-boundary and supply-chain implications"
- [ADR-003](003-cryptographic-workload-identity-spiffe-spire.md) F1 baseline
- [SPIFFE / SPIRE specification](https://spiffe.io/docs/latest/spiffe/)
- NIST AI Agent Standards Initiative (CAISI)
- NIST SP 800-171 §"Identification and Authentication"
