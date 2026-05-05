# 26. Hierarchical Per-Turn Ledger Protocol (F9 Schema v2)

**Status:** Proposed
**Date:** 2026-05-05
**Domain:** Ledger / schemas (extends [ADR-011](011-hash-chained-tamper-evident-ledger.md), supports [ADR-025](025-multi-turn-agent-loop-with-triple-bound-circuit-breaker.md), preserves [ADR-010](010-deterministic-trajectory-replay-offline-viewer.md))
**Related research:** [docs/research/multi-turn-agent-loop.md](../research/multi-turn-agent-loop.md) §"Group D"

## Context

Today's F9 ledger emits exactly one `reasoning_step` entry per
session, capturing the model's `toolSelected` + `toolsConsidered`
prior to the single-pass dispatch. This is sufficient when there is
exactly one model invocation per session.

[ADR-025](025-multi-turn-agent-loop-with-triple-bound-circuit-breaker.md)
introduces a multi-turn loop where each session may run up to
`--max-turns` cycles of (model_response → tool_dispatch → tool_results).
The single `reasoning_step` per session can no longer carry the
information an auditor needs to reconstruct what happened, and the
F8 trajectory replay viewer cannot deterministically replay a session
without per-turn state.

NIST SP 800-171 §3.3 (Audit and Accountability) requires audit logs
sufficient to "track and investigate unauthorized activity." For an
autonomous agent, that means: every turn's reasoning, every tool's
parameters, every tool's response, every approval decision. Standard
application logging captures *what* happened; agentic audit must
capture *why* the agent chose to do it.

Two further constraints shape the design:

1. **F9 immutability.** The append-only hash chain (entry N references
   `prevHash` of entry N-1) is load-bearing for tamper-evidence. New
   entry types must extend the chain, not break it.
2. **Compatibility Charter.** v1 ledger consumers must continue to
   parse v2 ledgers — they may skip unknown entry types, but the
   envelope (sequence, prevHash, signatures) must remain structurally
   identical.

## Decision

**Bump the ledger schema to `v2`. Emit a hierarchical, per-turn
sequence of typed entries that fully reconstruct each turn's
trajectory. Hash every tool result payload into the corresponding
ledger entry to guarantee replay determinism.**

### Per-turn entry sequence

For each session, ledger entries are emitted in this order:

```text
session_start
  ├── turn_start (turn 1)
  │   ├── reasoning_step (model's tool_selection + considered tools, per turn)
  │   ├── tool_call (one per dispatch — args, target tool name)
  │   ├── tool_result (one per dispatch — result_hash, result_payload)
  │   ├── approval_decision (zero or more — F3 grants tied to specific calls)
  │   └── turn_end (cumulative tokens, cumulative wallclock, aggregate quotas)
  ├── turn_start (turn 2)
  │   └── ... same children ...
  ├── network_attestation (F6 end-of-session, unchanged)
  └── session_end
```

Concurrent tool calls within a single turn appear as multiple
`tool_call`/`tool_result` pairs under the same `turn_start`,
distinguishable by `tool_call_id`. Sequential tool calls across turns
appear under separate `turn_start` parents — auditors and the F8
viewer can tell the difference at a glance.

### Entry shapes (delta from v1)

New entry types in v2:

| `entryType` | Carries |
|---|---|
| `turn_start` | `turnNumber`, `modelDigestHex`, `contextDigestHex` (hash of input context window), `seed`, `temperature`, `topP`, `topK` |
| `turn_end` | `turnNumber`, `tokensIn`, `tokensOut`, `tokensCumulative`, `wallclockMsCumulative`, `quotaSnapshots[]` (per-tool-class accumulator state, see [ADR-027](027-aggregate-quota-schema.md)) |
| `tool_call` | existing `access` shape + `turnNumber`, `toolCallId` (model-emitted), `requestArgsHex` (sha256 of canonical args) |
| `tool_result` | new: `turnNumber`, `toolCallId`, `resultHashHex` (sha256 of canonical payload), `resultPayload` (inline if ≤32KB; else `resultPayloadRef` pointing at sidecar) |
| `approval_decision` | F3-issued grant: `turnNumber`, `toolCallId`, `decision` (allow/deny/escalate), `grantTtlSeconds`, `grantArgHashHex`, `decidedBy` (channel + identity) |

Existing v1 entries (`session_start`, `access`, `violation`,
`network_attestation`, `session_end`) keep their shape; the only
addition is an optional `turnNumber` field on `access` and `violation`
so per-turn aggregation is straightforward in tooling. Consumers that
ignore `turnNumber` see no behavior change.

### Schema-version bump policy

Per [Compatibility Charter](../COMPATIBILITY_CHARTER.md) §"ledger":

- The `@context` URL bumps to `https://aegis-node.dev/schemas/ledger/v2`.
- v1 consumers reading a v2 ledger encounter unknown `entryType`
  values for `turn_start` / `turn_end` / `tool_result` /
  `approval_decision`. They must skip these entries, advancing
  `prevHash` accounting through them. This is the same forward-
  compatibility rule already documented for v1 itself.
- `aegis verify` validates v1 and v2 ledgers; the chain hashing is
  identical across versions — only the typed payload shapes differ.

### Replay determinism (interaction with [ADR-010](010-deterministic-trajectory-replay-offline-viewer.md))

