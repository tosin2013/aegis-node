# Aegis-Node — Baseline Release Plan

**Status:** Greenfield baseline · established 2026-04-27
**Source:** Generated from PRD v3.0 + ADRs 001–016 + project plan (mcp_planning, project ID `1777320020862-gygda2e2r`).

> The MCP `release_tracking create_milestone` operation requires an authenticated GitHub remote, which is not yet configured for this greenfield project. Once a GitHub remote is added and `gh auth login` is run, these milestones can be synced upstream by re-running `release_tracking` with `syncGithubMilestones: true`. Until then, this document is the authoritative baseline release plan.

## Release Cadence and Versioning

- Semantic versioning (`MAJOR.MINOR.PATCH`).
- Pre-Phase-1-GA releases use `0.x.y` (community-preview, no compatibility guarantees).
- Phase 1 GA is `v1.0.0` and locks the manifest + ledger schemas.
- Phase 2 GA is `v2.0.0`; Phase 3 GA is `v3.0.0`. Manifest and ledger formats remain backward-compatible across major versions wherever possible (per ADR-015).

## Baseline Milestones

| Version | Milestone | Target | Linked ADRs / Phase |
|---|---|---|---|
| **v0.1.0** | Foundations: schemas frozen, IPC contract committed, monorepo + CI scaffolded | 2026-05-25 | ADR-002, ADR-004, ADR-011, ADR-015 / Phase 0 |
| **v0.5.0** | Core Security Primitives: F1 identity, F2 manifest enforcement, F9 ledger writer + verify, F4 access log emitter | 2026-07-20 | ADR-003, ADR-004, ADR-006, ADR-011 / Phase 1a |
| **v0.8.0** | Reasoning + Approval: F5 trajectory, F3 approval gate, F6 network deny, F7 read-only default | 2026-08-31 | ADR-005, ADR-007, ADR-008, ADR-009 / Phase 1b |
| **v0.9.0** | Tooling + Replay: F10 validator, F8 replay viewer, OCI pull + Cosign, llama.cpp FFI, MCP client | 2026-10-05 | ADR-010, ADR-012, ADR-013, ADR-014, ADR-018 / Phase 1c |
| **v1.0.0** | **Phase 1 GA — Security Review Milestone:** multi-turn agent loop with per-turn enforcement, web UI, conformance suite, auditor evidence package, design-partner review passed, Apache 2.0 community release | **2026-11-02** | ADR-001, ADR-016, ADR-025, ADR-026, ADR-027, ADR-028, ADR-029, ADR-030 / Phase 1 GA |
| **v2.0.0** | Kubernetes Runtime: Operator + CRDs, SPIRE attestation, GPU backends (vLLM/TGI/KServe), persistent ledger | 2027-01-25 | ADR-002, ADR-015 / Phase 2 |
| **v3.0.0** | OpenShift Enterprise Runtime: SCCs, disconnected install, GitOps, automated CMMC/FedRAMP exports | 2027-04-19 | ADR-015 / Phase 3 |

## Strategic Anchor: CMMC 2.0 Deadline

`v1.0.0` (2026-11-02) is deliberately aligned with the U.S. CMMC 2.0 deadline of November 2026 (PRD §9 — defense beachhead market). The Phase 1 GA milestone exists to be the answer enterprise security teams give the auditor on the deadline date.

## How This Plan Is Maintained

- Milestone updates: re-run `mcp__adr-analysis__release_tracking` with the desired operation.
- ADR additions/changes: re-run `generate_adrs_from_prd` (or hand-author) and update the linked-ADRs column above.
- Phase progress: managed via `mcp__adr-analysis__mcp_planning` (project ID above).

## When GitHub Is Configured

Once `gh auth login` succeeds and a GitHub remote is added:

1. Re-run `release_tracking` with `operation: create_milestone` for each row above (with `syncGithubMilestones: true` if cluster-syncing is desired).
2. Use `release_tracking` with `operation: track_release` to attach actual commits/tags as the milestones close.

<!-- LOCAL MILESTONES -->

## Local Milestones

_Synced from `.mcp-adr-cache/milestones.local.json`. Run `release_tracking` with `operation: "push_local_milestones"` to publish to GitHub._

> **Format note (consumed by `.github/workflows/release.yml`):** each card
> below must start with an H3 header in the exact form `### vX.Y.Z — <name>`
> (em-dash, not hyphen). The release workflow extracts the matching card
> when a tag is pushed and includes it in the GitHub Release body. Pre-
> release tags (`vX.Y.Z-rc.N`) fall back to the base-version card. See
> `scripts/release/extract-milestone-notes.sh` for the parser.

