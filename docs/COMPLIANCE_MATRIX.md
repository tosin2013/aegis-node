# Aegis-Node Compliance Traceability Matrix

This matrix maps Aegis-Node's runtime architecture to U.S. defense
and federal compliance frameworks. It is the document a Certified
Third-Party Assessment Organization (C3PAO) inspects when validating
the runtime against CMMC 2.0 Level 2 / NIST SP 800-171 / NIST AI RMF
controls.

The matrix is maintained alongside the [Architectural Decision
Records](adrs/) — every architectural decision that lands a
control mapping cites the corresponding ADR.

**Version:** 0.1 (initial — covers v0.5.0 → v1.0.0 architecture)
**Date:** 2026-05-05
**Strategic anchor:** [v1.0.0 GA / CMMC 2.0 deadline 2026-11-02](../RELEASE_PLAN.md)

## Frameworks covered

- **CMMC 2.0 Level 2** — Cybersecurity Maturity Model Certification,
  required for U.S. DoD contractors handling Controlled Unclassified
  Information (CUI). Mandates the 110 controls of NIST SP 800-171.
- **NIST SP 800-171 Rev. 3** — Protecting CUI in nonfederal systems.
  14 control families (AC, AT, AU, CM, IA, IR, MA, MP, PE, PS, RA, CA, SC, SI).
- **NIST AI Risk Management Framework (AI RMF 1.0 + Generative AI
  Profile)** — Govern / Map / Measure / Manage functions for
  AI-specific risk.
- **OWASP Top 10 for Agentic Applications 2026** — community-driven
  threat catalog for autonomous agents.
- **MITRE ATLAS** — adversarial tactics targeting AI systems.

## NIST SP 800-171 control mapping

### Access Control (AC)

| Control | Aegis-Node coverage | ADR |
|---|---|---|
| 3.1.1 Limit system access to authorized users | F2 Permission Manifest enforces per-tool allowlist (closed-by-default) | [ADR-004](adrs/004-declarative-yaml-permission-manifest.md) |
| 3.1.2 Limit system access to authorized transactions | Per-call dispatch through `mediate_*` gates | [ADR-004](adrs/004-declarative-yaml-permission-manifest.md), [ADR-024](adrs/024-mcp-args-prevalidation.md) |
| 3.1.3 Control flow of CUI per approved authorizations | F6 Network deny-by-default + signed network attestation | [ADR-008](adrs/008-network-deny-by-default-at-runtime-level.md) |
| 3.1.5 Employ principle of least privilege | F7 read-only default + explicit time-bounded write grants | [ADR-009](adrs/009-read-only-default-with-explicit-write-grants.md), [ADR-019](adrs/019-explicit-write-grant-takes-precedence.md) |
| 3.1.7 Prevent non-privileged users from executing privileged functions | F3 Approval Gate, tier-based (validating/blocking/escalating) | [ADR-005](adrs/005-human-approval-gate-for-sensitive-actions.md), [ADR-029](adrs/029-task-scoped-ephemeral-approval-grants.md) |
| 3.1.8 Limit unsuccessful logon attempts | Aggregate quota cap on auth-shaped tools (e.g., `exec.max_calls_per_session`) | [ADR-027](adrs/027-aggregate-quota-schema.md) |
| 3.1.20 Verify and control connections to external systems | MCP server allowlist + per-server SVID mTLS for out-of-process transports | [ADR-018](adrs/018-adopt-mcp-protocol-for-agent-tool-boundary.md), [ADR-030](adrs/030-per-turn-spiffe-mtls-attestation.md) |

### Audit and Accountability (AU)

