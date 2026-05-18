# Aegis-Node Compatibility Charter

This document defines what Aegis-Node promises **not to break** across versions, and how schemas, contracts, and APIs evolve without breaking deployed agents and stored ledgers.

Per [ADR-002](adrs/002-split-language-architecture-rust-and-go.md) and [ADR-015](adrs/015-three-phase-deployment-roadmap.md), the manifest and ledger formats span all three deployment tiers (CLI → Kubernetes → OpenShift). A manifest written for v0.1.0 must load unchanged in v3.0.0, and a ledger captured today must replay in any future v1.x viewer. The audit story collapses if old evidence stops being readable.

## Frozen surfaces

These artifacts are the **committed API**. Breaking any of them requires a major version bump on the artifact's own version axis (not the product version):

| Surface | Version axis | Current | Compatibility window |
|---|---|---|---|
| Permission Manifest | `schemaVersion` field in the manifest itself | `"1"` | Forever, unless superseded by `schemaVersion: "2"` with an explicit migration path |
| Trajectory Ledger / Access Log | JSON-LD `@context` URI (`https://aegis-node.dev/schemas/ledger/v1#` or `…/v2#`) | `v1` (default), `v2` (opt-in per ADR-026) | Forever for stored ledgers (read compatibility); writers may move to `v2`. v1 and v2 are both first-class — see §"Ledger v2 (ADR-026)" below |
| IPC contract | proto package (`aegis.v1`) | `v1` | Forever for the wire format; new functionality lives in `aegis.v2` |
| F1–F10 feature contracts | PRD §2 | n/a | The ten security-review questions are the product spec; their answers cannot be silently dropped |

## Allowed evolution within a frozen surface

These changes are **non-breaking** and may land at any time:

- **Manifest schema**: adding new *optional* properties; relaxing validation (e.g., increasing `maxLength`); adding new enum values to fields whose semantics permit it (with default behavior preserved).
- **Ledger `@context`**: adding new term definitions; specifying additional `@type` annotations for existing terms.
- **Proto**: adding new RPCs to existing services; adding new fields to existing messages (using new tag numbers); adding new enum values; adding entirely new messages or services.

## Disallowed evolution within a frozen surface

These changes are **breaking** and require a new major version surface:

- Removing or renaming any required field, message, RPC, service, enum, or enum value.
- Repurposing an existing field (changing its semantic meaning or type).
- Tightening validation on an existing field (narrower regex, lower max, new required dependency).
- Changing the cryptographic chain semantics in the ledger (hash algorithm, prev-hash placement, entry ordering rules).

When in doubt: if a *downstream* tool that worked yesterday could break tomorrow because of your change, it's breaking.

## Enforcement

- **Proto**: `buf breaking` check runs on every PR. CI rejects breaking changes on the `v1` package. Configured in [`proto/buf.yaml`](../proto/buf.yaml).
- **Manifest schema**: PRs touching `schemas/manifest/v1/manifest.schema.json` require a justification in the PR description and a maintainer review. The example manifests under `schemas/manifest/v1/examples/` must continue to validate.
- **Ledger `@context`**: same — PRs require maintainer review, and downstream tooling (replay viewer, CLI ledger reader) must continue to consume v1 ledgers byte-identically.

## Version bump process

When a breaking change is genuinely required:

1. Open an ADR documenting the why (and what couldn't be solved with a non-breaking change).
2. Create the new version surface alongside the old:
   - `proto/aegis/v2/aegis.proto`
   - `schemas/manifest/v2/manifest.schema.json`
   - `schemas/ledger/v2/context.jsonld`
3. The runtime supports **both** versions during the deprecation window (minimum: one full minor release).
4. The validator, ledger reader/writer, and replay viewer detect the version and dispatch accordingly.
5. Existing v1 artifacts continue to round-trip.

There is no plan to retire `v1` once `v2` ships. Removal would itself be a breaking change against deployed audit evidence.

## Evidence pack v1 (#187)

`aegis evidence cmmc --ledger <path> --out <dir>` (per the [Compliance Traceability Matrix](COMPLIANCE_MATRIX.md) §"Evidence artifact generation") emits a JSON + Markdown bundle mapping F9 ledger entries to NIST SP 800-171 controls.

**Frozen surface.** The pack JSON shape is pinned at `schemas/evidence/v1/evidence-pack.schema.json`; downstream tools (C3PAO ingest, internal dashboards) may rely on it. Schema bumps follow the same rules as ledger v1→v2: new fields can be additive at minor versions; renames/removals require a v2 bump and a transition window.

**Cryptographic anchor.** The pack embeds the F9 ledger's chain root in `ledgerRootHex` — auditors re-derive it via `aegis verify <ledger>` to confirm the pack and the chain agree. No cosign signature on the pack itself yet; that's a deferred follow-up.

**Foundation scope.** v2 ledgers only; NIST SP 800-171 mapping only; single ledger input; unsigned. AI RMF + OWASP framework outputs, v1 ledger compat shim, multi-ledger aggregation, cosign `--sign`, and the CI lint asserting full COMPLIANCE_MATRIX.md coverage are deferred follow-ups.

## Approval grants (ADR-029)

[ADR-029](adrs/029-task-scoped-ephemeral-approval-grants.md) evolves the F3 approval gate from per-call prompts to **task-scoped ephemeral grants**. The manifest gains an optional `tools.<class>.approval` block with a `tier` enum and a `grant_ttl_seconds` budget. The schema bump is **non-breaking**: every existing manifest behaves exactly as before (effectively `tier: validating` + a 5-minute default TTL when the block is absent).

**Forbid-overrides-permit + audit-by-default.** Every approval decision lands on the F9 ledger as both the legacy `approval_granted` / `approval_rejected` entry (operator's raw decision) and a new `approval_decision` entry (the grant lifecycle: `grant_issued`, `auto_consumed_allow`, `auto_consumed_deny`, `auto_advisory`). Auditors can chart every silent retry back to the original prompt via `sourceGrantId`.

**Auto-consume binding.** Grants bind to `(tool_name, sha256(canonical_args))`. Identical retries within TTL skip the prompt; **argument drift voids the match** (different args → fresh prompt). This is what defeats the "approve once, attacker mutates args" attack ADR-029 §"Context" calls out.

**Foundation slice.** Wires `advisory` (no prompt, log + dispatch) and `validating` (today's prompt behavior, plus the grant table). `blocking` and `escalating` are recognized in the manifest + type system but **treated as `validating`** for now — their distinct behaviors are deferred follow-ups.

**Intentionally deferred (file as separate issues when picked up):**

- Session pause/resume for headless mTLS async approvals (sidecar serialization, `aegis run --resume`, encrypted state).
- `blocking` tier's hard-deny behavior (the most security-relevant tier left unwired).
- `escalating` tier's secondary-channel routing.
- Aggregate-state visibility in the TTY prompt text (ADR-029 §"Visibility into aggregate state").
- `aegis revoke <session-id> <grant-id>` for early grant revocation.
- F8 replay viewer rendering of `approval_decision` entries with grant lineage.
- Go validator behavioral parity for `tier` (parse parity already in via `ApprovalPolicy`).
- Cross-language conformance fixtures for the new manifest + canonical-args hash.

## Per-turn workload attestation (ADR-030)

[ADR-030](adrs/030-per-turn-spiffe-mtls-attestation.md) introduces fresh, short-lived SVIDs at every turn boundary in the multi-turn loop. The session-long SVID minted at `Session::boot` is **unchanged** — it still binds the agent's identity for non-turn paths (single-turn `run_turn`, mediator calls outside the loop).

**Per-turn SVID lifecycle.** `Session::run` issues a turn SVID at `turn_start`, stashes it as `current_turn_svid`, and drops it at `turn_end`. Each per-turn SVID carries:

- the bound `(model, manifest, config)` digest triple (unchanged from session SVIDs),
- the optional chat-template digest (unchanged),
- a NEW `TURN_BINDING` custom extension (OID `1.3.6.1.4.1.99999.3`) carrying a 2-byte-length-prefixed UTF-8 audience string of the form `aegis-turn://<session_id>/<turn_number>`.

A verifier seeing the `TURN_BINDING` extension knows the SVID is turn-scoped; absence means it's a session SVID. v1 ledger consumers that don't parse the extension at all see a regular SVID with one extra non-critical extension — TLS libraries pass it through cleanly.

**Audience replay protection.** A stolen turn-N SVID cannot be replayed at turn M because the embedded audience disagrees. Foundation PR records the audience + thumbprint in `turn_start.spiffeIdAud` and `turn_start.svidThumbprintHex`; the dispatch-side cross-check ("every access entry's signing SVID matches the active turn's SVID") is a deferred follow-up.

**Foundation PR scope.** Per-turn SVID issuance with the TurnBinding extension; ledger `turn_start` carries `svidThumbprintHex` + `spiffeIdAud`. Deferred follow-ups:

- The <50 ms latency benchmark + HKDF-derived fallback if attestation runs slow (ADR-030 §"Latency budget").
- MCP-over-network mTLS transport adoption of the per-turn SVID (ADR-022 dependency).
- `aegis verify` cross-check of access entries' signing SVID against their turn-bracket SVID.
- Process-attestation selector expansion (`process:binary_digest`, etc.).
- ADR-029 approval-channel cross-binding (resume must re-attest before next turn).

## Aggregate quotas (ADR-027)

[ADR-027](adrs/027-aggregate-quota-schema.md) extends the F2 Permission Manifest with optional `tools.<class>.quota` blocks. The schema bump is **non-breaking**: every existing manifest stays valid because `quota` is an optional sub-object on each tool class.

**Forbid-overrides-permit.** A manifest may grant a tool via per-call rules and still deny in aggregate. The deny wins — there is no "the per-call check said yes, so let this through." Mirrors AWS Cedar's evaluation posture.

**Scope: foundation PR.** `max_calls_per_session` only, on `tools.filesystem`, `tools.network`, `tools.exec` (new sub-block carrying just `quota`), and per-server on `tools.mcp[*]`. Byte-counter quotas (`max_bytes_read_per_session`, etc.), per-tool MCP quotas, F10 `aegis validate` lint rules, sliding-window quotas, and Go validator parity are deferred follow-ups.

**Session-scoped accumulators.** Counters reset at session start; there is no cross-session quota state by design (cross-session limits would require a persistent state store outside Aegis-Node's trust boundary). Operators wanting cross-session caps wrap the runtime in their own scheduler.

**Visibility in the ledger.** Each v2 `turn_end` entry carries a `quotaSnapshots[]` array — one entry per declared-or-dispatched class — so auditors can chart budget burn-down across turns. Aggregate-cap breaches surface as `Violation` entries with `violationKind: "AggregateCapExceeded"` for v1-schema compatibility.

## Ledger v2 (ADR-026)

[ADR-026](adrs/026-hierarchical-per-turn-ledger-protocol.md) introduces a hierarchical per-turn protocol on top of the v1 chain mechanics. The chain itself (`sequenceNumber`, `prevHash`, SHA-256 walk) is **identical** across versions — only the typed payload set differs.

**Version detection.** Readers detect the version from the `@context` URL on the first entry (`session_start`). Every subsequent line MUST carry the same context URL — mixing v1 and v2 within one ledger file is a chain-break (rejected by `aegis verify` with `VerifyBreak::BadContext`).

**Forward-compat rule for new entry types.** v1 consumers reading a v2 ledger encounter `entryType` values they don't recognize (`turn_start`, `turn_end`, `tool_call`, `tool_result`, `approval_decision`). They MUST skip these entries, advancing `prevHash` accounting through them. This is the same rule v1 already documented for itself; ADR-026 makes it concrete.

**Per-turn entry shape.** v2 brackets each turn with `turn_start` / `turn_end` and emits `tool_call` / `tool_result` pairs (joined by `toolCallId`) for every model-initiated dispatch. Existing v1 entry types (`session_start`, `access`, `violation`, `network_attestation`, `session_end`) keep their shape; v2 may add an optional `turnNumber` field on `access` and `violation` for per-turn aggregation. Consumers that ignore `turnNumber` see no behavior change.

**Default and opt-in.** As of the foundation PR, the writer default remains `v1` for back-compat. v2 is opt-in via `BootConfig::ledger_schema = Some(LedgerSchemaVersion::V2)`. The CLI default will flip to v2 once the deferred follow-ups land:

- Sidecar blob mechanism for tool-result payloads >32 KB (ADR-026 §"Tool-result payload size policy").
- Cross-language conformance fixtures (Go ↔ Rust) for the new entry kinds.
- `aegis verify` CLI surface for the per-line schema-version report (the `VerifySummary.schema_version` field is already populated).
- F8 replay viewer per-turn fold/unfold rendering.
- `approval_decision` emission tied to F3 grants (the entry kind exists; emission is pending the approval-gate refactor).

## Storage and replay compatibility

This is the strongest guarantee in the project: a ledger written by v0.5.0 must replay in the v1.0.0 viewer; a manifest accepted at v0.5.0 must load and produce identical enforcement behavior at v1.0.0 (modulo new optional fields).

Reviewers and auditors should be able to take a USB stick from a 2026 deployment to a 2030 reviewer machine and reconstruct the session faithfully.

## Out of scope

The following are **not** committed surfaces and may change between minor versions without a deprecation window:

- Internal Rust crate APIs (`crates/*`) — consumers should use the proto contract or the CLI.
- Internal Go packages under `pkg/` — same.
- The on-disk layout of caches, work directories, and per-session scratch — implementation detail.
- The exact text of human-readable approval prompts and replay-viewer copy — UX, not contract.
- Tool versions in `mise.toml` and the devbox image — these track upstream and are not user-visible.

## Contact

Questions about whether a proposed change is breaking: open an issue tagged `compatibility` and link the affected files.