For F8 replay to reproduce a session bit-for-bit, the ledger must
capture:

1. **The exact context window seen at each `turn_start`.** Hashed,
   not stored verbatim — the payload is reconstructable from
   prior `tool_result` entries plus the session-start prompt. The
   hash is the integrity check.
2. **The exact tool result payload returned to the model.** Stored
   inline if small (≤32KB), externalized to a sidecar blob with a
   sha256 reference if larger (database dumps, image bytes). The
   ledger always carries the hash — the payload pointer is for
   replay convenience.
3. **The model's generative parameters per turn.** `seed`,
   `temperature`, `topP`, `topK` may be pinned by the manifest's
   `inference.determinism` block, but the F9 entry records what was
   actually used (so replay can detect a manifest mutation
   mid-session).
4. **Approval decisions.** Replays in the F8 viewer simulate the
   original decision automatically; the cryptographic signature on
   `approval_decision` proves the original decision was legitimate.

**Wallclock is recorded but not replayed.** The F8 viewer renders
recorded wallclock for context but uses turn count + token budget for
deterministic replay (per [ADR-025](025-multi-turn-agent-loop-with-triple-bound-circuit-breaker.md) §"Replay determinism").

### Tool-result payload size policy

| Payload size | Storage |
|---|---|
| ≤ 32 KB | Inline in `tool_result.resultPayload` (typical: filesystem reads, MCP search results) |
| 32 KB – 16 MB | Inline if operator opts in via `inference.audit.inline_threshold_bytes`; otherwise sidecar |
| > 16 MB | Mandatory sidecar: `tool_result.resultPayloadRef = "ledger-session-XXXX-blobs/turn-N-call-M.bin"` with sha256 carried in `resultHashHex` |

The 32 KB default keeps single-file ledgers practical for small
sessions while the sidecar mechanism handles binary-heavy workloads
without bloating the chain.

## Why not the alternatives

- **One `reasoning_step` per turn (no `turn_start`/`turn_end`
  bracketing).** Captures the *what* but loses the cumulative state
  ([ADR-027](027-aggregate-quota-schema.md) quotas, wallclock,
  cumulative tokens). Auditors can't ask "what was the budget state
  when the agent decided to call $TOOL?" without bracket entries.
- **Single flat list of v1 entries with a `turnNumber` tag.** Easier
  schema migration but loses the visual hierarchy for replay tooling.
  The F8 viewer wants to fold/unfold turns; flat tagging makes that
  expensive to render.
- **Schema v1 extension (no version bump).** v1 consumers that
  already encountered unknown entry types historically have skipped
  them (per the existing forward-compat rule), so a strict reading
  says we *could* extend without a bump. We bump anyway because v2 is
  a substantive change in audit semantics; tooling vendors will want
  an explicit signal to opt in to multi-turn rendering.
- **Off-chain replay state.** Some systems keep the audit log thin
  and store replay state (e.g., context windows) in a separate
  database. Rejected: the F9 hash chain is the single non-repudiable
  artifact a CMMC C3PAO would inspect; splitting state across two
  stores undermines the "the ledger is the truth" property.

## Implementation tracking

- Schema: `schemas/ledger/v2/` (new directory; v1 stays for
  backward-compat documentation), `schemas/ledger/v2/context.jsonld`,
  `schemas/ledger/v2/entries/*.json`.
- Crate changes: `crates/ledger-writer/src/` (new entry types, blob
  sidecar handling), `crates/cli/src/verify.rs` (v1 + v2 dispatch),
  F8 replay viewer (`replay-viewer/`) updated to render the
  hierarchical structure.
- Migration: existing v1 ledgers remain readable forever; new
  sessions emit v2 once [ADR-025](025-multi-turn-agent-loop-with-triple-bound-circuit-breaker.md) lands.
- Tracking issue: see v1.0.0 milestone tracker.

## Open questions for follow-up

- **Sidecar blob signing.** Should the sidecar blob's sha256 be
  countersigned by the runtime's identity (preventing payload
  swap-out by a compromised filesystem) or is the in-chain hash
  sufficient? Lean toward in-chain hash being sufficient — the chain
  is already hash-protected end-to-end and the sidecar is referenced
  by hash.
- **Non-determinism in MCP tool results.** External MCP servers may
  return non-deterministic data (live search, RNG). The ledger
  faithfully records what was returned; F8 replay shows the recorded
  value, marked "non-deterministic external input" so auditors don't
  expect to reproduce it.
- **Entry-type forward compat for v3.** When v3 lands (e.g., adding a
  `policy_revision` entry for in-session manifest updates), the same
  skip-unknown rule applies. Document the contract once in the
  Compatibility Charter rather than restating it per ADR.

## References

- Research brief: [docs/research/multi-turn-agent-loop.md](../research/multi-turn-agent-loop.md) §"Group D — Per-turn ledger model"
- NIST SP 800-171 §3.3 (Audit and Accountability)
- [ADR-011](011-hash-chained-tamper-evident-ledger.md) F9 hash-chain protocol
- [ADR-010](010-deterministic-trajectory-replay-offline-viewer.md) F8 replay determinism
- [Compatibility Charter](../COMPATIBILITY_CHARTER.md) §"ledger"