| Control | Aegis-Node coverage | ADR |
|---|---|---|
| 3.3.1 Create + retain audit logs | F9 hash-chained Trajectory Ledger; append-only | [ADR-011](adrs/011-hash-chained-tamper-evident-ledger.md) |
| 3.3.2 Ensure individual users uniquely identifiable in audit | F1 SPIFFE ID + per-turn SVID thumbprint in every entry | [ADR-003](adrs/003-cryptographic-workload-identity-spiffe-spire.md), [ADR-030](adrs/030-per-turn-spiffe-mtls-attestation.md) |
| 3.3.4 Alert in event of audit logging process failure | `aegis verify` chain integrity check; CI gate fails on broken chain | [ADR-011](adrs/011-hash-chained-tamper-evident-ledger.md) |
| 3.3.5 Correlate audit-record review for investigation | F8 Trajectory Replay viewer; hierarchical per-turn ledger structure | [ADR-010](adrs/010-deterministic-trajectory-replay-offline-viewer.md), [ADR-026](adrs/026-hierarchical-per-turn-ledger-protocol.md) |
| 3.3.7 Provide system capability to compare + synchronize internal clocks | Wallclock recorded per turn_end; F8 viewer renders without trusting client clocks | [ADR-026](adrs/026-hierarchical-per-turn-ledger-protocol.md) |
| 3.3.8 Protect audit info + audit logging tools from unauthorized access | F9 hash chain + per-entry signature; immutable append-only | [ADR-011](adrs/011-hash-chained-tamper-evident-ledger.md) |
| 3.3.9 Limit management of audit logging functionality | Manifest-only audit configuration; no runtime mutation | [ADR-004](adrs/004-declarative-yaml-permission-manifest.md), [ADR-026](adrs/026-hierarchical-per-turn-ledger-protocol.md) |

### Identification and Authentication (IA)

| Control | Aegis-Node coverage | ADR |
|---|---|---|
| 3.5.1 Identify users, processes, devices | F1 cryptographic workload identity with X.509-SVID | [ADR-003](adrs/003-cryptographic-workload-identity-spiffe-spire.md) |
| 3.5.2 Authenticate the identities | Local CA + node + workload attestation; mTLS for out-of-process transports | [ADR-003](adrs/003-cryptographic-workload-identity-spiffe-spire.md), [ADR-030](adrs/030-per-turn-spiffe-mtls-attestation.md) |
| 3.5.3 Use multi-factor authentication for privileged accounts | F3 approval gate's mTLS+SPIFFE channel for high-tier actions | [ADR-005](adrs/005-human-approval-gate-for-sensitive-actions.md), [ADR-029](adrs/029-task-scoped-ephemeral-approval-grants.md) |
| 3.5.4 Employ replay-resistant authentication | Per-turn SVID `aud` claim binds to `aegis-turn://<session>/<turn>` | [ADR-030](adrs/030-per-turn-spiffe-mtls-attestation.md) |
| 3.5.5 Prevent reuse of identifiers for a defined period | Per-turn SVID — minted at turn_start, destroyed at turn_end | [ADR-030](adrs/030-per-turn-spiffe-mtls-attestation.md) |
| 3.5.6 Disable identifiers after a period of inactivity | Session-scoped SVID lifecycle; no long-lived bearer tokens | [ADR-030](adrs/030-per-turn-spiffe-mtls-attestation.md) |

### Configuration Management (CM)

| Control | Aegis-Node coverage | ADR |
|---|---|---|
| 3.4.1 Establish baseline configurations | F2 manifest is source-of-truth; pinned via OCI artifact + cosign | [ADR-004](adrs/004-declarative-yaml-permission-manifest.md), [ADR-013](adrs/013-oci-artifacts-for-model-distribution.md) |
| 3.4.2 Establish and enforce security configuration settings | `aegis validate` linter before runtime; manifest digest binds to identity | [ADR-012](adrs/012-policy-as-code-validation.md) |
| 3.4.3 Track changes to organizational systems | Manifest digest in every ledger entry; manifest hash bound to SVID | [ADR-003](adrs/003-cryptographic-workload-identity-spiffe-spire.md) |
| 3.4.6 Employ least functionality | Closed-by-default manifest; `tools.*` quota caps | [ADR-004](adrs/004-declarative-yaml-permission-manifest.md), [ADR-027](adrs/027-aggregate-quota-schema.md) |

### System and Communications Protection (SC)