### v0.1.0 — Foundations
<!-- milestone-id: v0-1-0-foundations -->
- **Status:** pushed (#1)
- **Due:** 2026-05-25

Manifest schema v1 frozen, ledger JSON-LD schema frozen, IPC contract (aegis.proto) committed, Rust+Go monorepo + CI scaffolding. Sets the format-stability invariant for all downstream phases. Maps to ADRs 002, 004, 011, 015.

### v0.5.0 — Core Security Primitives
<!-- milestone-id: v0-5-0-core-security-primitives -->
- **Status:** pushed (#2)
- **Due:** 2026-07-20

Phase 1a: F1 SPIFFE-compatible workload identity with built-in CA, F2 Permission Manifest parser + enforcer, F9 hash-chained ledger writer + verify CLI, F4 access log emitter wired into Rust syscalls. Maps to ADRs 003, 004, 006, 011.

### v0.8.0 — Reasoning + Approval
<!-- milestone-id: v0-8-0-reasoning-approval -->
- **Status:** pushed (#3)
- **Due:** 2026-08-31

Phase 1b: F5 pre-execution trajectory recorder, F3 human approval gate (CLI + local web UI + signed API), F6 network-deny-by-default runtime gate with end-of-session attestation, F7 read-only default + write_grants enforcement (incl. time-bounded grants). Maps to ADRs 005, 007, 008, 009.

### v0.9.0 — Tooling and Replay
<!-- milestone-id: v0-9-0-tooling-and-replay -->
- **Status:** pushed (#4)
- **Due:** 2026-10-05

Phase 1c: F10 policy-as-code validator (aegis validate) with composition + linter, F8 deterministic offline single-file HTML replay viewer, OCI artifact pull + Cosign verification (aegis pull), llama.cpp Rust FFI binding integrated with Backend trait, MCP client adoption per ADR-018 (manifest gains optional `tools.mcp[]`; F5 reasoning entries carry MCP tool names). Maps to ADRs 010, 012, 013, 014, 018.

### v1.0.0 — Phase 1 GA / Security Review Milestone
<!-- milestone-id: v1-0-0-phase-1-ga-security-review-milestone -->
- **Status:** pushed (#5)
- **Due:** 2026-11-02

Phase 1 GA. Closes the gap between "agent runtime" the project name and what the runtime can actually do.

**Multi-turn agent loop with per-turn enforcement** (ADR-025 through ADR-030, informed by the [security research brief](docs/research/multi-turn-agent-loop.md)):
- ADR-025: bounded multi-turn loop in `Session::run_turn` with Triple-Bound Circuit Breaker (turns + tokens + wallclock)
- ADR-026: hierarchical per-turn ledger protocol (F9 schema v2) with per-turn reasoning, tool_call/tool_result hashing, replay determinism
- ADR-027: per-session aggregate quota schema for the F2 manifest (forbid-overrides-permit, prevention not just detection)
- ADR-028: Adversarial Pre-Filter Gate against indirect prompt injection in inbound tool results
- ADR-029: F3 evolution — task-scoped ephemeral approval grants (argument-hash-bound TTLs, tier-based scoping, async pause/resume)
- ADR-030: per-turn SPIFFE/mTLS workload attestation (ephemeral SVIDs, `aud` claim binds to turn)

**Other v1.0.0 deliverables:**
- Web UI (operator console for live agent observation + approval routing — details TBD)
- End-to-end conformance test suite green
- `aegis evidence cmmc` evidence-pack generator (signed report from F9 ledger; see [docs/COMPLIANCE_MATRIX.md](docs/COMPLIANCE_MATRIX.md))
- First design-partner security review passed
- Independent MITRE ATLAS red-team validation
- Apache 2.0 community release

Anchored to the U.S. CMMC 2.0 deadline (PRD §9 — defense beachhead). Maps to ADRs 001, 016, 025, 026, 027, 028, 029, 030.

### v2.0.0 — Kubernetes Runtime
<!-- milestone-id: v2-0-0-kubernetes-runtime -->
- **Status:** pushed (#6)
- **Due:** 2027-01-25

Phase 2: Kubernetes Operator + CRDs (AegisAgent, PermissionManifest, Ledger), SPIRE workload-attestation integration, GPU backends (vLLM/TGI/KServe) against the Backend trait, persistent ledger storage, NetworkPolicies stacked under runtime-level F6 deny. Maps to ADRs 002, 003, 008, 014, 015.

### v3.0.0 — OpenShift Enterprise Runtime
<!-- milestone-id: v3-0-0-openshift-enterprise-runtime -->
- **Status:** pushed (#7)
- **Due:** 2027-04-19

Phase 3: OpenShift Security Context Constraints (SCC) integration, disconnected (air-gapped) install path, GitOps deployment + RBAC mapping, automated CMMC/FedRAMP report exports from the ledger + manifest history. Maps to ADRs 001, 013, 015.

<!-- /LOCAL MILESTONES -->