| Control | Aegis-Node coverage | ADR |
|---|---|---|
| 3.13.1 Monitor + control communications at external boundaries | F6 network deny-by-default; F8 end-of-session signed network attestation | [ADR-008](adrs/008-network-deny-by-default-at-runtime-level.md) |
| 3.13.2 Employ architectural designs that promote security | Closed-by-default + zero-trust posture across F1–F10 | [ADR-001](adrs/001-zero-trust-security-review-as-product-specification.md) |
| 3.13.4 Prevent unauthorized + unintended information transfer via shared system resources | Per-session aggregate quotas; in-memory accumulator cleared on session end | [ADR-027](adrs/027-aggregate-quota-schema.md) |
| 3.13.6 Deny network communications by default + permit by exception | F6 outbound allowlist + per-host:port granularity | [ADR-008](adrs/008-network-deny-by-default-at-runtime-level.md) |
| 3.13.8 Implement cryptographic mechanisms to prevent unauthorized disclosure of CUI | Hash-chain + per-turn SVID signing of every dispatch | [ADR-011](adrs/011-hash-chained-tamper-evident-ledger.md), [ADR-030](adrs/030-per-turn-spiffe-mtls-attestation.md) |
| 3.13.10 Establish + manage cryptographic keys | Local CA per ADR-003; SPIRE-compatible upgrade path for v2.0.0 | [ADR-003](adrs/003-cryptographic-workload-identity-spiffe-spire.md) |
| 3.13.11 Employ FIPS-validated cryptography for protecting CUI | Pluggable signing — local CA today, FIPS-validated provider tracked for v1.x | [ADR-003](adrs/003-cryptographic-workload-identity-spiffe-spire.md) (open for v1.x) |

### System and Information Integrity (SI)

| Control | Aegis-Node coverage | ADR |
|---|---|---|
| 3.14.1 Identify, report + correct information system flaws | F9 ledger violation entries; aggregate quota breaches; adversarial classifier verdicts | [ADR-011](adrs/011-hash-chained-tamper-evident-ledger.md), [ADR-027](adrs/027-aggregate-quota-schema.md), [ADR-028](adrs/028-adversarial-pre-filter-gate.md) |
| 3.14.6 Monitor system to detect attacks + indicators of potential attacks | Adversarial Pre-Filter Gate classifier + verdict in F9 ledger | [ADR-028](adrs/028-adversarial-pre-filter-gate.md) |
| 3.14.7 Identify unauthorized use of organizational systems | Per-turn rebinding + per-call access entries; cross-checked by `aegis verify` | [ADR-030](adrs/030-per-turn-spiffe-mtls-attestation.md) |

## NIST AI Risk Management Framework mapping

| AI RMF Function | Aegis-Node coverage | ADR |
|---|---|---|
| **Govern 1.4** Document risks, including indirect prompt injection | OWASP T1 + T10 explicitly mitigated; threat model in research brief | [ADR-028](adrs/028-adversarial-pre-filter-gate.md), research brief |
| **Map 1.1** Document the system's intended purpose, capabilities, limitations | F2 manifest; pinned model digest; pinned chat-template digest | [ADR-004](adrs/004-declarative-yaml-permission-manifest.md), [ADR-022](adrs/022-trust-boundary-format-agnosticism.md) |
| **Measure 2.2** Evaluate AI systems for trustworthiness across deployment contexts | F8 trajectory replay; cross-language conformance harness | [ADR-010](adrs/010-deterministic-trajectory-replay-offline-viewer.md), [ADR-002](adrs/002-split-language-architecture-rust-and-go.md) |
| **Measure 2.7** Evaluate AI system security for OWASP Top 10 LLM | Adversarial Pre-Filter Gate; aggregate quotas | [ADR-028](adrs/028-adversarial-pre-filter-gate.md), [ADR-027](adrs/027-aggregate-quota-schema.md) |
| **Manage 4.1** Risk responses are planned, prioritized + implemented | Triple-Bound Circuit Breaker + tier-based approval | [ADR-025](adrs/025-multi-turn-agent-loop-with-triple-bound-circuit-breaker.md), [ADR-029](adrs/029-task-scoped-ephemeral-approval-grants.md) |
| **Manage 4.2** Mechanisms in place to monitor AI risks at deployment | F9 ledger + F8 viewer; aggregate-quota snapshots per turn | [ADR-026](adrs/026-hierarchical-per-turn-ledger-protocol.md) |

## OWASP Top 10 for Agentic Applications 2026 mapping

| OWASP ID | Threat | Aegis-Node mitigation | ADR |
|---|---|---|---|
| T1 | Prompt Injection (direct + indirect) | Adversarial Pre-Filter Gate sanitizes inbound tool results; warning-block wrapper | [ADR-028](adrs/028-adversarial-pre-filter-gate.md) |
| T2 | Agentic Supply Chain | OCI + cosign artifact verification; per-turn SVID rebinding | [ADR-013](adrs/013-oci-artifacts-for-model-distribution.md), [ADR-021](adrs/021-huggingface-as-upstream-oci-as-trust-boundary.md), [ADR-030](adrs/030-per-turn-spiffe-mtls-attestation.md) |
| T3 | Insecure Tool Use | Closed-by-default `tools.mcp[]` allowlist + ADR-024 pre_validate + ADR-028 post_validate | [ADR-018](adrs/018-adopt-mcp-protocol-for-agent-tool-boundary.md), [ADR-024](adrs/024-mcp-args-prevalidation.md), [ADR-028](adrs/028-adversarial-pre-filter-gate.md) |
| T6 | Cascading Agentic Failures | Triple-Bound Circuit Breaker (turns + tokens + wallclock) + aggregate quotas | [ADR-025](adrs/025-multi-turn-agent-loop-with-triple-bound-circuit-breaker.md), [ADR-027](adrs/027-aggregate-quota-schema.md) |
| T7 | Memory Poisoning | Hash-chained ledger + Adversarial Pre-Filter on inbound results; F8 replay highlights flagged turns | [ADR-011](adrs/011-hash-chained-tamper-evident-ledger.md), [ADR-028](adrs/028-adversarial-pre-filter-gate.md) |
| T9 | Human-Agent Trust Exploitation | Tier-based approvals; argument-hash-bound ephemeral grants | [ADR-005](adrs/005-human-approval-gate-for-sensitive-actions.md), [ADR-029](adrs/029-task-scoped-ephemeral-approval-grants.md) |
| T10 | Excessive Agency / Over-Privilege | Aggregate quotas with `forbid-overrides-permit`; per-turn SVID with `aud` claim | [ADR-027](adrs/027-aggregate-quota-schema.md), [ADR-030](adrs/030-per-turn-spiffe-mtls-attestation.md) |

## Evidence artifact generation (for v1.0.0 GA)

A C3PAO assessing CMMC 2.0 Level 2 needs evidence that controls are
operating, not just declared. Aegis-Node generates evidence artifacts
directly from the F9 ledger:

- **`aegis evidence cmmc --session <id>`** (planned, v1.0.0) emits a
  signed report listing every control mapping that the session
  exercised, with cryptographic references back to the ledger
  entries. Auditors can spot-check by running `aegis verify` against
  the cited ledger.
- **Cross-language conformance harness output** is reproducible
  evidence that policy interpretation matches between the Go
  validator and the Rust enforcer — a property auditors can rerun on
  any commit.
- **`aegis validate`** lint pass at policy authoring time is recorded
  in CI, providing pre-runtime evidence of policy correctness.

This evidence pipeline is in scope for the v1.0.0 GA release and
the design-partner security review.

## Open compliance items tracked toward v1.0.0 GA

- **FIPS-validated cryptography** (NIST SP 800-171 §3.13.11). Local
  CA in [ADR-003](adrs/003-cryptographic-workload-identity-spiffe-spire.md)
  uses pluggable signing; FIPS-validated provider integration is
  tracked but not yet implemented.
- **`aegis evidence cmmc` evidence-pack generator.** Sketched above;
  implementation tracked in v1.0.0.
- **Independent third-party red-team validation against MITRE
  ATLAS.** Recommended in the research brief; scheduled before v1.0.0
  GA.
- **Design-partner security review.** Required for v1.0.0 per the
  Release Plan.

## Maintenance

- This matrix is updated whenever a new ADR maps to a control, or
  when an existing ADR's coverage changes. It's expected to grow as
  v1.0.0 controls move from Proposed → Accepted.
- The matrix is **NOT a substitute for a C3PAO assessment**. It's the
  documentation that an assessment uses as its starting point.
- Versioning: header version bumps with each substantive update;
  history is in git log.

## References

- CMMC 2.0 — https://dodcio.defense.gov/CMMC/about/
- NIST SP 800-171 Rev. 3 — https://csrc.nist.gov/pubs/sp/800/171/r3/final
- NIST AI Risk Management Framework — https://www.nist.gov/itl/ai-risk-management-framework
- OWASP Top 10 for Agentic Applications 2026 — https://genai.owasp.org/resource/owasp-top-10-for-agentic-applications-for-2026/
- MITRE ATLAS — https://atlas.mitre.org/
- Research brief: [docs/research/multi-turn-agent-loop.md](research/multi-turn-agent-loop.md)
