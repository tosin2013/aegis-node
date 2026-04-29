# Aegis-Node TODO

**Generated:** 2026-04-27 from PRD v3.0, ADRs 001–016, and project plan (`1777320020862-gygda2e2r`).

This file has two sections:
1. **Phase-grouped task plan (hand-authored)** — the milestone-aligned roadmap below, mapped to the seven baseline milestones in `RELEASE_PLAN.md`.
2. **ADR-Generated Tasks** — paired test+production (TDD) tasks emitted by `mcp__adr-analysis__generate_adr_todo` decomposing every numbered point in each ADR. Re-runs are idempotent and preserve manual edits in the phase-grouped section.

Priority key: **P0** = release blocker · **P1** = required for the phase exit milestone · **P2** = important but deferrable.

---

## Phase 0 — Foundations (target: v0.1.0, 2026-05-25)

- [x] **P0** [ADR-002, ADR-015] Define and commit IPC contract `aegis.proto` (gRPC over UDS local / mTLS K8s). Treat as a versioned API. — `proto/aegis/v1/aegis.proto` + `proto/buf.yaml`
- [x] **P0** [ADR-004] Define and freeze the Permission Manifest JSON Schema v1 (versioned, extensible). Cover scoped permissions, `write_grants`, `approval_required_for`, `extends:` composition. — `schemas/manifest/v1/manifest.schema.json` + 2 examples
- [x] **P0** [ADR-011, ADR-006] Define and freeze the Trajectory Ledger and Access Log JSON-LD `@context` documents (stable URIs, content fields, `prev_hash`, `timestamp`, `agent_identity_hash`). — `schemas/ledger/v1/context.jsonld`
- [x] **P0** [ADR-002] Stand up Rust + Go monorepo with shared CI (cross-language conformance test harness from day one). — workspace + `cmd/`, `pkg/`, `crates/`, `Makefile`, 5 GitHub Actions workflows
- [x] **P0** [ADR-017] Set up Devcontainer + `mise.toml` with pinned Rust/Go/buf/ajv/cosign/oras/golangci-lint versions; CI consumes the same definition. — `.devcontainer/Dockerfile` + `mise.toml` (CI still uses native installs; switch to image is a follow-up)
- [x] **P1** [ADR-017] Build, Cosign-sign, and publish the devbox OCI image; document `oras pull` + `cosign verify` flow for air-gapped reviewers. — image at `ghcr.io/tosin2013/aegis-node-devbox`, Cosign-signed via Sigstore keyless; reviewer flow documented in `docs/SUPPLY_CHAIN.md`.
- [x] **P1** [ADR-002] Document the cross-phase Compatibility Charter (which schemas cannot break across versions). — `docs/COMPATIBILITY_CHARTER.md`
- [x] **P1** [ADR-016] CONTRIBUTING.md + DCO/CLA decision; finalize before first external contribution. — `CONTRIBUTING.md` (DCO over CLA, Apache 2.0).
- [x] **P1** [ADR-013, ADR-017] Document the air-gapped reviewer workflow: `oras pull` + `cosign verify` against the signed devbox image. — `docs/SUPPLY_CHAIN.md`
- [ ] **P2** [ADR-015] Document phase exit criteria publicly so the design-partner review milestone is unambiguous.
- [ ] **P2** [ADR-016] Wire DCO sign-off check into CI (currently maintainer-verified at PR review).

## Phase 1a — Core Security Primitives (✅ shipped: [v0.5.0](https://github.com/tosin2013/aegis-node/releases/tag/v0.5.0), 2026-04-29 — 12 weeks ahead of 2026-07-19 due date)

- [x] **P0** [ADR-003, F1] Implement built-in lightweight CA for local CLI (file-backed, single-tenant). SPIFFE ID format `spiffe://<trust-domain>/agent/<workload-name>/<instance>`. — PR #8 (issue #2)
- [x] **P0** [ADR-003, F1] Bind identity to `(model digest, manifest digest, configuration digest)` triple; halt execution on any digest change. — PR #18 (issue #4)
- [x] **P0** [ADR-004, F2] Implement strict YAML manifest parser with line/column errors; reject any tool call not covered by the manifest. — PR #12 (issue #6)
- [x] **P0** [ADR-004, F2] Wire enforcement at every Rust syscall boundary (file open, network connect, exec). Conformance test against Go-side validator. — PRs #13 (decision engine + network gate), #19 (filesystem gate), #21 (exec_grants schema), #20 (cross-language conformance harness), #31 (runtime mediator) — issues #7, #14, #15, #16
- [x] **P0** [ADR-011, F9] Implement append-only ledger writer in Rust (no delete/update API surface); SHA-256 chain with genesis-zero start. — PR #9 (issue #1) + golden-pinned root in PR #11
- [x] **P0** [ADR-011, F9] Implement `aegis verify <ledger-file>` walk-and-verify CLI with non-zero exit on integrity failure. — PR #11 (issue #5)
- [x] **P0** [ADR-006, F4] Emit structured access log entries at every I/O syscall: agent identity, resource URI, type, bytes, ns timestamp, session ID, F5 reasoning-step ID. Atomic writes only. — PR #10 (typed emitter, issue #3) + PR #31 (mediator wires the emitter at every tool call)
- [ ] **P1** [ADR-004, F2] Ship official manifest templates: `read-only-research`, `single-write-target`, `network-egress-allowlist`, `air-gapped`. — *partial*: read-only-research, single-write-target, agent-with-exec landed in `schemas/manifest/v1/examples/`; allowlist + air-gapped variants pending.
- [ ] **P1** [ADR-011, F9] Add RFC 3161 TSA notarization integration (optional, at session close). — deferred.
- [ ] **P2** [ADR-006] Ship reference Splunk + Elastic dashboards for ingestion. — deferred.

### Bonus shipped under v0.5.0 (not in original Phase 1a plan)

- [x] **P0** F0 runtime — Session boot/shutdown lifecycle (PR #30, issue #24).
- [x] **P0** F0 runtime — Per-tool-call mediator (PR #31, issue #25). Closes #3 + #7 by transitivity.
- [x] **P0** F0 runtime — `aegis run` CLI subcommand (PR #32, issue #28).
- [x] **P0** F0 runtime — End-to-end golden-ledger conformance (PR #33, issue #29).
- [x] **P1** F1 — Go-callable FFI surface for in-process SVID issuance (PR #22, issue #17).

## Phase 1b — Reasoning + Approval (target: v0.8.0, 2026-08-31)

- [ ] **P0** [ADR-007, F5] Implement Reasoning Capturer that intercepts LLM tool-selection output and writes a structured trajectory entry **before** the action handler is invoked. Failure to write blocks the action.
- [ ] **P0** [ADR-007, F5] Define + implement reasoning-step ID linkage between F5 trajectory entries and F4 access entries.
- [ ] **P0** [ADR-005, F3] Implement three approval channels: TTY-attached CLI prompt, localhost-only web UI (session-token gated), signed-API (mTLS + identity).
- [ ] **P0** [ADR-005, F3] Define action-summary template (plain English + structured action metadata); ledger schema events: `ApprovalRequested/Granted/Rejected/TimedOut`.
- [ ] **P0** [ADR-008, F6] Implement Rust network gate wrapping `std::net` + the inference engine HTTP clients. Non-allowlisted connect → deterministic error + critical F9 violation + halt.
- [ ] **P0** [ADR-008, F6] Define + emit signed end-of-session network attestation (zero-connections or allowed-only).
- [ ] **P0** [ADR-009, F7] Implement `write_grants` enforcement at every mutation syscall (file write/truncate/rename/unlink, allowed-host POST/PUT/PATCH/DELETE, plugin-defined mutation tools).
- [ ] **P0** [ADR-009, F7] Implement time-bounded grants using monotonic clock + validated wall clock at session start; expired grants emit F9 violations.
- [ ] **P1** [ADR-005, F3] Default approval timeout (120 s) and refuse-on-timeout semantics.
- [ ] **P1** [ADR-008, F6] Document tool-author contract for tools that bring their own network stack (wrapping or rejection rules).
- [ ] **P2** [ADR-009, F7] Compose `approval_required` write grants with F3 approval gate (manifest field).

## Phase 1c — Tooling and Replay (target: v0.9.0, 2026-10-05)

- [ ] **P0** [ADR-018, F2/F5] Adopt MCP as the agent-to-tool protocol per ADR-018. Manifest gains optional `tools.mcp[]` (closed-by-default allowlist of `{server_name, server_uri, allowed_tools}`); mediator gains `mediate_mcp_tool_call` that routes side-effects through existing `mediate_*` methods; F5 reasoning entries' `toolsConsidered`/`toolSelected` carry MCP tool names; cross-language conformance battery extended.
- [ ] **P0** [ADR-012, F10] Implement `aegis validate` schema validator + linter (start with ~10 high-value rules: overly broad paths, wildcard tools, missing approval gates on writes, unjustified network grants, etc.).
- [x] **P0** [ADR-012, F10] Implement composition + inheritance: `extends:` links with parent-permission enforcement (child cannot exceed parent). — landed early under v0.5.0: PR #12 (issue #6) implements the resolver in `pkg/manifest/extends.go` with narrowing checks for fs paths, network policies, apis, write_grants, exec_grants, and approval classes.
- [ ] **P0** [ADR-012, F10] Output formats: GitHub Actions annotations, JUnit XML, plain text, JSON. Generate human-readable policy summary report.
- [ ] **P0** [ADR-010, F8] Build offline single-file HTML replay viewer (no `fetch()`/CDN/external calls). Synchronized timeline of F5 reasoning + F4 access + F3 approval events.
- [ ] **P0** [ADR-010, F8] Add F9 chain verification on viewer load; broken chain → prominent integrity warning.
- [ ] **P0** [ADR-013] Implement `aegis pull <ref>` with embedded OCI client (or initial shell-out to `oras`). Cosign signature + SHA-256 verification at load; refuse boot on missing/invalid signature.
- [ ] **P0** [ADR-013] Verification covers GGUF file *and* chat-template metadata (defends against template-only poisoning).
- [ ] **P0** [ADR-014] Build Rust FFI binding to `llama.cpp` with strict safety wrapper (no unwrap on FFI returns; defined panic behavior). Pin a known-good upstream revision.
- [ ] **P0** [ADR-014] Define `Backend` trait abstraction and implement `LlamaCppBackend` against it.
- [ ] **P1** [ADR-014] Define determinism config knobs (seed, temperature, top-p) exposed through manifest.
- [ ] **P1** [ADR-013] Document operator workflow: download upstream, scan, sign with org Cosign key, push to internal registry.
- [ ] **P2** [ADR-010, F8] CI test: fixed ledger fixture renders to fixed DOM snapshot.

## Phase 1 GA — Security Review Milestone (target: v1.0.0, 2026-11-02 — CMMC deadline)

- [ ] **P0** [ADR-001] Build the auditor evidence package generator: combine manifest + ledger + replay viewer + policy summary into a single signed bundle.
- [ ] **P0** [ADR-001] Cross-language conformance suite green: every manifest accepted by the Go validator is enforced consistently by the Rust runtime, and vice versa.
- [ ] **P0** [ADR-001] Pass a real security review with at least one design-partner organization (defense beachhead preferred).
- [ ] **P0** [ADR-016] v1.0.0 community release under Apache 2.0; tag, sign, publish.
- [ ] **P1** [ADR-001] Public security documentation organized by the F1–F10 questions (one section per question, mapping to features and ADRs).
- [ ] **P1** [ADR-001] Establish PR-review rule: every new feature must reference the security-review question it answers (or be marked post-MVP).

## Phase 2 — Kubernetes Runtime (target: v2.0.0, 2027-01-25)

- [ ] **P0** [ADR-015] Build Kubernetes Operator + CRDs (`AegisAgent`, `PermissionManifest`, `Ledger`).
- [ ] **P0** [ADR-003] Replace local CA with SPIRE workload-attestation integration; bind ServiceAccount → SPIFFE ID.
- [ ] **P0** [ADR-014, ADR-015] Implement GPU backend(s) against the `Backend` trait: at minimum vLLM; design space for TGI / KServe.
- [ ] **P0** [ADR-008] Stack F6 runtime-deny under cluster NetworkPolicies (defense in depth).
- [ ] **P0** [ADR-011] Persistent ledger storage strategy (PVC + retention/archival).

## Phase 3 — OpenShift Enterprise (target: v3.0.0, 2027-04-19)

- [ ] **P0** [ADR-015] OpenShift Security Context Constraints (SCC) integration.
- [ ] **P0** [ADR-013, ADR-015] Disconnected (air-gapped) installation path; document end-to-end with Harbor / internal OCI.
- [ ] **P0** [ADR-015] GitOps deployment + RBAC mapping (Argo CD / OpenShift GitOps).
- [ ] **P0** [ADR-001, ADR-015] Automated CMMC / FedRAMP report exports from the ledger + manifest history.

---

## Cross-Cutting / Always-On

- [ ] [ADR-001] PR-review gate: any feature without an F1–F10 mapping is post-MVP unless explicitly justified.
- [ ] [ADR-002] Maintain conformance test suite at every Go ↔ Rust boundary change.
- [ ] [ADR-014] Track upstream `llama.cpp` revisions; bumps are deliberate and reviewed.
- [ ] [ADR-016] Open-vs-commercial boundary: any new feature lands with a license-tier classification before merge.

<!-- ADR-GENERATED-TASKS -->

<!-- This section is managed by `generate_adr_todo`. Tasks outside this
     bounded block are preserved verbatim across re-runs. Toggling Status -->
<!-- generated-at: 2026-04-27T21:18:26.632Z -->

## ADR-Generated Tasks

## Write tests for: Every feature in MVP must answer one of the ten security review questions. Any feature that does not is post-MVP.
<!-- task-id: 001-zero-trust-security-review-as-product-specification-md/every-feature-in-mvp-must-answer-one-of-the-ten-security-review-questions-any-fe/test -->
<!-- adr: 001-zero-trust-security-review-as-product-specification.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

The product specification of Aegis-Node v1 is the zero-trust security review checklist itself. Specifically:

1. Every feature in MVP must answer one of the ten security review questions. Any feature that does not is post-MVP.
2. Any feature that answers one of the questions is non-negotiable for v1.
3. Architecture trade-offs are evaluated against "review passability" before any other criterion (performance, ergonomics, cost).
4. Marketing, sales, and developer relations all anchor on "agents that survive the security review" rather than "AI agent framework."

## Every feature in MVP must answer one of the ten security review questions. Any feature that does not is post-MVP.
<!-- task-id: 001-zero-trust-security-review-as-product-specification-md/every-feature-in-mvp-must-answer-one-of-the-ten-security-review-questions-any-fe/production -->
<!-- adr: 001-zero-trust-security-review-as-product-specification.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

The product specification of Aegis-Node v1 is the zero-trust security review checklist itself. Specifically:

1. Every feature in MVP must answer one of the ten security review questions. Any feature that does not is post-MVP.
2. Any feature that answers one of the questions is non-negotiable for v1.
3. Architecture trade-offs are evaluated against "review passability" before any other criterion (performance, ergonomics, cost).
4. Marketing, sales, and developer relations all anchor on "agents that survive the security review" rather than "AI agent framework."

## Write tests for: Any feature that answers one of the questions is non-negotiable for v1.
<!-- task-id: 001-zero-trust-security-review-as-product-specification-md/any-feature-that-answers-one-of-the-questions-is-non-negotiable-for-v1/test -->
<!-- adr: 001-zero-trust-security-review-as-product-specification.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

The product specification of Aegis-Node v1 is the zero-trust security review checklist itself. Specifically:

1. Every feature in MVP must answer one of the ten security review questions. Any feature that does not is post-MVP.
2. Any feature that answers one of the questions is non-negotiable for v1.
3. Architecture trade-offs are evaluated against "review passability" before any other criterion (performance, ergonomics, cost).
4. Marketing, sales, and developer relations all anchor on "agents that survive the security review" rather than "AI agent framework."

## Any feature that answers one of the questions is non-negotiable for v1.
<!-- task-id: 001-zero-trust-security-review-as-product-specification-md/any-feature-that-answers-one-of-the-questions-is-non-negotiable-for-v1/production -->
<!-- adr: 001-zero-trust-security-review-as-product-specification.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

The product specification of Aegis-Node v1 is the zero-trust security review checklist itself. Specifically:

1. Every feature in MVP must answer one of the ten security review questions. Any feature that does not is post-MVP.
2. Any feature that answers one of the questions is non-negotiable for v1.
3. Architecture trade-offs are evaluated against "review passability" before any other criterion (performance, ergonomics, cost).
4. Marketing, sales, and developer relations all anchor on "agents that survive the security review" rather than "AI agent framework."

## Write tests for: Architecture trade-offs are evaluated against "review passability" before any other criterion (performance, ergonomics, cost).
<!-- task-id: 001-zero-trust-security-review-as-product-specification-md/architecture-trade-offs-are-evaluated-against-review-passability-before-any-othe/test -->
<!-- adr: 001-zero-trust-security-review-as-product-specification.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

The product specification of Aegis-Node v1 is the zero-trust security review checklist itself. Specifically:

1. Every feature in MVP must answer one of the ten security review questions. Any feature that does not is post-MVP.
2. Any feature that answers one of the questions is non-negotiable for v1.
3. Architecture trade-offs are evaluated against "review passability" before any other criterion (performance, ergonomics, cost).
4. Marketing, sales, and developer relations all anchor on "agents that survive the security review" rather than "AI agent framework."

## Architecture trade-offs are evaluated against "review passability" before any other criterion (performance, ergonomics, cost).
<!-- task-id: 001-zero-trust-security-review-as-product-specification-md/architecture-trade-offs-are-evaluated-against-review-passability-before-any-othe/production -->
<!-- adr: 001-zero-trust-security-review-as-product-specification.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

The product specification of Aegis-Node v1 is the zero-trust security review checklist itself. Specifically:

1. Every feature in MVP must answer one of the ten security review questions. Any feature that does not is post-MVP.
2. Any feature that answers one of the questions is non-negotiable for v1.
3. Architecture trade-offs are evaluated against "review passability" before any other criterion (performance, ergonomics, cost).
4. Marketing, sales, and developer relations all anchor on "agents that survive the security review" rather than "AI agent framework."

## Write tests for: Marketing, sales, and developer relations all anchor on "agents that survive the security review" rather than "AI agent framework."
<!-- task-id: 001-zero-trust-security-review-as-product-specification-md/marketing-sales-and-developer-relations-all-anchor-on-agents-that-survive-the-se/test -->
<!-- adr: 001-zero-trust-security-review-as-product-specification.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

The product specification of Aegis-Node v1 is the zero-trust security review checklist itself. Specifically:

1. Every feature in MVP must answer one of the ten security review questions. Any feature that does not is post-MVP.
2. Any feature that answers one of the questions is non-negotiable for v1.
3. Architecture trade-offs are evaluated against "review passability" before any other criterion (performance, ergonomics, cost).
4. Marketing, sales, and developer relations all anchor on "agents that survive the security review" rather than "AI agent framework."

## Marketing, sales, and developer relations all anchor on "agents that survive the security review" rather than "AI agent framework."
<!-- task-id: 001-zero-trust-security-review-as-product-specification-md/marketing-sales-and-developer-relations-all-anchor-on-agents-that-survive-the-se/production -->
<!-- adr: 001-zero-trust-security-review-as-product-specification.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

The product specification of Aegis-Node v1 is the zero-trust security review checklist itself. Specifically:

1. Every feature in MVP must answer one of the ten security review questions. Any feature that does not is post-MVP.
2. Any feature that answers one of the questions is non-negotiable for v1.
3. Architecture trade-offs are evaluated against "review passability" before any other criterion (performance, ergonomics, cost).
4. Marketing, sales, and developer relations all anchor on "agents that survive the security review" rather than "AI agent framework."

## Write tests for: **Inference Engine — Rust** (binding to `llama.cpp` via FFI). Owns: model loading, tokenization, generation, network-deny enforcement (F6), filesystem sandbox enforcement, identity token verification at the runtime boundary.
<!-- task-id: 002-split-language-architecture-rust-and-go-md/inference-engine-rust-binding-to-llama-cpp-via-ffi-owns-model-loading-tokenizati/test -->
<!-- adr: 002-split-language-architecture-rust-and-go.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Adopt a deliberate split-language architecture:

1. **Inference Engine — Rust** (binding to `llama.cpp` via FFI). Owns: model loading, tokenization, generation, network-deny enforcement (F6), filesystem sandbox enforcement, identity token verification at the runtime boundary.
2. **Control Plane & Orchestrator — Go.** Owns: CLI (`aegis ...`), API server, policy validator (F10), trajectory ledger writer (F9), Kubernetes Operator, OpenShift SCC integration, manifest parser, replay viewer build pipeline.

The two halves communicate over a stable, versioned IPC contract (gRPC over Unix domain socket on local; gRPC over mTLS in K8s) with the Permission Manifest serving as the shared schema.

## **Inference Engine — Rust** (binding to `llama.cpp` via FFI). Owns: model loading, tokenization, generation, network-deny enforcement (F6), filesystem sandbox enforcement, identity token verification at the runtime boundary.
<!-- task-id: 002-split-language-architecture-rust-and-go-md/inference-engine-rust-binding-to-llama-cpp-via-ffi-owns-model-loading-tokenizati/production -->
<!-- adr: 002-split-language-architecture-rust-and-go.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Adopt a deliberate split-language architecture:

1. **Inference Engine — Rust** (binding to `llama.cpp` via FFI). Owns: model loading, tokenization, generation, network-deny enforcement (F6), filesystem sandbox enforcement, identity token verification at the runtime boundary.
2. **Control Plane & Orchestrator — Go.** Owns: CLI (`aegis ...`), API server, policy validator (F10), trajectory ledger writer (F9), Kubernetes Operator, OpenShift SCC integration, manifest parser, replay viewer build pipeline.

The two halves communicate over a stable, versioned IPC contract (gRPC over Unix domain socket on local; gRPC over mTLS in K8s) with the Permission Manifest serving as the shared schema.

## Write tests for: **Control Plane & Orchestrator — Go.** Owns: CLI (`aegis ...`), API server, policy validator (F10), trajectory ledger writer (F9), Kubernetes Operator, OpenShift SCC integration, manifest parser, replay viewer build pipeline.
<!-- task-id: 002-split-language-architecture-rust-and-go-md/control-plane-orchestrator-go-owns-cli-aegis-api-server-policy-validator-f10-tra/test -->
<!-- adr: 002-split-language-architecture-rust-and-go.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Adopt a deliberate split-language architecture:

1. **Inference Engine — Rust** (binding to `llama.cpp` via FFI). Owns: model loading, tokenization, generation, network-deny enforcement (F6), filesystem sandbox enforcement, identity token verification at the runtime boundary.
2. **Control Plane & Orchestrator — Go.** Owns: CLI (`aegis ...`), API server, policy validator (F10), trajectory ledger writer (F9), Kubernetes Operator, OpenShift SCC integration, manifest parser, replay viewer build pipeline.

The two halves communicate over a stable, versioned IPC contract (gRPC over Unix domain socket on local; gRPC over mTLS in K8s) with the Permission Manifest serving as the shared schema.

## **Control Plane & Orchestrator — Go.** Owns: CLI (`aegis ...`), API server, policy validator (F10), trajectory ledger writer (F9), Kubernetes Operator, OpenShift SCC integration, manifest parser, replay viewer build pipeline.
<!-- task-id: 002-split-language-architecture-rust-and-go-md/control-plane-orchestrator-go-owns-cli-aegis-api-server-policy-validator-f10-tra/production -->
<!-- adr: 002-split-language-architecture-rust-and-go.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Adopt a deliberate split-language architecture:

1. **Inference Engine — Rust** (binding to `llama.cpp` via FFI). Owns: model loading, tokenization, generation, network-deny enforcement (F6), filesystem sandbox enforcement, identity token verification at the runtime boundary.
2. **Control Plane & Orchestrator — Go.** Owns: CLI (`aegis ...`), API server, policy validator (F10), trajectory ledger writer (F9), Kubernetes Operator, OpenShift SCC integration, manifest parser, replay viewer build pipeline.

The two halves communicate over a stable, versioned IPC contract (gRPC over Unix domain socket on local; gRPC over mTLS in K8s) with the Permission Manifest serving as the shared schema.

## Write tests for: Is bound to a triple `(model digest, manifest digest, configuration digest)` — changing any of them invalidates the identity and halts execution.
<!-- task-id: 003-cryptographic-workload-identity-spiffe-spire-md/is-bound-to-a-triple-model-digest-manifest-digest-configuration-digest-changing-/test -->
<!-- adr: 003-cryptographic-workload-identity-spiffe-spire.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Every Aegis-Node agent instance receives a cryptographically signed workload identity at instantiation. The identity:

1. Is bound to a triple `(model digest, manifest digest, configuration digest)` — changing any of them invalidates the identity and halts execution.
2. Conforms to SPIFFE workload identity standards: SPIFFE ID format, X.509-SVID or JWT-SVID issuance, compatible with SPIRE-issued identities for enterprise deployments.
3. Signs every action recorded in the Trajectory Ledger (F9), creating an unambiguous chain of attribution.
4. Is verifiable offline: the runtime ships with a built-in CA mode for local CLI use; in Kubernetes deployments, identities are issued by a SPIRE server in the cluster.

## Is bound to a triple `(model digest, manifest digest, configuration digest)` — changing any of them invalidates the identity and halts execution.
<!-- task-id: 003-cryptographic-workload-identity-spiffe-spire-md/is-bound-to-a-triple-model-digest-manifest-digest-configuration-digest-changing-/production -->
<!-- adr: 003-cryptographic-workload-identity-spiffe-spire.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Every Aegis-Node agent instance receives a cryptographically signed workload identity at instantiation. The identity:

1. Is bound to a triple `(model digest, manifest digest, configuration digest)` — changing any of them invalidates the identity and halts execution.
2. Conforms to SPIFFE workload identity standards: SPIFFE ID format, X.509-SVID or JWT-SVID issuance, compatible with SPIRE-issued identities for enterprise deployments.
3. Signs every action recorded in the Trajectory Ledger (F9), creating an unambiguous chain of attribution.
4. Is verifiable offline: the runtime ships with a built-in CA mode for local CLI use; in Kubernetes deployments, identities are issued by a SPIRE server in the cluster.

## Write tests for: Conforms to SPIFFE workload identity standards: SPIFFE ID format, X.509-SVID or JWT-SVID issuance, compatible with SPIRE-issued identities for enterprise deployments.
<!-- task-id: 003-cryptographic-workload-identity-spiffe-spire-md/conforms-to-spiffe-workload-identity-standards-spiffe-id-format-x-509-svid-or-jw/test -->
<!-- adr: 003-cryptographic-workload-identity-spiffe-spire.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Every Aegis-Node agent instance receives a cryptographically signed workload identity at instantiation. The identity:

1. Is bound to a triple `(model digest, manifest digest, configuration digest)` — changing any of them invalidates the identity and halts execution.
2. Conforms to SPIFFE workload identity standards: SPIFFE ID format, X.509-SVID or JWT-SVID issuance, compatible with SPIRE-issued identities for enterprise deployments.
3. Signs every action recorded in the Trajectory Ledger (F9), creating an unambiguous chain of attribution.
4. Is verifiable offline: the runtime ships with a built-in CA mode for local CLI use; in Kubernetes deployments, identities are issued by a SPIRE server in the cluster.

## Conforms to SPIFFE workload identity standards: SPIFFE ID format, X.509-SVID or JWT-SVID issuance, compatible with SPIRE-issued identities for enterprise deployments.
<!-- task-id: 003-cryptographic-workload-identity-spiffe-spire-md/conforms-to-spiffe-workload-identity-standards-spiffe-id-format-x-509-svid-or-jw/production -->
<!-- adr: 003-cryptographic-workload-identity-spiffe-spire.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Every Aegis-Node agent instance receives a cryptographically signed workload identity at instantiation. The identity:

1. Is bound to a triple `(model digest, manifest digest, configuration digest)` — changing any of them invalidates the identity and halts execution.
2. Conforms to SPIFFE workload identity standards: SPIFFE ID format, X.509-SVID or JWT-SVID issuance, compatible with SPIRE-issued identities for enterprise deployments.
3. Signs every action recorded in the Trajectory Ledger (F9), creating an unambiguous chain of attribution.
4. Is verifiable offline: the runtime ships with a built-in CA mode for local CLI use; in Kubernetes deployments, identities are issued by a SPIRE server in the cluster.

## Write tests for: Signs every action recorded in the Trajectory Ledger (F9), creating an unambiguous chain of attribution.
<!-- task-id: 003-cryptographic-workload-identity-spiffe-spire-md/signs-every-action-recorded-in-the-trajectory-ledger-f9-creating-an-unambiguous-/test -->
<!-- adr: 003-cryptographic-workload-identity-spiffe-spire.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Every Aegis-Node agent instance receives a cryptographically signed workload identity at instantiation. The identity:

1. Is bound to a triple `(model digest, manifest digest, configuration digest)` — changing any of them invalidates the identity and halts execution.
2. Conforms to SPIFFE workload identity standards: SPIFFE ID format, X.509-SVID or JWT-SVID issuance, compatible with SPIRE-issued identities for enterprise deployments.
3. Signs every action recorded in the Trajectory Ledger (F9), creating an unambiguous chain of attribution.
4. Is verifiable offline: the runtime ships with a built-in CA mode for local CLI use; in Kubernetes deployments, identities are issued by a SPIRE server in the cluster.

## Signs every action recorded in the Trajectory Ledger (F9), creating an unambiguous chain of attribution.
<!-- task-id: 003-cryptographic-workload-identity-spiffe-spire-md/signs-every-action-recorded-in-the-trajectory-ledger-f9-creating-an-unambiguous-/production -->
<!-- adr: 003-cryptographic-workload-identity-spiffe-spire.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Every Aegis-Node agent instance receives a cryptographically signed workload identity at instantiation. The identity:

1. Is bound to a triple `(model digest, manifest digest, configuration digest)` — changing any of them invalidates the identity and halts execution.
2. Conforms to SPIFFE workload identity standards: SPIFFE ID format, X.509-SVID or JWT-SVID issuance, compatible with SPIRE-issued identities for enterprise deployments.
3. Signs every action recorded in the Trajectory Ledger (F9), creating an unambiguous chain of attribution.
4. Is verifiable offline: the runtime ships with a built-in CA mode for local CLI use; in Kubernetes deployments, identities are issued by a SPIRE server in the cluster.

## Write tests for: Is verifiable offline: the runtime ships with a built-in CA mode for local CLI use; in Kubernetes deployments, identities are issued by a SPIRE server in the cluster.
<!-- task-id: 003-cryptographic-workload-identity-spiffe-spire-md/is-verifiable-offline-the-runtime-ships-with-a-built-in-ca-mode-for-local-cli-us/test -->
<!-- adr: 003-cryptographic-workload-identity-spiffe-spire.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Every Aegis-Node agent instance receives a cryptographically signed workload identity at instantiation. The identity:

1. Is bound to a triple `(model digest, manifest digest, configuration digest)` — changing any of them invalidates the identity and halts execution.
2. Conforms to SPIFFE workload identity standards: SPIFFE ID format, X.509-SVID or JWT-SVID issuance, compatible with SPIRE-issued identities for enterprise deployments.
3. Signs every action recorded in the Trajectory Ledger (F9), creating an unambiguous chain of attribution.
4. Is verifiable offline: the runtime ships with a built-in CA mode for local CLI use; in Kubernetes deployments, identities are issued by a SPIRE server in the cluster.

## Is verifiable offline: the runtime ships with a built-in CA mode for local CLI use; in Kubernetes deployments, identities are issued by a SPIRE server in the cluster.
<!-- task-id: 003-cryptographic-workload-identity-spiffe-spire-md/is-verifiable-offline-the-runtime-ships-with-a-built-in-ca-mode-for-local-cli-us/production -->
<!-- adr: 003-cryptographic-workload-identity-spiffe-spire.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Every Aegis-Node agent instance receives a cryptographically signed workload identity at instantiation. The identity:

1. Is bound to a triple `(model digest, manifest digest, configuration digest)` — changing any of them invalidates the identity and halts execution.
2. Conforms to SPIFFE workload identity standards: SPIFFE ID format, X.509-SVID or JWT-SVID issuance, compatible with SPIRE-issued identities for enterprise deployments.
3. Signs every action recorded in the Trajectory Ledger (F9), creating an unambiguous chain of attribution.
4. Is verifiable offline: the runtime ships with a built-in CA mode for local CLI use; in Kubernetes deployments, identities are issued by a SPIRE server in the cluster.

## Write tests for: **Closed by default.** No "allow all" mode. Anything not listed is forbidden.
<!-- task-id: 004-declarative-yaml-permission-manifest-md/closed-by-default-no-allow-all-mode-anything-not-listed-is-forbidden/test -->
<!-- adr: 004-declarative-yaml-permission-manifest.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

The Permission Manifest is a YAML file checked into source control next to the agent code. It is the single source of truth for what an agent may do.

Key properties:
1. **Closed by default.** No "allow all" mode. Anything not listed is forbidden.
2. **Scoped permissions.** `fs:read:/data/reports`, `fs:write:none`, `network:outbound:deny`, `apis: [<allowlist>]`, etc.
3. **Versioned.** Every manifest carries a `schemaVersion` and an `agentVersion`; the runtime refuses to load a manifest with an unsupported schema.
4. **Strict enforcement.** Any tool call not covered by the manifest is rejected with a logged violation in the Trajectory Ledger (F9). Violations are first-class events, not silent failures.
5. **Composable inheritance.** Org-level base policies can be extended by team and agent-level policies; the validator (F10) enforces that child policies cannot exceed parent permissions.

## **Closed by default.** No "allow all" mode. Anything not listed is forbidden.
<!-- task-id: 004-declarative-yaml-permission-manifest-md/closed-by-default-no-allow-all-mode-anything-not-listed-is-forbidden/production -->
<!-- adr: 004-declarative-yaml-permission-manifest.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

The Permission Manifest is a YAML file checked into source control next to the agent code. It is the single source of truth for what an agent may do.

Key properties:
1. **Closed by default.** No "allow all" mode. Anything not listed is forbidden.
2. **Scoped permissions.** `fs:read:/data/reports`, `fs:write:none`, `network:outbound:deny`, `apis: [<allowlist>]`, etc.
3. **Versioned.** Every manifest carries a `schemaVersion` and an `agentVersion`; the runtime refuses to load a manifest with an unsupported schema.
4. **Strict enforcement.** Any tool call not covered by the manifest is rejected with a logged violation in the Trajectory Ledger (F9). Violations are first-class events, not silent failures.
5. **Composable inheritance.** Org-level base policies can be extended by team and agent-level policies; the validator (F10) enforces that child policies cannot exceed parent permissions.

## Write tests for: **Scoped permissions.** `fs:read:/data/reports`, `fs:write:none`, `network:outbound:deny`, `apis: [<allowlist>]`, etc.
<!-- task-id: 004-declarative-yaml-permission-manifest-md/scoped-permissions-fs-read-data-reports-fs-write-none-network-outbound-deny-apis/test -->
<!-- adr: 004-declarative-yaml-permission-manifest.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

The Permission Manifest is a YAML file checked into source control next to the agent code. It is the single source of truth for what an agent may do.

Key properties:
1. **Closed by default.** No "allow all" mode. Anything not listed is forbidden.
2. **Scoped permissions.** `fs:read:/data/reports`, `fs:write:none`, `network:outbound:deny`, `apis: [<allowlist>]`, etc.
3. **Versioned.** Every manifest carries a `schemaVersion` and an `agentVersion`; the runtime refuses to load a manifest with an unsupported schema.
4. **Strict enforcement.** Any tool call not covered by the manifest is rejected with a logged violation in the Trajectory Ledger (F9). Violations are first-class events, not silent failures.
5. **Composable inheritance.** Org-level base policies can be extended by team and agent-level policies; the validator (F10) enforces that child policies cannot exceed parent permissions.

## **Scoped permissions.** `fs:read:/data/reports`, `fs:write:none`, `network:outbound:deny`, `apis: [<allowlist>]`, etc.
<!-- task-id: 004-declarative-yaml-permission-manifest-md/scoped-permissions-fs-read-data-reports-fs-write-none-network-outbound-deny-apis/production -->
<!-- adr: 004-declarative-yaml-permission-manifest.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

The Permission Manifest is a YAML file checked into source control next to the agent code. It is the single source of truth for what an agent may do.

Key properties:
1. **Closed by default.** No "allow all" mode. Anything not listed is forbidden.
2. **Scoped permissions.** `fs:read:/data/reports`, `fs:write:none`, `network:outbound:deny`, `apis: [<allowlist>]`, etc.
3. **Versioned.** Every manifest carries a `schemaVersion` and an `agentVersion`; the runtime refuses to load a manifest with an unsupported schema.
4. **Strict enforcement.** Any tool call not covered by the manifest is rejected with a logged violation in the Trajectory Ledger (F9). Violations are first-class events, not silent failures.
5. **Composable inheritance.** Org-level base policies can be extended by team and agent-level policies; the validator (F10) enforces that child policies cannot exceed parent permissions.

## Write tests for: **Versioned.** Every manifest carries a `schemaVersion` and an `agentVersion`; the runtime refuses to load a manifest with an unsupported schema.
<!-- task-id: 004-declarative-yaml-permission-manifest-md/versioned-every-manifest-carries-a-schemaversion-and-an-agentversion-the-runtime/test -->
<!-- adr: 004-declarative-yaml-permission-manifest.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

The Permission Manifest is a YAML file checked into source control next to the agent code. It is the single source of truth for what an agent may do.

Key properties:
1. **Closed by default.** No "allow all" mode. Anything not listed is forbidden.
2. **Scoped permissions.** `fs:read:/data/reports`, `fs:write:none`, `network:outbound:deny`, `apis: [<allowlist>]`, etc.
3. **Versioned.** Every manifest carries a `schemaVersion` and an `agentVersion`; the runtime refuses to load a manifest with an unsupported schema.
4. **Strict enforcement.** Any tool call not covered by the manifest is rejected with a logged violation in the Trajectory Ledger (F9). Violations are first-class events, not silent failures.
5. **Composable inheritance.** Org-level base policies can be extended by team and agent-level policies; the validator (F10) enforces that child policies cannot exceed parent permissions.

## **Versioned.** Every manifest carries a `schemaVersion` and an `agentVersion`; the runtime refuses to load a manifest with an unsupported schema.
<!-- task-id: 004-declarative-yaml-permission-manifest-md/versioned-every-manifest-carries-a-schemaversion-and-an-agentversion-the-runtime/production -->
<!-- adr: 004-declarative-yaml-permission-manifest.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

The Permission Manifest is a YAML file checked into source control next to the agent code. It is the single source of truth for what an agent may do.

Key properties:
1. **Closed by default.** No "allow all" mode. Anything not listed is forbidden.
2. **Scoped permissions.** `fs:read:/data/reports`, `fs:write:none`, `network:outbound:deny`, `apis: [<allowlist>]`, etc.
3. **Versioned.** Every manifest carries a `schemaVersion` and an `agentVersion`; the runtime refuses to load a manifest with an unsupported schema.
4. **Strict enforcement.** Any tool call not covered by the manifest is rejected with a logged violation in the Trajectory Ledger (F9). Violations are first-class events, not silent failures.
5. **Composable inheritance.** Org-level base policies can be extended by team and agent-level policies; the validator (F10) enforces that child policies cannot exceed parent permissions.

## Write tests for: **Strict enforcement.** Any tool call not covered by the manifest is rejected with a logged violation in the Trajectory Ledger (F9). Violations are first-class events, not silent failures.
<!-- task-id: 004-declarative-yaml-permission-manifest-md/strict-enforcement-any-tool-call-not-covered-by-the-manifest-is-rejected-with-a-/test -->
<!-- adr: 004-declarative-yaml-permission-manifest.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

The Permission Manifest is a YAML file checked into source control next to the agent code. It is the single source of truth for what an agent may do.

Key properties:
1. **Closed by default.** No "allow all" mode. Anything not listed is forbidden.
2. **Scoped permissions.** `fs:read:/data/reports`, `fs:write:none`, `network:outbound:deny`, `apis: [<allowlist>]`, etc.
3. **Versioned.** Every manifest carries a `schemaVersion` and an `agentVersion`; the runtime refuses to load a manifest with an unsupported schema.
4. **Strict enforcement.** Any tool call not covered by the manifest is rejected with a logged violation in the Trajectory Ledger (F9). Violations are first-class events, not silent failures.
5. **Composable inheritance.** Org-level base policies can be extended by team and agent-level policies; the validator (F10) enforces that child policies cannot exceed parent permissions.

## **Strict enforcement.** Any tool call not covered by the manifest is rejected with a logged violation in the Trajectory Ledger (F9). Violations are first-class events, not silent failures.
<!-- task-id: 004-declarative-yaml-permission-manifest-md/strict-enforcement-any-tool-call-not-covered-by-the-manifest-is-rejected-with-a-/production -->
<!-- adr: 004-declarative-yaml-permission-manifest.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

The Permission Manifest is a YAML file checked into source control next to the agent code. It is the single source of truth for what an agent may do.

Key properties:
1. **Closed by default.** No "allow all" mode. Anything not listed is forbidden.
2. **Scoped permissions.** `fs:read:/data/reports`, `fs:write:none`, `network:outbound:deny`, `apis: [<allowlist>]`, etc.
3. **Versioned.** Every manifest carries a `schemaVersion` and an `agentVersion`; the runtime refuses to load a manifest with an unsupported schema.
4. **Strict enforcement.** Any tool call not covered by the manifest is rejected with a logged violation in the Trajectory Ledger (F9). Violations are first-class events, not silent failures.
5. **Composable inheritance.** Org-level base policies can be extended by team and agent-level policies; the validator (F10) enforces that child policies cannot exceed parent permissions.

## Write tests for: **Composable inheritance.** Org-level base policies can be extended by team and agent-level policies; the validator (F10) enforces that child policies cannot exceed parent permissions.
<!-- task-id: 004-declarative-yaml-permission-manifest-md/composable-inheritance-org-level-base-policies-can-be-extended-by-team-and-agent/test -->
<!-- adr: 004-declarative-yaml-permission-manifest.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

The Permission Manifest is a YAML file checked into source control next to the agent code. It is the single source of truth for what an agent may do.

Key properties:
1. **Closed by default.** No "allow all" mode. Anything not listed is forbidden.
2. **Scoped permissions.** `fs:read:/data/reports`, `fs:write:none`, `network:outbound:deny`, `apis: [<allowlist>]`, etc.
3. **Versioned.** Every manifest carries a `schemaVersion` and an `agentVersion`; the runtime refuses to load a manifest with an unsupported schema.
4. **Strict enforcement.** Any tool call not covered by the manifest is rejected with a logged violation in the Trajectory Ledger (F9). Violations are first-class events, not silent failures.
5. **Composable inheritance.** Org-level base policies can be extended by team and agent-level policies; the validator (F10) enforces that child policies cannot exceed parent permissions.

## **Composable inheritance.** Org-level base policies can be extended by team and agent-level policies; the validator (F10) enforces that child policies cannot exceed parent permissions.
<!-- task-id: 004-declarative-yaml-permission-manifest-md/composable-inheritance-org-level-base-policies-can-be-extended-by-team-and-agent/production -->
<!-- adr: 004-declarative-yaml-permission-manifest.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

The Permission Manifest is a YAML file checked into source control next to the agent code. It is the single source of truth for what an agent may do.

Key properties:
1. **Closed by default.** No "allow all" mode. Anything not listed is forbidden.
2. **Scoped permissions.** `fs:read:/data/reports`, `fs:write:none`, `network:outbound:deny`, `apis: [<allowlist>]`, etc.
3. **Versioned.** Every manifest carries a `schemaVersion` and an `agentVersion`; the runtime refuses to load a manifest with an unsupported schema.
4. **Strict enforcement.** Any tool call not covered by the manifest is rejected with a logged violation in the Trajectory Ledger (F9). Violations are first-class events, not silent failures.
5. **Composable inheritance.** Org-level base policies can be extended by team and agent-level policies; the validator (F10) enforces that child policies cannot exceed parent permissions.

## Write tests for: **Structured, human-readable summary.** The agent presents a plain-language explanation of what it intends to do, not raw JSON. Approvers must be able to evaluate without reading the agent's internal state.
<!-- task-id: 005-human-approval-gate-for-sensitive-actions-md/structured-human-readable-summary-the-agent-presents-a-plain-language-explanatio/test -->
<!-- adr: 005-human-approval-gate-for-sensitive-actions.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Every action class that the operator marks as sensitive in the Permission Manifest (F2) triggers a synchronous Human Approval Gate. Properties:

1. **Structured, human-readable summary.** The agent presents a plain-language explanation of what it intends to do, not raw JSON. Approvers must be able to evaluate without reading the agent's internal state.
2. **Authenticated approval channels only.** CLI prompt with local OS user attribution, a local web UI bound to localhost with a session token, or a signed API call with an authenticated identity. Never an unauthenticated channel.
3. **Configurable timeout with safe default.** If no approval is received within `N` seconds (default: rejected), the action is automatically rejected. No silent escalation.
4. **Immutable approval record.** Approval and rejection events are written to the Trajectory Ledger (F9) with: approver identity, timestamp, the exact action approved, and the reasoning step (F5) that triggered the request.
5. **No batch approval bypass in v1.** Each action request is a discrete approval event. Bulk-approve modes are deferred to ensure auditability is unambiguous in MVP.

## **Structured, human-readable summary.** The agent presents a plain-language explanation of what it intends to do, not raw JSON. Approvers must be able to evaluate without reading the agent's internal state.
<!-- task-id: 005-human-approval-gate-for-sensitive-actions-md/structured-human-readable-summary-the-agent-presents-a-plain-language-explanatio/production -->
<!-- adr: 005-human-approval-gate-for-sensitive-actions.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Every action class that the operator marks as sensitive in the Permission Manifest (F2) triggers a synchronous Human Approval Gate. Properties:

1. **Structured, human-readable summary.** The agent presents a plain-language explanation of what it intends to do, not raw JSON. Approvers must be able to evaluate without reading the agent's internal state.
2. **Authenticated approval channels only.** CLI prompt with local OS user attribution, a local web UI bound to localhost with a session token, or a signed API call with an authenticated identity. Never an unauthenticated channel.
3. **Configurable timeout with safe default.** If no approval is received within `N` seconds (default: rejected), the action is automatically rejected. No silent escalation.
4. **Immutable approval record.** Approval and rejection events are written to the Trajectory Ledger (F9) with: approver identity, timestamp, the exact action approved, and the reasoning step (F5) that triggered the request.
5. **No batch approval bypass in v1.** Each action request is a discrete approval event. Bulk-approve modes are deferred to ensure auditability is unambiguous in MVP.

## Write tests for: **Authenticated approval channels only.** CLI prompt with local OS user attribution, a local web UI bound to localhost with a session token, or a signed API call with an authenticated identity. Never an unauthenticated channel.
<!-- task-id: 005-human-approval-gate-for-sensitive-actions-md/authenticated-approval-channels-only-cli-prompt-with-local-os-user-attribution-a/test -->
<!-- adr: 005-human-approval-gate-for-sensitive-actions.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Every action class that the operator marks as sensitive in the Permission Manifest (F2) triggers a synchronous Human Approval Gate. Properties:

1. **Structured, human-readable summary.** The agent presents a plain-language explanation of what it intends to do, not raw JSON. Approvers must be able to evaluate without reading the agent's internal state.
2. **Authenticated approval channels only.** CLI prompt with local OS user attribution, a local web UI bound to localhost with a session token, or a signed API call with an authenticated identity. Never an unauthenticated channel.
3. **Configurable timeout with safe default.** If no approval is received within `N` seconds (default: rejected), the action is automatically rejected. No silent escalation.
4. **Immutable approval record.** Approval and rejection events are written to the Trajectory Ledger (F9) with: approver identity, timestamp, the exact action approved, and the reasoning step (F5) that triggered the request.
5. **No batch approval bypass in v1.** Each action request is a discrete approval event. Bulk-approve modes are deferred to ensure auditability is unambiguous in MVP.

## **Authenticated approval channels only.** CLI prompt with local OS user attribution, a local web UI bound to localhost with a session token, or a signed API call with an authenticated identity. Never an unauthenticated channel.
<!-- task-id: 005-human-approval-gate-for-sensitive-actions-md/authenticated-approval-channels-only-cli-prompt-with-local-os-user-attribution-a/production -->
<!-- adr: 005-human-approval-gate-for-sensitive-actions.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Every action class that the operator marks as sensitive in the Permission Manifest (F2) triggers a synchronous Human Approval Gate. Properties:

1. **Structured, human-readable summary.** The agent presents a plain-language explanation of what it intends to do, not raw JSON. Approvers must be able to evaluate without reading the agent's internal state.
2. **Authenticated approval channels only.** CLI prompt with local OS user attribution, a local web UI bound to localhost with a session token, or a signed API call with an authenticated identity. Never an unauthenticated channel.
3. **Configurable timeout with safe default.** If no approval is received within `N` seconds (default: rejected), the action is automatically rejected. No silent escalation.
4. **Immutable approval record.** Approval and rejection events are written to the Trajectory Ledger (F9) with: approver identity, timestamp, the exact action approved, and the reasoning step (F5) that triggered the request.
5. **No batch approval bypass in v1.** Each action request is a discrete approval event. Bulk-approve modes are deferred to ensure auditability is unambiguous in MVP.

## Write tests for: **Configurable timeout with safe default.** If no approval is received within `N` seconds (default: rejected), the action is automatically rejected. No silent escalation.
<!-- task-id: 005-human-approval-gate-for-sensitive-actions-md/configurable-timeout-with-safe-default-if-no-approval-is-received-within-n-secon/test -->
<!-- adr: 005-human-approval-gate-for-sensitive-actions.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Every action class that the operator marks as sensitive in the Permission Manifest (F2) triggers a synchronous Human Approval Gate. Properties:

1. **Structured, human-readable summary.** The agent presents a plain-language explanation of what it intends to do, not raw JSON. Approvers must be able to evaluate without reading the agent's internal state.
2. **Authenticated approval channels only.** CLI prompt with local OS user attribution, a local web UI bound to localhost with a session token, or a signed API call with an authenticated identity. Never an unauthenticated channel.
3. **Configurable timeout with safe default.** If no approval is received within `N` seconds (default: rejected), the action is automatically rejected. No silent escalation.
4. **Immutable approval record.** Approval and rejection events are written to the Trajectory Ledger (F9) with: approver identity, timestamp, the exact action approved, and the reasoning step (F5) that triggered the request.
5. **No batch approval bypass in v1.** Each action request is a discrete approval event. Bulk-approve modes are deferred to ensure auditability is unambiguous in MVP.

## **Configurable timeout with safe default.** If no approval is received within `N` seconds (default: rejected), the action is automatically rejected. No silent escalation.
<!-- task-id: 005-human-approval-gate-for-sensitive-actions-md/configurable-timeout-with-safe-default-if-no-approval-is-received-within-n-secon/production -->
<!-- adr: 005-human-approval-gate-for-sensitive-actions.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Every action class that the operator marks as sensitive in the Permission Manifest (F2) triggers a synchronous Human Approval Gate. Properties:

1. **Structured, human-readable summary.** The agent presents a plain-language explanation of what it intends to do, not raw JSON. Approvers must be able to evaluate without reading the agent's internal state.
2. **Authenticated approval channels only.** CLI prompt with local OS user attribution, a local web UI bound to localhost with a session token, or a signed API call with an authenticated identity. Never an unauthenticated channel.
3. **Configurable timeout with safe default.** If no approval is received within `N` seconds (default: rejected), the action is automatically rejected. No silent escalation.
4. **Immutable approval record.** Approval and rejection events are written to the Trajectory Ledger (F9) with: approver identity, timestamp, the exact action approved, and the reasoning step (F5) that triggered the request.
5. **No batch approval bypass in v1.** Each action request is a discrete approval event. Bulk-approve modes are deferred to ensure auditability is unambiguous in MVP.

## Write tests for: **Immutable approval record.** Approval and rejection events are written to the Trajectory Ledger (F9) with: approver identity, timestamp, the exact action approved, and the reasoning step (F5) that triggered the request.
<!-- task-id: 005-human-approval-gate-for-sensitive-actions-md/immutable-approval-record-approval-and-rejection-events-are-written-to-the-traje/test -->
<!-- adr: 005-human-approval-gate-for-sensitive-actions.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Every action class that the operator marks as sensitive in the Permission Manifest (F2) triggers a synchronous Human Approval Gate. Properties:

1. **Structured, human-readable summary.** The agent presents a plain-language explanation of what it intends to do, not raw JSON. Approvers must be able to evaluate without reading the agent's internal state.
2. **Authenticated approval channels only.** CLI prompt with local OS user attribution, a local web UI bound to localhost with a session token, or a signed API call with an authenticated identity. Never an unauthenticated channel.
3. **Configurable timeout with safe default.** If no approval is received within `N` seconds (default: rejected), the action is automatically rejected. No silent escalation.
4. **Immutable approval record.** Approval and rejection events are written to the Trajectory Ledger (F9) with: approver identity, timestamp, the exact action approved, and the reasoning step (F5) that triggered the request.
5. **No batch approval bypass in v1.** Each action request is a discrete approval event. Bulk-approve modes are deferred to ensure auditability is unambiguous in MVP.

## **Immutable approval record.** Approval and rejection events are written to the Trajectory Ledger (F9) with: approver identity, timestamp, the exact action approved, and the reasoning step (F5) that triggered the request.
<!-- task-id: 005-human-approval-gate-for-sensitive-actions-md/immutable-approval-record-approval-and-rejection-events-are-written-to-the-traje/production -->
<!-- adr: 005-human-approval-gate-for-sensitive-actions.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Every action class that the operator marks as sensitive in the Permission Manifest (F2) triggers a synchronous Human Approval Gate. Properties:

1. **Structured, human-readable summary.** The agent presents a plain-language explanation of what it intends to do, not raw JSON. Approvers must be able to evaluate without reading the agent's internal state.
2. **Authenticated approval channels only.** CLI prompt with local OS user attribution, a local web UI bound to localhost with a session token, or a signed API call with an authenticated identity. Never an unauthenticated channel.
3. **Configurable timeout with safe default.** If no approval is received within `N` seconds (default: rejected), the action is automatically rejected. No silent escalation.
4. **Immutable approval record.** Approval and rejection events are written to the Trajectory Ledger (F9) with: approver identity, timestamp, the exact action approved, and the reasoning step (F5) that triggered the request.
5. **No batch approval bypass in v1.** Each action request is a discrete approval event. Bulk-approve modes are deferred to ensure auditability is unambiguous in MVP.

## Write tests for: **No batch approval bypass in v1.** Each action request is a discrete approval event. Bulk-approve modes are deferred to ensure auditability is unambiguous in MVP.
<!-- task-id: 005-human-approval-gate-for-sensitive-actions-md/no-batch-approval-bypass-in-v1-each-action-request-is-a-discrete-approval-event-/test -->
<!-- adr: 005-human-approval-gate-for-sensitive-actions.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Every action class that the operator marks as sensitive in the Permission Manifest (F2) triggers a synchronous Human Approval Gate. Properties:

1. **Structured, human-readable summary.** The agent presents a plain-language explanation of what it intends to do, not raw JSON. Approvers must be able to evaluate without reading the agent's internal state.
2. **Authenticated approval channels only.** CLI prompt with local OS user attribution, a local web UI bound to localhost with a session token, or a signed API call with an authenticated identity. Never an unauthenticated channel.
3. **Configurable timeout with safe default.** If no approval is received within `N` seconds (default: rejected), the action is automatically rejected. No silent escalation.
4. **Immutable approval record.** Approval and rejection events are written to the Trajectory Ledger (F9) with: approver identity, timestamp, the exact action approved, and the reasoning step (F5) that triggered the request.
5. **No batch approval bypass in v1.** Each action request is a discrete approval event. Bulk-approve modes are deferred to ensure auditability is unambiguous in MVP.

## **No batch approval bypass in v1.** Each action request is a discrete approval event. Bulk-approve modes are deferred to ensure auditability is unambiguous in MVP.
<!-- task-id: 005-human-approval-gate-for-sensitive-actions-md/no-batch-approval-bypass-in-v1-each-action-request-is-a-discrete-approval-event-/production -->
<!-- adr: 005-human-approval-gate-for-sensitive-actions.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Every action class that the operator marks as sensitive in the Permission Manifest (F2) triggers a synchronous Human Approval Gate. Properties:

1. **Structured, human-readable summary.** The agent presents a plain-language explanation of what it intends to do, not raw JSON. Approvers must be able to evaluate without reading the agent's internal state.
2. **Authenticated approval channels only.** CLI prompt with local OS user attribution, a local web UI bound to localhost with a session token, or a signed API call with an authenticated identity. Never an unauthenticated channel.
3. **Configurable timeout with safe default.** If no approval is received within `N` seconds (default: rejected), the action is automatically rejected. No silent escalation.
4. **Immutable approval record.** Approval and rejection events are written to the Trajectory Ledger (F9) with: approver identity, timestamp, the exact action approved, and the reasoning step (F5) that triggered the request.
5. **No batch approval bypass in v1.** Each action request is a discrete approval event. Bulk-approve modes are deferred to ensure auditability is unambiguous in MVP.

## Write tests for: **Mandatory fields** in every entry: agent identity (F1), resource URI, access type, bytes accessed, nanosecond-precision timestamp, session ID, and the F5 reasoning-step ID that triggered the access.
<!-- task-id: 006-structured-access-log-jsonld-siem-format-md/mandatory-fields-in-every-entry-agent-identity-f1-resource-uri-access-type-bytes/test -->
<!-- adr: 006-structured-access-log-jsonld-siem-format.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Every file read, file write, API call, database query, and resource access produces a structured Access Log entry. Properties:

1. **Mandatory fields** in every entry: agent identity (F1), resource URI, access type, bytes accessed, nanosecond-precision timestamp, session ID, and the F5 reasoning-step ID that triggered the access.
2. **JSON-LD format** for export — semantically annotated for SIEM ingestion and for cross-organization correlation when multiple agents interact.
3. **Atomic writes.** A partial log entry is a critical violation; the runtime must guarantee either a complete entry or none.
4. **Cross-referenced.** Access log entries and reasoning trajectory entries (F5) are stored separately but linked by session ID and reasoning-step ID — so an auditor can answer "what data did this reasoning step touch?" in O(1).
5. **Stored inside the F9 hash chain.** Access entries are themselves ledger entries — they participate in the tamper-evident chain and cannot be selectively edited.

## **Mandatory fields** in every entry: agent identity (F1), resource URI, access type, bytes accessed, nanosecond-precision timestamp, session ID, and the F5 reasoning-step ID that triggered the access.
<!-- task-id: 006-structured-access-log-jsonld-siem-format-md/mandatory-fields-in-every-entry-agent-identity-f1-resource-uri-access-type-bytes/production -->
<!-- adr: 006-structured-access-log-jsonld-siem-format.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Every file read, file write, API call, database query, and resource access produces a structured Access Log entry. Properties:

1. **Mandatory fields** in every entry: agent identity (F1), resource URI, access type, bytes accessed, nanosecond-precision timestamp, session ID, and the F5 reasoning-step ID that triggered the access.
2. **JSON-LD format** for export — semantically annotated for SIEM ingestion and for cross-organization correlation when multiple agents interact.
3. **Atomic writes.** A partial log entry is a critical violation; the runtime must guarantee either a complete entry or none.
4. **Cross-referenced.** Access log entries and reasoning trajectory entries (F5) are stored separately but linked by session ID and reasoning-step ID — so an auditor can answer "what data did this reasoning step touch?" in O(1).
5. **Stored inside the F9 hash chain.** Access entries are themselves ledger entries — they participate in the tamper-evident chain and cannot be selectively edited.

## Write tests for: **JSON-LD format** for export — semantically annotated for SIEM ingestion and for cross-organization correlation when multiple agents interact.
<!-- task-id: 006-structured-access-log-jsonld-siem-format-md/json-ld-format-for-export-semantically-annotated-for-siem-ingestion-and-for-cros/test -->
<!-- adr: 006-structured-access-log-jsonld-siem-format.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Every file read, file write, API call, database query, and resource access produces a structured Access Log entry. Properties:

1. **Mandatory fields** in every entry: agent identity (F1), resource URI, access type, bytes accessed, nanosecond-precision timestamp, session ID, and the F5 reasoning-step ID that triggered the access.
2. **JSON-LD format** for export — semantically annotated for SIEM ingestion and for cross-organization correlation when multiple agents interact.
3. **Atomic writes.** A partial log entry is a critical violation; the runtime must guarantee either a complete entry or none.
4. **Cross-referenced.** Access log entries and reasoning trajectory entries (F5) are stored separately but linked by session ID and reasoning-step ID — so an auditor can answer "what data did this reasoning step touch?" in O(1).
5. **Stored inside the F9 hash chain.** Access entries are themselves ledger entries — they participate in the tamper-evident chain and cannot be selectively edited.

## **JSON-LD format** for export — semantically annotated for SIEM ingestion and for cross-organization correlation when multiple agents interact.
<!-- task-id: 006-structured-access-log-jsonld-siem-format-md/json-ld-format-for-export-semantically-annotated-for-siem-ingestion-and-for-cros/production -->
<!-- adr: 006-structured-access-log-jsonld-siem-format.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Every file read, file write, API call, database query, and resource access produces a structured Access Log entry. Properties:

1. **Mandatory fields** in every entry: agent identity (F1), resource URI, access type, bytes accessed, nanosecond-precision timestamp, session ID, and the F5 reasoning-step ID that triggered the access.
2. **JSON-LD format** for export — semantically annotated for SIEM ingestion and for cross-organization correlation when multiple agents interact.
3. **Atomic writes.** A partial log entry is a critical violation; the runtime must guarantee either a complete entry or none.
4. **Cross-referenced.** Access log entries and reasoning trajectory entries (F5) are stored separately but linked by session ID and reasoning-step ID — so an auditor can answer "what data did this reasoning step touch?" in O(1).
5. **Stored inside the F9 hash chain.** Access entries are themselves ledger entries — they participate in the tamper-evident chain and cannot be selectively edited.

## Write tests for: **Atomic writes.** A partial log entry is a critical violation; the runtime must guarantee either a complete entry or none.
<!-- task-id: 006-structured-access-log-jsonld-siem-format-md/atomic-writes-a-partial-log-entry-is-a-critical-violation-the-runtime-must-guara/test -->
<!-- adr: 006-structured-access-log-jsonld-siem-format.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Every file read, file write, API call, database query, and resource access produces a structured Access Log entry. Properties:

1. **Mandatory fields** in every entry: agent identity (F1), resource URI, access type, bytes accessed, nanosecond-precision timestamp, session ID, and the F5 reasoning-step ID that triggered the access.
2. **JSON-LD format** for export — semantically annotated for SIEM ingestion and for cross-organization correlation when multiple agents interact.
3. **Atomic writes.** A partial log entry is a critical violation; the runtime must guarantee either a complete entry or none.
4. **Cross-referenced.** Access log entries and reasoning trajectory entries (F5) are stored separately but linked by session ID and reasoning-step ID — so an auditor can answer "what data did this reasoning step touch?" in O(1).
5. **Stored inside the F9 hash chain.** Access entries are themselves ledger entries — they participate in the tamper-evident chain and cannot be selectively edited.

## **Atomic writes.** A partial log entry is a critical violation; the runtime must guarantee either a complete entry or none.
<!-- task-id: 006-structured-access-log-jsonld-siem-format-md/atomic-writes-a-partial-log-entry-is-a-critical-violation-the-runtime-must-guara/production -->
<!-- adr: 006-structured-access-log-jsonld-siem-format.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Every file read, file write, API call, database query, and resource access produces a structured Access Log entry. Properties:

1. **Mandatory fields** in every entry: agent identity (F1), resource URI, access type, bytes accessed, nanosecond-precision timestamp, session ID, and the F5 reasoning-step ID that triggered the access.
2. **JSON-LD format** for export — semantically annotated for SIEM ingestion and for cross-organization correlation when multiple agents interact.
3. **Atomic writes.** A partial log entry is a critical violation; the runtime must guarantee either a complete entry or none.
4. **Cross-referenced.** Access log entries and reasoning trajectory entries (F5) are stored separately but linked by session ID and reasoning-step ID — so an auditor can answer "what data did this reasoning step touch?" in O(1).
5. **Stored inside the F9 hash chain.** Access entries are themselves ledger entries — they participate in the tamper-evident chain and cannot be selectively edited.

## Write tests for: **Cross-referenced.** Access log entries and reasoning trajectory entries (F5) are stored separately but linked by session ID and reasoning-step ID — so an auditor can answer "what data did this reasoning step touch?" in O(1).
<!-- task-id: 006-structured-access-log-jsonld-siem-format-md/cross-referenced-access-log-entries-and-reasoning-trajectory-entries-f5-are-stor/test -->
<!-- adr: 006-structured-access-log-jsonld-siem-format.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Every file read, file write, API call, database query, and resource access produces a structured Access Log entry. Properties:

1. **Mandatory fields** in every entry: agent identity (F1), resource URI, access type, bytes accessed, nanosecond-precision timestamp, session ID, and the F5 reasoning-step ID that triggered the access.
2. **JSON-LD format** for export — semantically annotated for SIEM ingestion and for cross-organization correlation when multiple agents interact.
3. **Atomic writes.** A partial log entry is a critical violation; the runtime must guarantee either a complete entry or none.
4. **Cross-referenced.** Access log entries and reasoning trajectory entries (F5) are stored separately but linked by session ID and reasoning-step ID — so an auditor can answer "what data did this reasoning step touch?" in O(1).
5. **Stored inside the F9 hash chain.** Access entries are themselves ledger entries — they participate in the tamper-evident chain and cannot be selectively edited.

## **Cross-referenced.** Access log entries and reasoning trajectory entries (F5) are stored separately but linked by session ID and reasoning-step ID — so an auditor can answer "what data did this reasoning step touch?" in O(1).
<!-- task-id: 006-structured-access-log-jsonld-siem-format-md/cross-referenced-access-log-entries-and-reasoning-trajectory-entries-f5-are-stor/production -->
<!-- adr: 006-structured-access-log-jsonld-siem-format.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Every file read, file write, API call, database query, and resource access produces a structured Access Log entry. Properties:

1. **Mandatory fields** in every entry: agent identity (F1), resource URI, access type, bytes accessed, nanosecond-precision timestamp, session ID, and the F5 reasoning-step ID that triggered the access.
2. **JSON-LD format** for export — semantically annotated for SIEM ingestion and for cross-organization correlation when multiple agents interact.
3. **Atomic writes.** A partial log entry is a critical violation; the runtime must guarantee either a complete entry or none.
4. **Cross-referenced.** Access log entries and reasoning trajectory entries (F5) are stored separately but linked by session ID and reasoning-step ID — so an auditor can answer "what data did this reasoning step touch?" in O(1).
5. **Stored inside the F9 hash chain.** Access entries are themselves ledger entries — they participate in the tamper-evident chain and cannot be selectively edited.

## Write tests for: **Stored inside the F9 hash chain.** Access entries are themselves ledger entries — they participate in the tamper-evident chain and cannot be selectively edited.
<!-- task-id: 006-structured-access-log-jsonld-siem-format-md/stored-inside-the-f9-hash-chain-access-entries-are-themselves-ledger-entries-the/test -->
<!-- adr: 006-structured-access-log-jsonld-siem-format.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Every file read, file write, API call, database query, and resource access produces a structured Access Log entry. Properties:

1. **Mandatory fields** in every entry: agent identity (F1), resource URI, access type, bytes accessed, nanosecond-precision timestamp, session ID, and the F5 reasoning-step ID that triggered the access.
2. **JSON-LD format** for export — semantically annotated for SIEM ingestion and for cross-organization correlation when multiple agents interact.
3. **Atomic writes.** A partial log entry is a critical violation; the runtime must guarantee either a complete entry or none.
4. **Cross-referenced.** Access log entries and reasoning trajectory entries (F5) are stored separately but linked by session ID and reasoning-step ID — so an auditor can answer "what data did this reasoning step touch?" in O(1).
5. **Stored inside the F9 hash chain.** Access entries are themselves ledger entries — they participate in the tamper-evident chain and cannot be selectively edited.

## **Stored inside the F9 hash chain.** Access entries are themselves ledger entries — they participate in the tamper-evident chain and cannot be selectively edited.
<!-- task-id: 006-structured-access-log-jsonld-siem-format-md/stored-inside-the-f9-hash-chain-access-entries-are-themselves-ledger-entries-the/production -->
<!-- adr: 006-structured-access-log-jsonld-siem-format.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Every file read, file write, API call, database query, and resource access produces a structured Access Log entry. Properties:

1. **Mandatory fields** in every entry: agent identity (F1), resource URI, access type, bytes accessed, nanosecond-precision timestamp, session ID, and the F5 reasoning-step ID that triggered the access.
2. **JSON-LD format** for export — semantically annotated for SIEM ingestion and for cross-organization correlation when multiple agents interact.
3. **Atomic writes.** A partial log entry is a critical violation; the runtime must guarantee either a complete entry or none.
4. **Cross-referenced.** Access log entries and reasoning trajectory entries (F5) are stored separately but linked by session ID and reasoning-step ID — so an auditor can answer "what data did this reasoning step touch?" in O(1).
5. **Stored inside the F9 hash chain.** Access entries are themselves ledger entries — they participate in the tamper-evident chain and cannot be selectively edited.

## Write tests for: **Pre-execution write.** The trajectory entry is committed to the ledger (F9) *before* the action executes. A crash during action execution still leaves a complete record of intent.
<!-- task-id: 007-pre-execution-reasoning-trajectory-md/pre-execution-write-the-trajectory-entry-is-committed-to-the-ledger-f9-before-th/test -->
<!-- adr: 007-pre-execution-reasoning-trajectory.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Every action the agent takes is preceded by a Trajectory Ledger entry recording the reasoning that led to it. Properties:

1. **Pre-execution write.** The trajectory entry is committed to the ledger (F9) *before* the action executes. A crash during action execution still leaves a complete record of intent.
2. **Structured content.** Each entry records: triggering input, the reasoning chain (in structured natural language), tools considered, tool selected, and the action to execute.
3. **Linked to access entries.** Each trajectory entry has a reasoning-step ID; access log entries (F4) reference it via the same ID.
4. **Replay-capable.** The format is machine-parseable enough for the F8 replay viewer to reconstruct a synchronized timeline of reasoning + action without external services.
5. **No reasoning recorded after action completion.** Post-execution outcomes are recorded in separate result entries, not appended to the reasoning entry — the reasoning record is immutable evidence of the agent's pre-action state.

## **Pre-execution write.** The trajectory entry is committed to the ledger (F9) *before* the action executes. A crash during action execution still leaves a complete record of intent.
<!-- task-id: 007-pre-execution-reasoning-trajectory-md/pre-execution-write-the-trajectory-entry-is-committed-to-the-ledger-f9-before-th/production -->
<!-- adr: 007-pre-execution-reasoning-trajectory.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Every action the agent takes is preceded by a Trajectory Ledger entry recording the reasoning that led to it. Properties:

1. **Pre-execution write.** The trajectory entry is committed to the ledger (F9) *before* the action executes. A crash during action execution still leaves a complete record of intent.
2. **Structured content.** Each entry records: triggering input, the reasoning chain (in structured natural language), tools considered, tool selected, and the action to execute.
3. **Linked to access entries.** Each trajectory entry has a reasoning-step ID; access log entries (F4) reference it via the same ID.
4. **Replay-capable.** The format is machine-parseable enough for the F8 replay viewer to reconstruct a synchronized timeline of reasoning + action without external services.
5. **No reasoning recorded after action completion.** Post-execution outcomes are recorded in separate result entries, not appended to the reasoning entry — the reasoning record is immutable evidence of the agent's pre-action state.

## Write tests for: **Structured content.** Each entry records: triggering input, the reasoning chain (in structured natural language), tools considered, tool selected, and the action to execute.
<!-- task-id: 007-pre-execution-reasoning-trajectory-md/structured-content-each-entry-records-triggering-input-the-reasoning-chain-in-st/test -->
<!-- adr: 007-pre-execution-reasoning-trajectory.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Every action the agent takes is preceded by a Trajectory Ledger entry recording the reasoning that led to it. Properties:

1. **Pre-execution write.** The trajectory entry is committed to the ledger (F9) *before* the action executes. A crash during action execution still leaves a complete record of intent.
2. **Structured content.** Each entry records: triggering input, the reasoning chain (in structured natural language), tools considered, tool selected, and the action to execute.
3. **Linked to access entries.** Each trajectory entry has a reasoning-step ID; access log entries (F4) reference it via the same ID.
4. **Replay-capable.** The format is machine-parseable enough for the F8 replay viewer to reconstruct a synchronized timeline of reasoning + action without external services.
5. **No reasoning recorded after action completion.** Post-execution outcomes are recorded in separate result entries, not appended to the reasoning entry — the reasoning record is immutable evidence of the agent's pre-action state.

## **Structured content.** Each entry records: triggering input, the reasoning chain (in structured natural language), tools considered, tool selected, and the action to execute.
<!-- task-id: 007-pre-execution-reasoning-trajectory-md/structured-content-each-entry-records-triggering-input-the-reasoning-chain-in-st/production -->
<!-- adr: 007-pre-execution-reasoning-trajectory.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Every action the agent takes is preceded by a Trajectory Ledger entry recording the reasoning that led to it. Properties:

1. **Pre-execution write.** The trajectory entry is committed to the ledger (F9) *before* the action executes. A crash during action execution still leaves a complete record of intent.
2. **Structured content.** Each entry records: triggering input, the reasoning chain (in structured natural language), tools considered, tool selected, and the action to execute.
3. **Linked to access entries.** Each trajectory entry has a reasoning-step ID; access log entries (F4) reference it via the same ID.
4. **Replay-capable.** The format is machine-parseable enough for the F8 replay viewer to reconstruct a synchronized timeline of reasoning + action without external services.
5. **No reasoning recorded after action completion.** Post-execution outcomes are recorded in separate result entries, not appended to the reasoning entry — the reasoning record is immutable evidence of the agent's pre-action state.

## Write tests for: **Linked to access entries.** Each trajectory entry has a reasoning-step ID; access log entries (F4) reference it via the same ID.
<!-- task-id: 007-pre-execution-reasoning-trajectory-md/linked-to-access-entries-each-trajectory-entry-has-a-reasoning-step-id-access-lo/test -->
<!-- adr: 007-pre-execution-reasoning-trajectory.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Every action the agent takes is preceded by a Trajectory Ledger entry recording the reasoning that led to it. Properties:

1. **Pre-execution write.** The trajectory entry is committed to the ledger (F9) *before* the action executes. A crash during action execution still leaves a complete record of intent.
2. **Structured content.** Each entry records: triggering input, the reasoning chain (in structured natural language), tools considered, tool selected, and the action to execute.
3. **Linked to access entries.** Each trajectory entry has a reasoning-step ID; access log entries (F4) reference it via the same ID.
4. **Replay-capable.** The format is machine-parseable enough for the F8 replay viewer to reconstruct a synchronized timeline of reasoning + action without external services.
5. **No reasoning recorded after action completion.** Post-execution outcomes are recorded in separate result entries, not appended to the reasoning entry — the reasoning record is immutable evidence of the agent's pre-action state.

## **Linked to access entries.** Each trajectory entry has a reasoning-step ID; access log entries (F4) reference it via the same ID.
<!-- task-id: 007-pre-execution-reasoning-trajectory-md/linked-to-access-entries-each-trajectory-entry-has-a-reasoning-step-id-access-lo/production -->
<!-- adr: 007-pre-execution-reasoning-trajectory.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Every action the agent takes is preceded by a Trajectory Ledger entry recording the reasoning that led to it. Properties:

1. **Pre-execution write.** The trajectory entry is committed to the ledger (F9) *before* the action executes. A crash during action execution still leaves a complete record of intent.
2. **Structured content.** Each entry records: triggering input, the reasoning chain (in structured natural language), tools considered, tool selected, and the action to execute.
3. **Linked to access entries.** Each trajectory entry has a reasoning-step ID; access log entries (F4) reference it via the same ID.
4. **Replay-capable.** The format is machine-parseable enough for the F8 replay viewer to reconstruct a synchronized timeline of reasoning + action without external services.
5. **No reasoning recorded after action completion.** Post-execution outcomes are recorded in separate result entries, not appended to the reasoning entry — the reasoning record is immutable evidence of the agent's pre-action state.

## Write tests for: **Replay-capable.** The format is machine-parseable enough for the F8 replay viewer to reconstruct a synchronized timeline of reasoning + action without external services.
<!-- task-id: 007-pre-execution-reasoning-trajectory-md/replay-capable-the-format-is-machine-parseable-enough-for-the-f8-replay-viewer-t/test -->
<!-- adr: 007-pre-execution-reasoning-trajectory.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Every action the agent takes is preceded by a Trajectory Ledger entry recording the reasoning that led to it. Properties:

1. **Pre-execution write.** The trajectory entry is committed to the ledger (F9) *before* the action executes. A crash during action execution still leaves a complete record of intent.
2. **Structured content.** Each entry records: triggering input, the reasoning chain (in structured natural language), tools considered, tool selected, and the action to execute.
3. **Linked to access entries.** Each trajectory entry has a reasoning-step ID; access log entries (F4) reference it via the same ID.
4. **Replay-capable.** The format is machine-parseable enough for the F8 replay viewer to reconstruct a synchronized timeline of reasoning + action without external services.
5. **No reasoning recorded after action completion.** Post-execution outcomes are recorded in separate result entries, not appended to the reasoning entry — the reasoning record is immutable evidence of the agent's pre-action state.

## **Replay-capable.** The format is machine-parseable enough for the F8 replay viewer to reconstruct a synchronized timeline of reasoning + action without external services.
<!-- task-id: 007-pre-execution-reasoning-trajectory-md/replay-capable-the-format-is-machine-parseable-enough-for-the-f8-replay-viewer-t/production -->
<!-- adr: 007-pre-execution-reasoning-trajectory.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Every action the agent takes is preceded by a Trajectory Ledger entry recording the reasoning that led to it. Properties:

1. **Pre-execution write.** The trajectory entry is committed to the ledger (F9) *before* the action executes. A crash during action execution still leaves a complete record of intent.
2. **Structured content.** Each entry records: triggering input, the reasoning chain (in structured natural language), tools considered, tool selected, and the action to execute.
3. **Linked to access entries.** Each trajectory entry has a reasoning-step ID; access log entries (F4) reference it via the same ID.
4. **Replay-capable.** The format is machine-parseable enough for the F8 replay viewer to reconstruct a synchronized timeline of reasoning + action without external services.
5. **No reasoning recorded after action completion.** Post-execution outcomes are recorded in separate result entries, not appended to the reasoning entry — the reasoning record is immutable evidence of the agent's pre-action state.

## Write tests for: **No reasoning recorded after action completion.** Post-execution outcomes are recorded in separate result entries, not appended to the reasoning entry — the reasoning record is immutable evidence of the agent's pre-action state.
<!-- task-id: 007-pre-execution-reasoning-trajectory-md/no-reasoning-recorded-after-action-completion-post-execution-outcomes-are-record/test -->
<!-- adr: 007-pre-execution-reasoning-trajectory.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Every action the agent takes is preceded by a Trajectory Ledger entry recording the reasoning that led to it. Properties:

1. **Pre-execution write.** The trajectory entry is committed to the ledger (F9) *before* the action executes. A crash during action execution still leaves a complete record of intent.
2. **Structured content.** Each entry records: triggering input, the reasoning chain (in structured natural language), tools considered, tool selected, and the action to execute.
3. **Linked to access entries.** Each trajectory entry has a reasoning-step ID; access log entries (F4) reference it via the same ID.
4. **Replay-capable.** The format is machine-parseable enough for the F8 replay viewer to reconstruct a synchronized timeline of reasoning + action without external services.
5. **No reasoning recorded after action completion.** Post-execution outcomes are recorded in separate result entries, not appended to the reasoning entry — the reasoning record is immutable evidence of the agent's pre-action state.

## **No reasoning recorded after action completion.** Post-execution outcomes are recorded in separate result entries, not appended to the reasoning entry — the reasoning record is immutable evidence of the agent's pre-action state.
<!-- task-id: 007-pre-execution-reasoning-trajectory-md/no-reasoning-recorded-after-action-completion-post-execution-outcomes-are-record/production -->
<!-- adr: 007-pre-execution-reasoning-trajectory.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Every action the agent takes is preceded by a Trajectory Ledger entry recording the reasoning that led to it. Properties:

1. **Pre-execution write.** The trajectory entry is committed to the ledger (F9) *before* the action executes. A crash during action execution still leaves a complete record of intent.
2. **Structured content.** Each entry records: triggering input, the reasoning chain (in structured natural language), tools considered, tool selected, and the action to execute.
3. **Linked to access entries.** Each trajectory entry has a reasoning-step ID; access log entries (F4) reference it via the same ID.
4. **Replay-capable.** The format is machine-parseable enough for the F8 replay viewer to reconstruct a synchronized timeline of reasoning + action without external services.
5. **No reasoning recorded after action completion.** Post-execution outcomes are recorded in separate result entries, not appended to the reasoning entry — the reasoning record is immutable evidence of the agent's pre-action state.

## Write tests for: **Default = deny, both directions.** New deployments cannot make outbound or inbound network connections without explicit grants in the Permission Manifest (F2).
<!-- task-id: 008-network-deny-by-default-at-runtime-level-md/default-deny-both-directions-new-deployments-cannot-make-outbound-or-inbound-net/test -->
<!-- adr: 008-network-deny-by-default-at-runtime-level.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Network access is denied by default and enforced at the Rust runtime level. Properties:

1. **Default = deny, both directions.** New deployments cannot make outbound or inbound network connections without explicit grants in the Permission Manifest (F2).
2. **Enforced inside the runtime, not by external firewall.** The agent process refuses to open sockets for non-allowlisted destinations regardless of host firewall state. This survives misconfigured hosts and containers.
3. **Critical violation on any deny-mode connection attempt.** Such attempts are written to the Trajectory Ledger (F9) as critical events and the agent is halted.
4. **Verifiable attestation.** At session end the runtime emits a signed attestation that the session made zero network connections (or only the connections the manifest allowed). The attestation is suitable for direct inclusion in a compliance report.
5. **Layered with platform controls.** The runtime guarantee does not replace cluster-level NetworkPolicies in Phase 2/3 — it stacks beneath them; both must agree.

## **Default = deny, both directions.** New deployments cannot make outbound or inbound network connections without explicit grants in the Permission Manifest (F2).
<!-- task-id: 008-network-deny-by-default-at-runtime-level-md/default-deny-both-directions-new-deployments-cannot-make-outbound-or-inbound-net/production -->
<!-- adr: 008-network-deny-by-default-at-runtime-level.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Network access is denied by default and enforced at the Rust runtime level. Properties:

1. **Default = deny, both directions.** New deployments cannot make outbound or inbound network connections without explicit grants in the Permission Manifest (F2).
2. **Enforced inside the runtime, not by external firewall.** The agent process refuses to open sockets for non-allowlisted destinations regardless of host firewall state. This survives misconfigured hosts and containers.
3. **Critical violation on any deny-mode connection attempt.** Such attempts are written to the Trajectory Ledger (F9) as critical events and the agent is halted.
4. **Verifiable attestation.** At session end the runtime emits a signed attestation that the session made zero network connections (or only the connections the manifest allowed). The attestation is suitable for direct inclusion in a compliance report.
5. **Layered with platform controls.** The runtime guarantee does not replace cluster-level NetworkPolicies in Phase 2/3 — it stacks beneath them; both must agree.

## Write tests for: **Enforced inside the runtime, not by external firewall.** The agent process refuses to open sockets for non-allowlisted destinations regardless of host firewall state. This survives misconfigured hosts and containers.
<!-- task-id: 008-network-deny-by-default-at-runtime-level-md/enforced-inside-the-runtime-not-by-external-firewall-the-agent-process-refuses-t/test -->
<!-- adr: 008-network-deny-by-default-at-runtime-level.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Network access is denied by default and enforced at the Rust runtime level. Properties:

1. **Default = deny, both directions.** New deployments cannot make outbound or inbound network connections without explicit grants in the Permission Manifest (F2).
2. **Enforced inside the runtime, not by external firewall.** The agent process refuses to open sockets for non-allowlisted destinations regardless of host firewall state. This survives misconfigured hosts and containers.
3. **Critical violation on any deny-mode connection attempt.** Such attempts are written to the Trajectory Ledger (F9) as critical events and the agent is halted.
4. **Verifiable attestation.** At session end the runtime emits a signed attestation that the session made zero network connections (or only the connections the manifest allowed). The attestation is suitable for direct inclusion in a compliance report.
5. **Layered with platform controls.** The runtime guarantee does not replace cluster-level NetworkPolicies in Phase 2/3 — it stacks beneath them; both must agree.

## **Enforced inside the runtime, not by external firewall.** The agent process refuses to open sockets for non-allowlisted destinations regardless of host firewall state. This survives misconfigured hosts and containers.
<!-- task-id: 008-network-deny-by-default-at-runtime-level-md/enforced-inside-the-runtime-not-by-external-firewall-the-agent-process-refuses-t/production -->
<!-- adr: 008-network-deny-by-default-at-runtime-level.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Network access is denied by default and enforced at the Rust runtime level. Properties:

1. **Default = deny, both directions.** New deployments cannot make outbound or inbound network connections without explicit grants in the Permission Manifest (F2).
2. **Enforced inside the runtime, not by external firewall.** The agent process refuses to open sockets for non-allowlisted destinations regardless of host firewall state. This survives misconfigured hosts and containers.
3. **Critical violation on any deny-mode connection attempt.** Such attempts are written to the Trajectory Ledger (F9) as critical events and the agent is halted.
4. **Verifiable attestation.** At session end the runtime emits a signed attestation that the session made zero network connections (or only the connections the manifest allowed). The attestation is suitable for direct inclusion in a compliance report.
5. **Layered with platform controls.** The runtime guarantee does not replace cluster-level NetworkPolicies in Phase 2/3 — it stacks beneath them; both must agree.

## Write tests for: **Critical violation on any deny-mode connection attempt.** Such attempts are written to the Trajectory Ledger (F9) as critical events and the agent is halted.
<!-- task-id: 008-network-deny-by-default-at-runtime-level-md/critical-violation-on-any-deny-mode-connection-attempt-such-attempts-are-written/test -->
<!-- adr: 008-network-deny-by-default-at-runtime-level.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Network access is denied by default and enforced at the Rust runtime level. Properties:

1. **Default = deny, both directions.** New deployments cannot make outbound or inbound network connections without explicit grants in the Permission Manifest (F2).
2. **Enforced inside the runtime, not by external firewall.** The agent process refuses to open sockets for non-allowlisted destinations regardless of host firewall state. This survives misconfigured hosts and containers.
3. **Critical violation on any deny-mode connection attempt.** Such attempts are written to the Trajectory Ledger (F9) as critical events and the agent is halted.
4. **Verifiable attestation.** At session end the runtime emits a signed attestation that the session made zero network connections (or only the connections the manifest allowed). The attestation is suitable for direct inclusion in a compliance report.
5. **Layered with platform controls.** The runtime guarantee does not replace cluster-level NetworkPolicies in Phase 2/3 — it stacks beneath them; both must agree.

## **Critical violation on any deny-mode connection attempt.** Such attempts are written to the Trajectory Ledger (F9) as critical events and the agent is halted.
<!-- task-id: 008-network-deny-by-default-at-runtime-level-md/critical-violation-on-any-deny-mode-connection-attempt-such-attempts-are-written/production -->
<!-- adr: 008-network-deny-by-default-at-runtime-level.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Network access is denied by default and enforced at the Rust runtime level. Properties:

1. **Default = deny, both directions.** New deployments cannot make outbound or inbound network connections without explicit grants in the Permission Manifest (F2).
2. **Enforced inside the runtime, not by external firewall.** The agent process refuses to open sockets for non-allowlisted destinations regardless of host firewall state. This survives misconfigured hosts and containers.
3. **Critical violation on any deny-mode connection attempt.** Such attempts are written to the Trajectory Ledger (F9) as critical events and the agent is halted.
4. **Verifiable attestation.** At session end the runtime emits a signed attestation that the session made zero network connections (or only the connections the manifest allowed). The attestation is suitable for direct inclusion in a compliance report.
5. **Layered with platform controls.** The runtime guarantee does not replace cluster-level NetworkPolicies in Phase 2/3 — it stacks beneath them; both must agree.

## Write tests for: **Verifiable attestation.** At session end the runtime emits a signed attestation that the session made zero network connections (or only the connections the manifest allowed). The attestation is suitable for direct inclusion in a compliance report.
<!-- task-id: 008-network-deny-by-default-at-runtime-level-md/verifiable-attestation-at-session-end-the-runtime-emits-a-signed-attestation-tha/test -->
<!-- adr: 008-network-deny-by-default-at-runtime-level.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Network access is denied by default and enforced at the Rust runtime level. Properties:

1. **Default = deny, both directions.** New deployments cannot make outbound or inbound network connections without explicit grants in the Permission Manifest (F2).
2. **Enforced inside the runtime, not by external firewall.** The agent process refuses to open sockets for non-allowlisted destinations regardless of host firewall state. This survives misconfigured hosts and containers.
3. **Critical violation on any deny-mode connection attempt.** Such attempts are written to the Trajectory Ledger (F9) as critical events and the agent is halted.
4. **Verifiable attestation.** At session end the runtime emits a signed attestation that the session made zero network connections (or only the connections the manifest allowed). The attestation is suitable for direct inclusion in a compliance report.
5. **Layered with platform controls.** The runtime guarantee does not replace cluster-level NetworkPolicies in Phase 2/3 — it stacks beneath them; both must agree.

## **Verifiable attestation.** At session end the runtime emits a signed attestation that the session made zero network connections (or only the connections the manifest allowed). The attestation is suitable for direct inclusion in a compliance report.
<!-- task-id: 008-network-deny-by-default-at-runtime-level-md/verifiable-attestation-at-session-end-the-runtime-emits-a-signed-attestation-tha/production -->
<!-- adr: 008-network-deny-by-default-at-runtime-level.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Network access is denied by default and enforced at the Rust runtime level. Properties:

1. **Default = deny, both directions.** New deployments cannot make outbound or inbound network connections without explicit grants in the Permission Manifest (F2).
2. **Enforced inside the runtime, not by external firewall.** The agent process refuses to open sockets for non-allowlisted destinations regardless of host firewall state. This survives misconfigured hosts and containers.
3. **Critical violation on any deny-mode connection attempt.** Such attempts are written to the Trajectory Ledger (F9) as critical events and the agent is halted.
4. **Verifiable attestation.** At session end the runtime emits a signed attestation that the session made zero network connections (or only the connections the manifest allowed). The attestation is suitable for direct inclusion in a compliance report.
5. **Layered with platform controls.** The runtime guarantee does not replace cluster-level NetworkPolicies in Phase 2/3 — it stacks beneath them; both must agree.

## Write tests for: **Layered with platform controls.** The runtime guarantee does not replace cluster-level NetworkPolicies in Phase 2/3 — it stacks beneath them; both must agree.
<!-- task-id: 008-network-deny-by-default-at-runtime-level-md/layered-with-platform-controls-the-runtime-guarantee-does-not-replace-cluster-le/test -->
<!-- adr: 008-network-deny-by-default-at-runtime-level.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Network access is denied by default and enforced at the Rust runtime level. Properties:

1. **Default = deny, both directions.** New deployments cannot make outbound or inbound network connections without explicit grants in the Permission Manifest (F2).
2. **Enforced inside the runtime, not by external firewall.** The agent process refuses to open sockets for non-allowlisted destinations regardless of host firewall state. This survives misconfigured hosts and containers.
3. **Critical violation on any deny-mode connection attempt.** Such attempts are written to the Trajectory Ledger (F9) as critical events and the agent is halted.
4. **Verifiable attestation.** At session end the runtime emits a signed attestation that the session made zero network connections (or only the connections the manifest allowed). The attestation is suitable for direct inclusion in a compliance report.
5. **Layered with platform controls.** The runtime guarantee does not replace cluster-level NetworkPolicies in Phase 2/3 — it stacks beneath them; both must agree.

## **Layered with platform controls.** The runtime guarantee does not replace cluster-level NetworkPolicies in Phase 2/3 — it stacks beneath them; both must agree.
<!-- task-id: 008-network-deny-by-default-at-runtime-level-md/layered-with-platform-controls-the-runtime-guarantee-does-not-replace-cluster-le/production -->
<!-- adr: 008-network-deny-by-default-at-runtime-level.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Network access is denied by default and enforced at the Rust runtime level. Properties:

1. **Default = deny, both directions.** New deployments cannot make outbound or inbound network connections without explicit grants in the Permission Manifest (F2).
2. **Enforced inside the runtime, not by external firewall.** The agent process refuses to open sockets for non-allowlisted destinations regardless of host firewall state. This survives misconfigured hosts and containers.
3. **Critical violation on any deny-mode connection attempt.** Such attempts are written to the Trajectory Ledger (F9) as critical events and the agent is halted.
4. **Verifiable attestation.** At session end the runtime emits a signed attestation that the session made zero network connections (or only the connections the manifest allowed). The attestation is suitable for direct inclusion in a compliance report.
5. **Layered with platform controls.** The runtime guarantee does not replace cluster-level NetworkPolicies in Phase 2/3 — it stacks beneath them; both must agree.

## Write tests for: **Default = read-only** for every resource type (filesystem, database, API, message broker).
<!-- task-id: 009-read-only-default-with-explicit-write-grants-md/default-read-only-for-every-resource-type-filesystem-database-api-message-broker/test -->
<!-- adr: 009-read-only-default-with-explicit-write-grants.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Aegis-Node defaults all resource access to read-only. Write access is granted only via explicit, narrow, optionally time-bounded entries in the Permission Manifest (F2). Properties:

1. **Default = read-only** for every resource type (filesystem, database, API, message broker).
2. **Explicit `write_grants` block** in the manifest. Each entry specifies resource path/scope and the action (`write`, `delete`, `update`, etc.).
3. **No implicit grants.** A write attempt outside `write_grants` is a critical violation logged to F9, even if the resource is in `read` scope.
4. **Time-bounded grants supported.** Grants may include a duration or expiration timestamp; expired grants are inert and trigger violations.
5. **Composable with F3.** Write grants can require Human Approval Gate (F3) per-action via the manifest's `approval_required_for` field.

## **Default = read-only** for every resource type (filesystem, database, API, message broker).
<!-- task-id: 009-read-only-default-with-explicit-write-grants-md/default-read-only-for-every-resource-type-filesystem-database-api-message-broker/production -->
<!-- adr: 009-read-only-default-with-explicit-write-grants.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Aegis-Node defaults all resource access to read-only. Write access is granted only via explicit, narrow, optionally time-bounded entries in the Permission Manifest (F2). Properties:

1. **Default = read-only** for every resource type (filesystem, database, API, message broker).
2. **Explicit `write_grants` block** in the manifest. Each entry specifies resource path/scope and the action (`write`, `delete`, `update`, etc.).
3. **No implicit grants.** A write attempt outside `write_grants` is a critical violation logged to F9, even if the resource is in `read` scope.
4. **Time-bounded grants supported.** Grants may include a duration or expiration timestamp; expired grants are inert and trigger violations.
5. **Composable with F3.** Write grants can require Human Approval Gate (F3) per-action via the manifest's `approval_required_for` field.

## Write tests for: **Explicit `write_grants` block** in the manifest. Each entry specifies resource path/scope and the action (`write`, `delete`, `update`, etc.).
<!-- task-id: 009-read-only-default-with-explicit-write-grants-md/explicit-write-grants-block-in-the-manifest-each-entry-specifies-resource-path-s/test -->
<!-- adr: 009-read-only-default-with-explicit-write-grants.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Aegis-Node defaults all resource access to read-only. Write access is granted only via explicit, narrow, optionally time-bounded entries in the Permission Manifest (F2). Properties:

1. **Default = read-only** for every resource type (filesystem, database, API, message broker).
2. **Explicit `write_grants` block** in the manifest. Each entry specifies resource path/scope and the action (`write`, `delete`, `update`, etc.).
3. **No implicit grants.** A write attempt outside `write_grants` is a critical violation logged to F9, even if the resource is in `read` scope.
4. **Time-bounded grants supported.** Grants may include a duration or expiration timestamp; expired grants are inert and trigger violations.
5. **Composable with F3.** Write grants can require Human Approval Gate (F3) per-action via the manifest's `approval_required_for` field.

## **Explicit `write_grants` block** in the manifest. Each entry specifies resource path/scope and the action (`write`, `delete`, `update`, etc.).
<!-- task-id: 009-read-only-default-with-explicit-write-grants-md/explicit-write-grants-block-in-the-manifest-each-entry-specifies-resource-path-s/production -->
<!-- adr: 009-read-only-default-with-explicit-write-grants.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Aegis-Node defaults all resource access to read-only. Write access is granted only via explicit, narrow, optionally time-bounded entries in the Permission Manifest (F2). Properties:

1. **Default = read-only** for every resource type (filesystem, database, API, message broker).
2. **Explicit `write_grants` block** in the manifest. Each entry specifies resource path/scope and the action (`write`, `delete`, `update`, etc.).
3. **No implicit grants.** A write attempt outside `write_grants` is a critical violation logged to F9, even if the resource is in `read` scope.
4. **Time-bounded grants supported.** Grants may include a duration or expiration timestamp; expired grants are inert and trigger violations.
5. **Composable with F3.** Write grants can require Human Approval Gate (F3) per-action via the manifest's `approval_required_for` field.

## Write tests for: **No implicit grants.** A write attempt outside `write_grants` is a critical violation logged to F9, even if the resource is in `read` scope.
<!-- task-id: 009-read-only-default-with-explicit-write-grants-md/no-implicit-grants-a-write-attempt-outside-write-grants-is-a-critical-violation-/test -->
<!-- adr: 009-read-only-default-with-explicit-write-grants.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Aegis-Node defaults all resource access to read-only. Write access is granted only via explicit, narrow, optionally time-bounded entries in the Permission Manifest (F2). Properties:

1. **Default = read-only** for every resource type (filesystem, database, API, message broker).
2. **Explicit `write_grants` block** in the manifest. Each entry specifies resource path/scope and the action (`write`, `delete`, `update`, etc.).
3. **No implicit grants.** A write attempt outside `write_grants` is a critical violation logged to F9, even if the resource is in `read` scope.
4. **Time-bounded grants supported.** Grants may include a duration or expiration timestamp; expired grants are inert and trigger violations.
5. **Composable with F3.** Write grants can require Human Approval Gate (F3) per-action via the manifest's `approval_required_for` field.

## **No implicit grants.** A write attempt outside `write_grants` is a critical violation logged to F9, even if the resource is in `read` scope.
<!-- task-id: 009-read-only-default-with-explicit-write-grants-md/no-implicit-grants-a-write-attempt-outside-write-grants-is-a-critical-violation-/production -->
<!-- adr: 009-read-only-default-with-explicit-write-grants.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Aegis-Node defaults all resource access to read-only. Write access is granted only via explicit, narrow, optionally time-bounded entries in the Permission Manifest (F2). Properties:

1. **Default = read-only** for every resource type (filesystem, database, API, message broker).
2. **Explicit `write_grants` block** in the manifest. Each entry specifies resource path/scope and the action (`write`, `delete`, `update`, etc.).
3. **No implicit grants.** A write attempt outside `write_grants` is a critical violation logged to F9, even if the resource is in `read` scope.
4. **Time-bounded grants supported.** Grants may include a duration or expiration timestamp; expired grants are inert and trigger violations.
5. **Composable with F3.** Write grants can require Human Approval Gate (F3) per-action via the manifest's `approval_required_for` field.

## Write tests for: **Time-bounded grants supported.** Grants may include a duration or expiration timestamp; expired grants are inert and trigger violations.
<!-- task-id: 009-read-only-default-with-explicit-write-grants-md/time-bounded-grants-supported-grants-may-include-a-duration-or-expiration-timest/test -->
<!-- adr: 009-read-only-default-with-explicit-write-grants.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Aegis-Node defaults all resource access to read-only. Write access is granted only via explicit, narrow, optionally time-bounded entries in the Permission Manifest (F2). Properties:

1. **Default = read-only** for every resource type (filesystem, database, API, message broker).
2. **Explicit `write_grants` block** in the manifest. Each entry specifies resource path/scope and the action (`write`, `delete`, `update`, etc.).
3. **No implicit grants.** A write attempt outside `write_grants` is a critical violation logged to F9, even if the resource is in `read` scope.
4. **Time-bounded grants supported.** Grants may include a duration or expiration timestamp; expired grants are inert and trigger violations.
5. **Composable with F3.** Write grants can require Human Approval Gate (F3) per-action via the manifest's `approval_required_for` field.

## **Time-bounded grants supported.** Grants may include a duration or expiration timestamp; expired grants are inert and trigger violations.
<!-- task-id: 009-read-only-default-with-explicit-write-grants-md/time-bounded-grants-supported-grants-may-include-a-duration-or-expiration-timest/production -->
<!-- adr: 009-read-only-default-with-explicit-write-grants.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Aegis-Node defaults all resource access to read-only. Write access is granted only via explicit, narrow, optionally time-bounded entries in the Permission Manifest (F2). Properties:

1. **Default = read-only** for every resource type (filesystem, database, API, message broker).
2. **Explicit `write_grants` block** in the manifest. Each entry specifies resource path/scope and the action (`write`, `delete`, `update`, etc.).
3. **No implicit grants.** A write attempt outside `write_grants` is a critical violation logged to F9, even if the resource is in `read` scope.
4. **Time-bounded grants supported.** Grants may include a duration or expiration timestamp; expired grants are inert and trigger violations.
5. **Composable with F3.** Write grants can require Human Approval Gate (F3) per-action via the manifest's `approval_required_for` field.

## Write tests for: **Composable with F3.** Write grants can require Human Approval Gate (F3) per-action via the manifest's `approval_required_for` field.
<!-- task-id: 009-read-only-default-with-explicit-write-grants-md/composable-with-f3-write-grants-can-require-human-approval-gate-f3-per-action-vi/test -->
<!-- adr: 009-read-only-default-with-explicit-write-grants.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Aegis-Node defaults all resource access to read-only. Write access is granted only via explicit, narrow, optionally time-bounded entries in the Permission Manifest (F2). Properties:

1. **Default = read-only** for every resource type (filesystem, database, API, message broker).
2. **Explicit `write_grants` block** in the manifest. Each entry specifies resource path/scope and the action (`write`, `delete`, `update`, etc.).
3. **No implicit grants.** A write attempt outside `write_grants` is a critical violation logged to F9, even if the resource is in `read` scope.
4. **Time-bounded grants supported.** Grants may include a duration or expiration timestamp; expired grants are inert and trigger violations.
5. **Composable with F3.** Write grants can require Human Approval Gate (F3) per-action via the manifest's `approval_required_for` field.

## **Composable with F3.** Write grants can require Human Approval Gate (F3) per-action via the manifest's `approval_required_for` field.
<!-- task-id: 009-read-only-default-with-explicit-write-grants-md/composable-with-f3-write-grants-can-require-human-approval-gate-f3-per-action-vi/production -->
<!-- adr: 009-read-only-default-with-explicit-write-grants.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Aegis-Node defaults all resource access to read-only. Write access is granted only via explicit, narrow, optionally time-bounded entries in the Permission Manifest (F2). Properties:

1. **Default = read-only** for every resource type (filesystem, database, API, message broker).
2. **Explicit `write_grants` block** in the manifest. Each entry specifies resource path/scope and the action (`write`, `delete`, `update`, etc.).
3. **No implicit grants.** A write attempt outside `write_grants` is a critical violation logged to F9, even if the resource is in `read` scope.
4. **Time-bounded grants supported.** Grants may include a duration or expiration timestamp; expired grants are inert and trigger violations.
5. **Composable with F3.** Write grants can require Human Approval Gate (F3) per-action via the manifest's `approval_required_for` field.

## Write tests for: **Self-contained replay viewer.** A single static HTML file (CSS + JS embedded) that loads the local ledger file and renders a synchronized timeline of reasoning trajectory (F5) and access log (F4). No backend, no network calls, no CDN dependencies.
<!-- task-id: 010-deterministic-trajectory-replay-offline-viewer-md/self-contained-replay-viewer-a-single-static-html-file-css-js-embedded-that-load/test -->
<!-- adr: 010-deterministic-trajectory-replay-offline-viewer.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Aegis-Node ships a deterministic Trajectory Replay capability built around a single-file HTML viewer. Properties:

1. **Self-contained replay viewer.** A single static HTML file (CSS + JS embedded) that loads the local ledger file and renders a synchronized timeline of reasoning trajectory (F5) and access log (F4). No backend, no network calls, no CDN dependencies.
2. **Deterministic.** Replaying the same ledger always produces the same rendered output. No random ordering, no timezone-dependent rendering, no live data fetches.
3. **Self-sufficient ledger format.** The ledger contains all data necessary to reconstruct the session — no external lookups required. This shapes the F9 ledger schema.
4. **Air-gap shippable.** Auditors can be given a USB stick with `replay.html` + `ledger.jsonld` and reconstruct the session offline.
5. **Read-only viewer.** The viewer cannot mutate the ledger; integrity verification (F9 chain check) runs at load.

## **Self-contained replay viewer.** A single static HTML file (CSS + JS embedded) that loads the local ledger file and renders a synchronized timeline of reasoning trajectory (F5) and access log (F4). No backend, no network calls, no CDN dependencies.
<!-- task-id: 010-deterministic-trajectory-replay-offline-viewer-md/self-contained-replay-viewer-a-single-static-html-file-css-js-embedded-that-load/production -->
<!-- adr: 010-deterministic-trajectory-replay-offline-viewer.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Aegis-Node ships a deterministic Trajectory Replay capability built around a single-file HTML viewer. Properties:

1. **Self-contained replay viewer.** A single static HTML file (CSS + JS embedded) that loads the local ledger file and renders a synchronized timeline of reasoning trajectory (F5) and access log (F4). No backend, no network calls, no CDN dependencies.
2. **Deterministic.** Replaying the same ledger always produces the same rendered output. No random ordering, no timezone-dependent rendering, no live data fetches.
3. **Self-sufficient ledger format.** The ledger contains all data necessary to reconstruct the session — no external lookups required. This shapes the F9 ledger schema.
4. **Air-gap shippable.** Auditors can be given a USB stick with `replay.html` + `ledger.jsonld` and reconstruct the session offline.
5. **Read-only viewer.** The viewer cannot mutate the ledger; integrity verification (F9 chain check) runs at load.

## Write tests for: **Deterministic.** Replaying the same ledger always produces the same rendered output. No random ordering, no timezone-dependent rendering, no live data fetches.
<!-- task-id: 010-deterministic-trajectory-replay-offline-viewer-md/deterministic-replaying-the-same-ledger-always-produces-the-same-rendered-output/test -->
<!-- adr: 010-deterministic-trajectory-replay-offline-viewer.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Aegis-Node ships a deterministic Trajectory Replay capability built around a single-file HTML viewer. Properties:

1. **Self-contained replay viewer.** A single static HTML file (CSS + JS embedded) that loads the local ledger file and renders a synchronized timeline of reasoning trajectory (F5) and access log (F4). No backend, no network calls, no CDN dependencies.
2. **Deterministic.** Replaying the same ledger always produces the same rendered output. No random ordering, no timezone-dependent rendering, no live data fetches.
3. **Self-sufficient ledger format.** The ledger contains all data necessary to reconstruct the session — no external lookups required. This shapes the F9 ledger schema.
4. **Air-gap shippable.** Auditors can be given a USB stick with `replay.html` + `ledger.jsonld` and reconstruct the session offline.
5. **Read-only viewer.** The viewer cannot mutate the ledger; integrity verification (F9 chain check) runs at load.

## **Deterministic.** Replaying the same ledger always produces the same rendered output. No random ordering, no timezone-dependent rendering, no live data fetches.
<!-- task-id: 010-deterministic-trajectory-replay-offline-viewer-md/deterministic-replaying-the-same-ledger-always-produces-the-same-rendered-output/production -->
<!-- adr: 010-deterministic-trajectory-replay-offline-viewer.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Aegis-Node ships a deterministic Trajectory Replay capability built around a single-file HTML viewer. Properties:

1. **Self-contained replay viewer.** A single static HTML file (CSS + JS embedded) that loads the local ledger file and renders a synchronized timeline of reasoning trajectory (F5) and access log (F4). No backend, no network calls, no CDN dependencies.
2. **Deterministic.** Replaying the same ledger always produces the same rendered output. No random ordering, no timezone-dependent rendering, no live data fetches.
3. **Self-sufficient ledger format.** The ledger contains all data necessary to reconstruct the session — no external lookups required. This shapes the F9 ledger schema.
4. **Air-gap shippable.** Auditors can be given a USB stick with `replay.html` + `ledger.jsonld` and reconstruct the session offline.
5. **Read-only viewer.** The viewer cannot mutate the ledger; integrity verification (F9 chain check) runs at load.

## Write tests for: **Self-sufficient ledger format.** The ledger contains all data necessary to reconstruct the session — no external lookups required. This shapes the F9 ledger schema.
<!-- task-id: 010-deterministic-trajectory-replay-offline-viewer-md/self-sufficient-ledger-format-the-ledger-contains-all-data-necessary-to-reconstr/test -->
<!-- adr: 010-deterministic-trajectory-replay-offline-viewer.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Aegis-Node ships a deterministic Trajectory Replay capability built around a single-file HTML viewer. Properties:

1. **Self-contained replay viewer.** A single static HTML file (CSS + JS embedded) that loads the local ledger file and renders a synchronized timeline of reasoning trajectory (F5) and access log (F4). No backend, no network calls, no CDN dependencies.
2. **Deterministic.** Replaying the same ledger always produces the same rendered output. No random ordering, no timezone-dependent rendering, no live data fetches.
3. **Self-sufficient ledger format.** The ledger contains all data necessary to reconstruct the session — no external lookups required. This shapes the F9 ledger schema.
4. **Air-gap shippable.** Auditors can be given a USB stick with `replay.html` + `ledger.jsonld` and reconstruct the session offline.
5. **Read-only viewer.** The viewer cannot mutate the ledger; integrity verification (F9 chain check) runs at load.

## **Self-sufficient ledger format.** The ledger contains all data necessary to reconstruct the session — no external lookups required. This shapes the F9 ledger schema.
<!-- task-id: 010-deterministic-trajectory-replay-offline-viewer-md/self-sufficient-ledger-format-the-ledger-contains-all-data-necessary-to-reconstr/production -->
<!-- adr: 010-deterministic-trajectory-replay-offline-viewer.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Aegis-Node ships a deterministic Trajectory Replay capability built around a single-file HTML viewer. Properties:

1. **Self-contained replay viewer.** A single static HTML file (CSS + JS embedded) that loads the local ledger file and renders a synchronized timeline of reasoning trajectory (F5) and access log (F4). No backend, no network calls, no CDN dependencies.
2. **Deterministic.** Replaying the same ledger always produces the same rendered output. No random ordering, no timezone-dependent rendering, no live data fetches.
3. **Self-sufficient ledger format.** The ledger contains all data necessary to reconstruct the session — no external lookups required. This shapes the F9 ledger schema.
4. **Air-gap shippable.** Auditors can be given a USB stick with `replay.html` + `ledger.jsonld` and reconstruct the session offline.
5. **Read-only viewer.** The viewer cannot mutate the ledger; integrity verification (F9 chain check) runs at load.

## Write tests for: **Air-gap shippable.** Auditors can be given a USB stick with `replay.html` + `ledger.jsonld` and reconstruct the session offline.
<!-- task-id: 010-deterministic-trajectory-replay-offline-viewer-md/air-gap-shippable-auditors-can-be-given-a-usb-stick-with-replay-html-ledger-json/test -->
<!-- adr: 010-deterministic-trajectory-replay-offline-viewer.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Aegis-Node ships a deterministic Trajectory Replay capability built around a single-file HTML viewer. Properties:

1. **Self-contained replay viewer.** A single static HTML file (CSS + JS embedded) that loads the local ledger file and renders a synchronized timeline of reasoning trajectory (F5) and access log (F4). No backend, no network calls, no CDN dependencies.
2. **Deterministic.** Replaying the same ledger always produces the same rendered output. No random ordering, no timezone-dependent rendering, no live data fetches.
3. **Self-sufficient ledger format.** The ledger contains all data necessary to reconstruct the session — no external lookups required. This shapes the F9 ledger schema.
4. **Air-gap shippable.** Auditors can be given a USB stick with `replay.html` + `ledger.jsonld` and reconstruct the session offline.
5. **Read-only viewer.** The viewer cannot mutate the ledger; integrity verification (F9 chain check) runs at load.

## **Air-gap shippable.** Auditors can be given a USB stick with `replay.html` + `ledger.jsonld` and reconstruct the session offline.
<!-- task-id: 010-deterministic-trajectory-replay-offline-viewer-md/air-gap-shippable-auditors-can-be-given-a-usb-stick-with-replay-html-ledger-json/production -->
<!-- adr: 010-deterministic-trajectory-replay-offline-viewer.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Aegis-Node ships a deterministic Trajectory Replay capability built around a single-file HTML viewer. Properties:

1. **Self-contained replay viewer.** A single static HTML file (CSS + JS embedded) that loads the local ledger file and renders a synchronized timeline of reasoning trajectory (F5) and access log (F4). No backend, no network calls, no CDN dependencies.
2. **Deterministic.** Replaying the same ledger always produces the same rendered output. No random ordering, no timezone-dependent rendering, no live data fetches.
3. **Self-sufficient ledger format.** The ledger contains all data necessary to reconstruct the session — no external lookups required. This shapes the F9 ledger schema.
4. **Air-gap shippable.** Auditors can be given a USB stick with `replay.html` + `ledger.jsonld` and reconstruct the session offline.
5. **Read-only viewer.** The viewer cannot mutate the ledger; integrity verification (F9 chain check) runs at load.

## Write tests for: **Read-only viewer.** The viewer cannot mutate the ledger; integrity verification (F9 chain check) runs at load.
<!-- task-id: 010-deterministic-trajectory-replay-offline-viewer-md/read-only-viewer-the-viewer-cannot-mutate-the-ledger-integrity-verification-f9-c/test -->
<!-- adr: 010-deterministic-trajectory-replay-offline-viewer.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Aegis-Node ships a deterministic Trajectory Replay capability built around a single-file HTML viewer. Properties:

1. **Self-contained replay viewer.** A single static HTML file (CSS + JS embedded) that loads the local ledger file and renders a synchronized timeline of reasoning trajectory (F5) and access log (F4). No backend, no network calls, no CDN dependencies.
2. **Deterministic.** Replaying the same ledger always produces the same rendered output. No random ordering, no timezone-dependent rendering, no live data fetches.
3. **Self-sufficient ledger format.** The ledger contains all data necessary to reconstruct the session — no external lookups required. This shapes the F9 ledger schema.
4. **Air-gap shippable.** Auditors can be given a USB stick with `replay.html` + `ledger.jsonld` and reconstruct the session offline.
5. **Read-only viewer.** The viewer cannot mutate the ledger; integrity verification (F9 chain check) runs at load.

## **Read-only viewer.** The viewer cannot mutate the ledger; integrity verification (F9 chain check) runs at load.
<!-- task-id: 010-deterministic-trajectory-replay-offline-viewer-md/read-only-viewer-the-viewer-cannot-mutate-the-ledger-integrity-verification-f9-c/production -->
<!-- adr: 010-deterministic-trajectory-replay-offline-viewer.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Aegis-Node ships a deterministic Trajectory Replay capability built around a single-file HTML viewer. Properties:

1. **Self-contained replay viewer.** A single static HTML file (CSS + JS embedded) that loads the local ledger file and renders a synchronized timeline of reasoning trajectory (F5) and access log (F4). No backend, no network calls, no CDN dependencies.
2. **Deterministic.** Replaying the same ledger always produces the same rendered output. No random ordering, no timezone-dependent rendering, no live data fetches.
3. **Self-sufficient ledger format.** The ledger contains all data necessary to reconstruct the session — no external lookups required. This shapes the F9 ledger schema.
4. **Air-gap shippable.** Auditors can be given a USB stick with `replay.html` + `ledger.jsonld` and reconstruct the session offline.
5. **Read-only viewer.** The viewer cannot mutate the ledger; integrity verification (F9 chain check) runs at load.

## Write tests for: **Each entry contains:** content payload, timestamp, agent identity hash (F1), and the SHA-256 hash of the previous entry. The genesis entry's "previous" is a session-bound zero value.
<!-- task-id: 011-hash-chained-tamper-evident-ledger-md/each-entry-contains-content-payload-timestamp-agent-identity-hash-f1-and-the-sha/test -->
<!-- adr: 011-hash-chained-tamper-evident-ledger.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

The Trajectory Ledger is a hash-chained, append-only data structure, not a log file. Properties:

1. **Each entry contains:** content payload, timestamp, agent identity hash (F1), and the SHA-256 hash of the previous entry. The genesis entry's "previous" is a session-bound zero value.
2. **Append-only at the API level.** No delete or update operations are exposed. The runtime refuses to bind to a ledger file with non-append-only filesystem permissions where the OS supports it.
3. **`aegis verify` CLI command.** Walks the chain, verifies every hash link, and reports the position of any break. Exit code is non-zero on integrity failure for CI/CD use.
4. **Notarization-ready.** The ledger root hash (the latest entry's hash) can be exported and registered with an external trusted timestamp authority (RFC 3161 TSA) for legal admissibility. Local-only deployments use a built-in TSA-equivalent for development; production deployments integrate with the operator's TSA.
5. **JSON-LD format.** Compatible with W3C Verifiable Credentials, enabling integration with enterprise identity systems and downstream cryptographic verification.

## **Each entry contains:** content payload, timestamp, agent identity hash (F1), and the SHA-256 hash of the previous entry. The genesis entry's "previous" is a session-bound zero value.
<!-- task-id: 011-hash-chained-tamper-evident-ledger-md/each-entry-contains-content-payload-timestamp-agent-identity-hash-f1-and-the-sha/production -->
<!-- adr: 011-hash-chained-tamper-evident-ledger.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

The Trajectory Ledger is a hash-chained, append-only data structure, not a log file. Properties:

1. **Each entry contains:** content payload, timestamp, agent identity hash (F1), and the SHA-256 hash of the previous entry. The genesis entry's "previous" is a session-bound zero value.
2. **Append-only at the API level.** No delete or update operations are exposed. The runtime refuses to bind to a ledger file with non-append-only filesystem permissions where the OS supports it.
3. **`aegis verify` CLI command.** Walks the chain, verifies every hash link, and reports the position of any break. Exit code is non-zero on integrity failure for CI/CD use.
4. **Notarization-ready.** The ledger root hash (the latest entry's hash) can be exported and registered with an external trusted timestamp authority (RFC 3161 TSA) for legal admissibility. Local-only deployments use a built-in TSA-equivalent for development; production deployments integrate with the operator's TSA.
5. **JSON-LD format.** Compatible with W3C Verifiable Credentials, enabling integration with enterprise identity systems and downstream cryptographic verification.

## Write tests for: **Append-only at the API level.** No delete or update operations are exposed. The runtime refuses to bind to a ledger file with non-append-only filesystem permissions where the OS supports it.
<!-- task-id: 011-hash-chained-tamper-evident-ledger-md/append-only-at-the-api-level-no-delete-or-update-operations-are-exposed-the-runt/test -->
<!-- adr: 011-hash-chained-tamper-evident-ledger.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

The Trajectory Ledger is a hash-chained, append-only data structure, not a log file. Properties:

1. **Each entry contains:** content payload, timestamp, agent identity hash (F1), and the SHA-256 hash of the previous entry. The genesis entry's "previous" is a session-bound zero value.
2. **Append-only at the API level.** No delete or update operations are exposed. The runtime refuses to bind to a ledger file with non-append-only filesystem permissions where the OS supports it.
3. **`aegis verify` CLI command.** Walks the chain, verifies every hash link, and reports the position of any break. Exit code is non-zero on integrity failure for CI/CD use.
4. **Notarization-ready.** The ledger root hash (the latest entry's hash) can be exported and registered with an external trusted timestamp authority (RFC 3161 TSA) for legal admissibility. Local-only deployments use a built-in TSA-equivalent for development; production deployments integrate with the operator's TSA.
5. **JSON-LD format.** Compatible with W3C Verifiable Credentials, enabling integration with enterprise identity systems and downstream cryptographic verification.

## **Append-only at the API level.** No delete or update operations are exposed. The runtime refuses to bind to a ledger file with non-append-only filesystem permissions where the OS supports it.
<!-- task-id: 011-hash-chained-tamper-evident-ledger-md/append-only-at-the-api-level-no-delete-or-update-operations-are-exposed-the-runt/production -->
<!-- adr: 011-hash-chained-tamper-evident-ledger.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

The Trajectory Ledger is a hash-chained, append-only data structure, not a log file. Properties:

1. **Each entry contains:** content payload, timestamp, agent identity hash (F1), and the SHA-256 hash of the previous entry. The genesis entry's "previous" is a session-bound zero value.
2. **Append-only at the API level.** No delete or update operations are exposed. The runtime refuses to bind to a ledger file with non-append-only filesystem permissions where the OS supports it.
3. **`aegis verify` CLI command.** Walks the chain, verifies every hash link, and reports the position of any break. Exit code is non-zero on integrity failure for CI/CD use.
4. **Notarization-ready.** The ledger root hash (the latest entry's hash) can be exported and registered with an external trusted timestamp authority (RFC 3161 TSA) for legal admissibility. Local-only deployments use a built-in TSA-equivalent for development; production deployments integrate with the operator's TSA.
5. **JSON-LD format.** Compatible with W3C Verifiable Credentials, enabling integration with enterprise identity systems and downstream cryptographic verification.

## Write tests for: **`aegis verify` CLI command.** Walks the chain, verifies every hash link, and reports the position of any break. Exit code is non-zero on integrity failure for CI/CD use.
<!-- task-id: 011-hash-chained-tamper-evident-ledger-md/aegis-verify-cli-command-walks-the-chain-verifies-every-hash-link-and-reports-th/test -->
<!-- adr: 011-hash-chained-tamper-evident-ledger.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

The Trajectory Ledger is a hash-chained, append-only data structure, not a log file. Properties:

1. **Each entry contains:** content payload, timestamp, agent identity hash (F1), and the SHA-256 hash of the previous entry. The genesis entry's "previous" is a session-bound zero value.
2. **Append-only at the API level.** No delete or update operations are exposed. The runtime refuses to bind to a ledger file with non-append-only filesystem permissions where the OS supports it.
3. **`aegis verify` CLI command.** Walks the chain, verifies every hash link, and reports the position of any break. Exit code is non-zero on integrity failure for CI/CD use.
4. **Notarization-ready.** The ledger root hash (the latest entry's hash) can be exported and registered with an external trusted timestamp authority (RFC 3161 TSA) for legal admissibility. Local-only deployments use a built-in TSA-equivalent for development; production deployments integrate with the operator's TSA.
5. **JSON-LD format.** Compatible with W3C Verifiable Credentials, enabling integration with enterprise identity systems and downstream cryptographic verification.

## **`aegis verify` CLI command.** Walks the chain, verifies every hash link, and reports the position of any break. Exit code is non-zero on integrity failure for CI/CD use.
<!-- task-id: 011-hash-chained-tamper-evident-ledger-md/aegis-verify-cli-command-walks-the-chain-verifies-every-hash-link-and-reports-th/production -->
<!-- adr: 011-hash-chained-tamper-evident-ledger.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

The Trajectory Ledger is a hash-chained, append-only data structure, not a log file. Properties:

1. **Each entry contains:** content payload, timestamp, agent identity hash (F1), and the SHA-256 hash of the previous entry. The genesis entry's "previous" is a session-bound zero value.
2. **Append-only at the API level.** No delete or update operations are exposed. The runtime refuses to bind to a ledger file with non-append-only filesystem permissions where the OS supports it.
3. **`aegis verify` CLI command.** Walks the chain, verifies every hash link, and reports the position of any break. Exit code is non-zero on integrity failure for CI/CD use.
4. **Notarization-ready.** The ledger root hash (the latest entry's hash) can be exported and registered with an external trusted timestamp authority (RFC 3161 TSA) for legal admissibility. Local-only deployments use a built-in TSA-equivalent for development; production deployments integrate with the operator's TSA.
5. **JSON-LD format.** Compatible with W3C Verifiable Credentials, enabling integration with enterprise identity systems and downstream cryptographic verification.

## Write tests for: **Notarization-ready.** The ledger root hash (the latest entry's hash) can be exported and registered with an external trusted timestamp authority (RFC 3161 TSA) for legal admissibility. Local-only deployments use a built-in TSA-equivalent for development; production deployments integrate with the operator's TSA.
<!-- task-id: 011-hash-chained-tamper-evident-ledger-md/notarization-ready-the-ledger-root-hash-the-latest-entry-s-hash-can-be-exported-/test -->
<!-- adr: 011-hash-chained-tamper-evident-ledger.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

The Trajectory Ledger is a hash-chained, append-only data structure, not a log file. Properties:

1. **Each entry contains:** content payload, timestamp, agent identity hash (F1), and the SHA-256 hash of the previous entry. The genesis entry's "previous" is a session-bound zero value.
2. **Append-only at the API level.** No delete or update operations are exposed. The runtime refuses to bind to a ledger file with non-append-only filesystem permissions where the OS supports it.
3. **`aegis verify` CLI command.** Walks the chain, verifies every hash link, and reports the position of any break. Exit code is non-zero on integrity failure for CI/CD use.
4. **Notarization-ready.** The ledger root hash (the latest entry's hash) can be exported and registered with an external trusted timestamp authority (RFC 3161 TSA) for legal admissibility. Local-only deployments use a built-in TSA-equivalent for development; production deployments integrate with the operator's TSA.
5. **JSON-LD format.** Compatible with W3C Verifiable Credentials, enabling integration with enterprise identity systems and downstream cryptographic verification.

## **Notarization-ready.** The ledger root hash (the latest entry's hash) can be exported and registered with an external trusted timestamp authority (RFC 3161 TSA) for legal admissibility. Local-only deployments use a built-in TSA-equivalent for development; production deployments integrate with the operator's TSA.
<!-- task-id: 011-hash-chained-tamper-evident-ledger-md/notarization-ready-the-ledger-root-hash-the-latest-entry-s-hash-can-be-exported-/production -->
<!-- adr: 011-hash-chained-tamper-evident-ledger.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

The Trajectory Ledger is a hash-chained, append-only data structure, not a log file. Properties:

1. **Each entry contains:** content payload, timestamp, agent identity hash (F1), and the SHA-256 hash of the previous entry. The genesis entry's "previous" is a session-bound zero value.
2. **Append-only at the API level.** No delete or update operations are exposed. The runtime refuses to bind to a ledger file with non-append-only filesystem permissions where the OS supports it.
3. **`aegis verify` CLI command.** Walks the chain, verifies every hash link, and reports the position of any break. Exit code is non-zero on integrity failure for CI/CD use.
4. **Notarization-ready.** The ledger root hash (the latest entry's hash) can be exported and registered with an external trusted timestamp authority (RFC 3161 TSA) for legal admissibility. Local-only deployments use a built-in TSA-equivalent for development; production deployments integrate with the operator's TSA.
5. **JSON-LD format.** Compatible with W3C Verifiable Credentials, enabling integration with enterprise identity systems and downstream cryptographic verification.

## Write tests for: **JSON-LD format.** Compatible with W3C Verifiable Credentials, enabling integration with enterprise identity systems and downstream cryptographic verification.
<!-- task-id: 011-hash-chained-tamper-evident-ledger-md/json-ld-format-compatible-with-w3c-verifiable-credentials-enabling-integration-w/test -->
<!-- adr: 011-hash-chained-tamper-evident-ledger.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

The Trajectory Ledger is a hash-chained, append-only data structure, not a log file. Properties:

1. **Each entry contains:** content payload, timestamp, agent identity hash (F1), and the SHA-256 hash of the previous entry. The genesis entry's "previous" is a session-bound zero value.
2. **Append-only at the API level.** No delete or update operations are exposed. The runtime refuses to bind to a ledger file with non-append-only filesystem permissions where the OS supports it.
3. **`aegis verify` CLI command.** Walks the chain, verifies every hash link, and reports the position of any break. Exit code is non-zero on integrity failure for CI/CD use.
4. **Notarization-ready.** The ledger root hash (the latest entry's hash) can be exported and registered with an external trusted timestamp authority (RFC 3161 TSA) for legal admissibility. Local-only deployments use a built-in TSA-equivalent for development; production deployments integrate with the operator's TSA.
5. **JSON-LD format.** Compatible with W3C Verifiable Credentials, enabling integration with enterprise identity systems and downstream cryptographic verification.

## **JSON-LD format.** Compatible with W3C Verifiable Credentials, enabling integration with enterprise identity systems and downstream cryptographic verification.
<!-- task-id: 011-hash-chained-tamper-evident-ledger-md/json-ld-format-compatible-with-w3c-verifiable-credentials-enabling-integration-w/production -->
<!-- adr: 011-hash-chained-tamper-evident-ledger.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

The Trajectory Ledger is a hash-chained, append-only data structure, not a log file. Properties:

1. **Each entry contains:** content payload, timestamp, agent identity hash (F1), and the SHA-256 hash of the previous entry. The genesis entry's "previous" is a session-bound zero value.
2. **Append-only at the API level.** No delete or update operations are exposed. The runtime refuses to bind to a ledger file with non-append-only filesystem permissions where the OS supports it.
3. **`aegis verify` CLI command.** Walks the chain, verifies every hash link, and reports the position of any break. Exit code is non-zero on integrity failure for CI/CD use.
4. **Notarization-ready.** The ledger root hash (the latest entry's hash) can be exported and registered with an external trusted timestamp authority (RFC 3161 TSA) for legal admissibility. Local-only deployments use a built-in TSA-equivalent for development; production deployments integrate with the operator's TSA.
5. **JSON-LD format.** Compatible with W3C Verifiable Credentials, enabling integration with enterprise identity systems and downstream cryptographic verification.

## Write tests for: **Schema validation.** The manifest is checked against a versioned JSON Schema; all type, format, and required-field errors are reported with file/line context.
<!-- task-id: 012-policy-as-code-validation-md/schema-validation-the-manifest-is-checked-against-a-versioned-json-schema-all-ty/test -->
<!-- adr: 012-policy-as-code-validation.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Manifests and Behavioral Contracts are validated by a dedicated CLI (`aegis validate`) designed for CI/CD integration. Properties:

1. **Schema validation.** The manifest is checked against a versioned JSON Schema; all type, format, and required-field errors are reported with file/line context.
2. **Policy linting.** A set of rules detects common misconfigurations: overly broad file paths (`/`, `/home`), wildcard tools, missing approval gates on write actions, network grants without justification, etc.
3. **Composition + inheritance.** Org-level base policies can be extended by team-level and agent-level policies. The validator enforces that child policies cannot exceed parent permissions; violations fail the build.
4. **Structured JSON output.** Validation output is structured for CI consumption (GitHub Actions annotations, GitLab CI reports, Jenkins JUnit XML).
5. **Human-readable security summary.** The validator emits a "policy summary" report — a plain-language account of what the agent is permitted to do — suitable for inclusion in a security review package without modification.

## **Schema validation.** The manifest is checked against a versioned JSON Schema; all type, format, and required-field errors are reported with file/line context.
<!-- task-id: 012-policy-as-code-validation-md/schema-validation-the-manifest-is-checked-against-a-versioned-json-schema-all-ty/production -->
<!-- adr: 012-policy-as-code-validation.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Manifests and Behavioral Contracts are validated by a dedicated CLI (`aegis validate`) designed for CI/CD integration. Properties:

1. **Schema validation.** The manifest is checked against a versioned JSON Schema; all type, format, and required-field errors are reported with file/line context.
2. **Policy linting.** A set of rules detects common misconfigurations: overly broad file paths (`/`, `/home`), wildcard tools, missing approval gates on write actions, network grants without justification, etc.
3. **Composition + inheritance.** Org-level base policies can be extended by team-level and agent-level policies. The validator enforces that child policies cannot exceed parent permissions; violations fail the build.
4. **Structured JSON output.** Validation output is structured for CI consumption (GitHub Actions annotations, GitLab CI reports, Jenkins JUnit XML).
5. **Human-readable security summary.** The validator emits a "policy summary" report — a plain-language account of what the agent is permitted to do — suitable for inclusion in a security review package without modification.

## Write tests for: **Policy linting.** A set of rules detects common misconfigurations: overly broad file paths (`/`, `/home`), wildcard tools, missing approval gates on write actions, network grants without justification, etc.
<!-- task-id: 012-policy-as-code-validation-md/policy-linting-a-set-of-rules-detects-common-misconfigurations-overly-broad-file/test -->
<!-- adr: 012-policy-as-code-validation.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Manifests and Behavioral Contracts are validated by a dedicated CLI (`aegis validate`) designed for CI/CD integration. Properties:

1. **Schema validation.** The manifest is checked against a versioned JSON Schema; all type, format, and required-field errors are reported with file/line context.
2. **Policy linting.** A set of rules detects common misconfigurations: overly broad file paths (`/`, `/home`), wildcard tools, missing approval gates on write actions, network grants without justification, etc.
3. **Composition + inheritance.** Org-level base policies can be extended by team-level and agent-level policies. The validator enforces that child policies cannot exceed parent permissions; violations fail the build.
4. **Structured JSON output.** Validation output is structured for CI consumption (GitHub Actions annotations, GitLab CI reports, Jenkins JUnit XML).
5. **Human-readable security summary.** The validator emits a "policy summary" report — a plain-language account of what the agent is permitted to do — suitable for inclusion in a security review package without modification.

## **Policy linting.** A set of rules detects common misconfigurations: overly broad file paths (`/`, `/home`), wildcard tools, missing approval gates on write actions, network grants without justification, etc.
<!-- task-id: 012-policy-as-code-validation-md/policy-linting-a-set-of-rules-detects-common-misconfigurations-overly-broad-file/production -->
<!-- adr: 012-policy-as-code-validation.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Manifests and Behavioral Contracts are validated by a dedicated CLI (`aegis validate`) designed for CI/CD integration. Properties:

1. **Schema validation.** The manifest is checked against a versioned JSON Schema; all type, format, and required-field errors are reported with file/line context.
2. **Policy linting.** A set of rules detects common misconfigurations: overly broad file paths (`/`, `/home`), wildcard tools, missing approval gates on write actions, network grants without justification, etc.
3. **Composition + inheritance.** Org-level base policies can be extended by team-level and agent-level policies. The validator enforces that child policies cannot exceed parent permissions; violations fail the build.
4. **Structured JSON output.** Validation output is structured for CI consumption (GitHub Actions annotations, GitLab CI reports, Jenkins JUnit XML).
5. **Human-readable security summary.** The validator emits a "policy summary" report — a plain-language account of what the agent is permitted to do — suitable for inclusion in a security review package without modification.

## Write tests for: **Composition + inheritance.** Org-level base policies can be extended by team-level and agent-level policies. The validator enforces that child policies cannot exceed parent permissions; violations fail the build.
<!-- task-id: 012-policy-as-code-validation-md/composition-inheritance-org-level-base-policies-can-be-extended-by-team-level-an/test -->
<!-- adr: 012-policy-as-code-validation.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Manifests and Behavioral Contracts are validated by a dedicated CLI (`aegis validate`) designed for CI/CD integration. Properties:

1. **Schema validation.** The manifest is checked against a versioned JSON Schema; all type, format, and required-field errors are reported with file/line context.
2. **Policy linting.** A set of rules detects common misconfigurations: overly broad file paths (`/`, `/home`), wildcard tools, missing approval gates on write actions, network grants without justification, etc.
3. **Composition + inheritance.** Org-level base policies can be extended by team-level and agent-level policies. The validator enforces that child policies cannot exceed parent permissions; violations fail the build.
4. **Structured JSON output.** Validation output is structured for CI consumption (GitHub Actions annotations, GitLab CI reports, Jenkins JUnit XML).
5. **Human-readable security summary.** The validator emits a "policy summary" report — a plain-language account of what the agent is permitted to do — suitable for inclusion in a security review package without modification.

## **Composition + inheritance.** Org-level base policies can be extended by team-level and agent-level policies. The validator enforces that child policies cannot exceed parent permissions; violations fail the build.
<!-- task-id: 012-policy-as-code-validation-md/composition-inheritance-org-level-base-policies-can-be-extended-by-team-level-an/production -->
<!-- adr: 012-policy-as-code-validation.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Manifests and Behavioral Contracts are validated by a dedicated CLI (`aegis validate`) designed for CI/CD integration. Properties:

1. **Schema validation.** The manifest is checked against a versioned JSON Schema; all type, format, and required-field errors are reported with file/line context.
2. **Policy linting.** A set of rules detects common misconfigurations: overly broad file paths (`/`, `/home`), wildcard tools, missing approval gates on write actions, network grants without justification, etc.
3. **Composition + inheritance.** Org-level base policies can be extended by team-level and agent-level policies. The validator enforces that child policies cannot exceed parent permissions; violations fail the build.
4. **Structured JSON output.** Validation output is structured for CI consumption (GitHub Actions annotations, GitLab CI reports, Jenkins JUnit XML).
5. **Human-readable security summary.** The validator emits a "policy summary" report — a plain-language account of what the agent is permitted to do — suitable for inclusion in a security review package without modification.

## Write tests for: **Structured JSON output.** Validation output is structured for CI consumption (GitHub Actions annotations, GitLab CI reports, Jenkins JUnit XML).
<!-- task-id: 012-policy-as-code-validation-md/structured-json-output-validation-output-is-structured-for-ci-consumption-github/test -->
<!-- adr: 012-policy-as-code-validation.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Manifests and Behavioral Contracts are validated by a dedicated CLI (`aegis validate`) designed for CI/CD integration. Properties:

1. **Schema validation.** The manifest is checked against a versioned JSON Schema; all type, format, and required-field errors are reported with file/line context.
2. **Policy linting.** A set of rules detects common misconfigurations: overly broad file paths (`/`, `/home`), wildcard tools, missing approval gates on write actions, network grants without justification, etc.
3. **Composition + inheritance.** Org-level base policies can be extended by team-level and agent-level policies. The validator enforces that child policies cannot exceed parent permissions; violations fail the build.
4. **Structured JSON output.** Validation output is structured for CI consumption (GitHub Actions annotations, GitLab CI reports, Jenkins JUnit XML).
5. **Human-readable security summary.** The validator emits a "policy summary" report — a plain-language account of what the agent is permitted to do — suitable for inclusion in a security review package without modification.

## **Structured JSON output.** Validation output is structured for CI consumption (GitHub Actions annotations, GitLab CI reports, Jenkins JUnit XML).
<!-- task-id: 012-policy-as-code-validation-md/structured-json-output-validation-output-is-structured-for-ci-consumption-github/production -->
<!-- adr: 012-policy-as-code-validation.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Manifests and Behavioral Contracts are validated by a dedicated CLI (`aegis validate`) designed for CI/CD integration. Properties:

1. **Schema validation.** The manifest is checked against a versioned JSON Schema; all type, format, and required-field errors are reported with file/line context.
2. **Policy linting.** A set of rules detects common misconfigurations: overly broad file paths (`/`, `/home`), wildcard tools, missing approval gates on write actions, network grants without justification, etc.
3. **Composition + inheritance.** Org-level base policies can be extended by team-level and agent-level policies. The validator enforces that child policies cannot exceed parent permissions; violations fail the build.
4. **Structured JSON output.** Validation output is structured for CI consumption (GitHub Actions annotations, GitLab CI reports, Jenkins JUnit XML).
5. **Human-readable security summary.** The validator emits a "policy summary" report — a plain-language account of what the agent is permitted to do — suitable for inclusion in a security review package without modification.

## Write tests for: **Human-readable security summary.** The validator emits a "policy summary" report — a plain-language account of what the agent is permitted to do — suitable for inclusion in a security review package without modification.
<!-- task-id: 012-policy-as-code-validation-md/human-readable-security-summary-the-validator-emits-a-policy-summary-report-a-pl/test -->
<!-- adr: 012-policy-as-code-validation.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Manifests and Behavioral Contracts are validated by a dedicated CLI (`aegis validate`) designed for CI/CD integration. Properties:

1. **Schema validation.** The manifest is checked against a versioned JSON Schema; all type, format, and required-field errors are reported with file/line context.
2. **Policy linting.** A set of rules detects common misconfigurations: overly broad file paths (`/`, `/home`), wildcard tools, missing approval gates on write actions, network grants without justification, etc.
3. **Composition + inheritance.** Org-level base policies can be extended by team-level and agent-level policies. The validator enforces that child policies cannot exceed parent permissions; violations fail the build.
4. **Structured JSON output.** Validation output is structured for CI consumption (GitHub Actions annotations, GitLab CI reports, Jenkins JUnit XML).
5. **Human-readable security summary.** The validator emits a "policy summary" report — a plain-language account of what the agent is permitted to do — suitable for inclusion in a security review package without modification.

## **Human-readable security summary.** The validator emits a "policy summary" report — a plain-language account of what the agent is permitted to do — suitable for inclusion in a security review package without modification.
<!-- task-id: 012-policy-as-code-validation-md/human-readable-security-summary-the-validator-emits-a-policy-summary-report-a-pl/production -->
<!-- adr: 012-policy-as-code-validation.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Manifests and Behavioral Contracts are validated by a dedicated CLI (`aegis validate`) designed for CI/CD integration. Properties:

1. **Schema validation.** The manifest is checked against a versioned JSON Schema; all type, format, and required-field errors are reported with file/line context.
2. **Policy linting.** A set of rules detects common misconfigurations: overly broad file paths (`/`, `/home`), wildcard tools, missing approval gates on write actions, network grants without justification, etc.
3. **Composition + inheritance.** Org-level base policies can be extended by team-level and agent-level policies. The validator enforces that child policies cannot exceed parent permissions; violations fail the build.
4. **Structured JSON output.** Validation output is structured for CI consumption (GitHub Actions annotations, GitLab CI reports, Jenkins JUnit XML).
5. **Human-readable security summary.** The validator emits a "policy summary" report — a plain-language account of what the agent is permitted to do — suitable for inclusion in a security review package without modification.

## Write tests for: **Models = OCI artifacts.** GGUF (and equivalent) model files are wrapped as OCI artifacts and pushed to standard OCI registries (Harbor, Artifactory, AWS ECR, Quay).
<!-- task-id: 013-oci-artifacts-for-model-distribution-md/models-oci-artifacts-gguf-and-equivalent-model-files-are-wrapped-as-oci-artifact/test -->
<!-- adr: 013-oci-artifacts-for-model-distribution.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Aegis-Node packages and distributes models as OCI (Open Container Initiative) artifacts. Properties:

1. **Models = OCI artifacts.** GGUF (and equivalent) model files are wrapped as OCI artifacts and pushed to standard OCI registries (Harbor, Artifactory, AWS ECR, Quay).
2. **No background updates.** The runtime never pulls updates on its own. Model updates are explicit operator actions: `aegis pull internal.registry.com/models/llama-3-1b:v1.2`.
3. **Signature verification at load.** Before loading any model into memory, the runtime verifies the Cosign signature and SHA-256 hash. Missing or invalid signatures cause boot refusal.
4. **Chat-template scope.** Verification covers the GGUF file *and* the chat-template metadata bundled with it; template-only poisoning is detected.
5. **Air-gapped first.** The model pull path works against an internal registry with no public-internet dependency. Public registries are an opt-in remote source for internet-connected developers.

## **Models = OCI artifacts.** GGUF (and equivalent) model files are wrapped as OCI artifacts and pushed to standard OCI registries (Harbor, Artifactory, AWS ECR, Quay).
<!-- task-id: 013-oci-artifacts-for-model-distribution-md/models-oci-artifacts-gguf-and-equivalent-model-files-are-wrapped-as-oci-artifact/production -->
<!-- adr: 013-oci-artifacts-for-model-distribution.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Aegis-Node packages and distributes models as OCI (Open Container Initiative) artifacts. Properties:

1. **Models = OCI artifacts.** GGUF (and equivalent) model files are wrapped as OCI artifacts and pushed to standard OCI registries (Harbor, Artifactory, AWS ECR, Quay).
2. **No background updates.** The runtime never pulls updates on its own. Model updates are explicit operator actions: `aegis pull internal.registry.com/models/llama-3-1b:v1.2`.
3. **Signature verification at load.** Before loading any model into memory, the runtime verifies the Cosign signature and SHA-256 hash. Missing or invalid signatures cause boot refusal.
4. **Chat-template scope.** Verification covers the GGUF file *and* the chat-template metadata bundled with it; template-only poisoning is detected.
5. **Air-gapped first.** The model pull path works against an internal registry with no public-internet dependency. Public registries are an opt-in remote source for internet-connected developers.

## Write tests for: **No background updates.** The runtime never pulls updates on its own. Model updates are explicit operator actions: `aegis pull internal.registry.com/models/llama-3-1b:v1.2`.
<!-- task-id: 013-oci-artifacts-for-model-distribution-md/no-background-updates-the-runtime-never-pulls-updates-on-its-own-model-updates-a/test -->
<!-- adr: 013-oci-artifacts-for-model-distribution.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Aegis-Node packages and distributes models as OCI (Open Container Initiative) artifacts. Properties:

1. **Models = OCI artifacts.** GGUF (and equivalent) model files are wrapped as OCI artifacts and pushed to standard OCI registries (Harbor, Artifactory, AWS ECR, Quay).
2. **No background updates.** The runtime never pulls updates on its own. Model updates are explicit operator actions: `aegis pull internal.registry.com/models/llama-3-1b:v1.2`.
3. **Signature verification at load.** Before loading any model into memory, the runtime verifies the Cosign signature and SHA-256 hash. Missing or invalid signatures cause boot refusal.
4. **Chat-template scope.** Verification covers the GGUF file *and* the chat-template metadata bundled with it; template-only poisoning is detected.
5. **Air-gapped first.** The model pull path works against an internal registry with no public-internet dependency. Public registries are an opt-in remote source for internet-connected developers.

## **No background updates.** The runtime never pulls updates on its own. Model updates are explicit operator actions: `aegis pull internal.registry.com/models/llama-3-1b:v1.2`.
<!-- task-id: 013-oci-artifacts-for-model-distribution-md/no-background-updates-the-runtime-never-pulls-updates-on-its-own-model-updates-a/production -->
<!-- adr: 013-oci-artifacts-for-model-distribution.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Aegis-Node packages and distributes models as OCI (Open Container Initiative) artifacts. Properties:

1. **Models = OCI artifacts.** GGUF (and equivalent) model files are wrapped as OCI artifacts and pushed to standard OCI registries (Harbor, Artifactory, AWS ECR, Quay).
2. **No background updates.** The runtime never pulls updates on its own. Model updates are explicit operator actions: `aegis pull internal.registry.com/models/llama-3-1b:v1.2`.
3. **Signature verification at load.** Before loading any model into memory, the runtime verifies the Cosign signature and SHA-256 hash. Missing or invalid signatures cause boot refusal.
4. **Chat-template scope.** Verification covers the GGUF file *and* the chat-template metadata bundled with it; template-only poisoning is detected.
5. **Air-gapped first.** The model pull path works against an internal registry with no public-internet dependency. Public registries are an opt-in remote source for internet-connected developers.

## Write tests for: **Signature verification at load.** Before loading any model into memory, the runtime verifies the Cosign signature and SHA-256 hash. Missing or invalid signatures cause boot refusal.
<!-- task-id: 013-oci-artifacts-for-model-distribution-md/signature-verification-at-load-before-loading-any-model-into-memory-the-runtime-/test -->
<!-- adr: 013-oci-artifacts-for-model-distribution.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Aegis-Node packages and distributes models as OCI (Open Container Initiative) artifacts. Properties:

1. **Models = OCI artifacts.** GGUF (and equivalent) model files are wrapped as OCI artifacts and pushed to standard OCI registries (Harbor, Artifactory, AWS ECR, Quay).
2. **No background updates.** The runtime never pulls updates on its own. Model updates are explicit operator actions: `aegis pull internal.registry.com/models/llama-3-1b:v1.2`.
3. **Signature verification at load.** Before loading any model into memory, the runtime verifies the Cosign signature and SHA-256 hash. Missing or invalid signatures cause boot refusal.
4. **Chat-template scope.** Verification covers the GGUF file *and* the chat-template metadata bundled with it; template-only poisoning is detected.
5. **Air-gapped first.** The model pull path works against an internal registry with no public-internet dependency. Public registries are an opt-in remote source for internet-connected developers.

## **Signature verification at load.** Before loading any model into memory, the runtime verifies the Cosign signature and SHA-256 hash. Missing or invalid signatures cause boot refusal.
<!-- task-id: 013-oci-artifacts-for-model-distribution-md/signature-verification-at-load-before-loading-any-model-into-memory-the-runtime-/production -->
<!-- adr: 013-oci-artifacts-for-model-distribution.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Aegis-Node packages and distributes models as OCI (Open Container Initiative) artifacts. Properties:

1. **Models = OCI artifacts.** GGUF (and equivalent) model files are wrapped as OCI artifacts and pushed to standard OCI registries (Harbor, Artifactory, AWS ECR, Quay).
2. **No background updates.** The runtime never pulls updates on its own. Model updates are explicit operator actions: `aegis pull internal.registry.com/models/llama-3-1b:v1.2`.
3. **Signature verification at load.** Before loading any model into memory, the runtime verifies the Cosign signature and SHA-256 hash. Missing or invalid signatures cause boot refusal.
4. **Chat-template scope.** Verification covers the GGUF file *and* the chat-template metadata bundled with it; template-only poisoning is detected.
5. **Air-gapped first.** The model pull path works against an internal registry with no public-internet dependency. Public registries are an opt-in remote source for internet-connected developers.

## Write tests for: **Chat-template scope.** Verification covers the GGUF file *and* the chat-template metadata bundled with it; template-only poisoning is detected.
<!-- task-id: 013-oci-artifacts-for-model-distribution-md/chat-template-scope-verification-covers-the-gguf-file-and-the-chat-template-meta/test -->
<!-- adr: 013-oci-artifacts-for-model-distribution.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Aegis-Node packages and distributes models as OCI (Open Container Initiative) artifacts. Properties:

1. **Models = OCI artifacts.** GGUF (and equivalent) model files are wrapped as OCI artifacts and pushed to standard OCI registries (Harbor, Artifactory, AWS ECR, Quay).
2. **No background updates.** The runtime never pulls updates on its own. Model updates are explicit operator actions: `aegis pull internal.registry.com/models/llama-3-1b:v1.2`.
3. **Signature verification at load.** Before loading any model into memory, the runtime verifies the Cosign signature and SHA-256 hash. Missing or invalid signatures cause boot refusal.
4. **Chat-template scope.** Verification covers the GGUF file *and* the chat-template metadata bundled with it; template-only poisoning is detected.
5. **Air-gapped first.** The model pull path works against an internal registry with no public-internet dependency. Public registries are an opt-in remote source for internet-connected developers.

## **Chat-template scope.** Verification covers the GGUF file *and* the chat-template metadata bundled with it; template-only poisoning is detected.
<!-- task-id: 013-oci-artifacts-for-model-distribution-md/chat-template-scope-verification-covers-the-gguf-file-and-the-chat-template-meta/production -->
<!-- adr: 013-oci-artifacts-for-model-distribution.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Aegis-Node packages and distributes models as OCI (Open Container Initiative) artifacts. Properties:

1. **Models = OCI artifacts.** GGUF (and equivalent) model files are wrapped as OCI artifacts and pushed to standard OCI registries (Harbor, Artifactory, AWS ECR, Quay).
2. **No background updates.** The runtime never pulls updates on its own. Model updates are explicit operator actions: `aegis pull internal.registry.com/models/llama-3-1b:v1.2`.
3. **Signature verification at load.** Before loading any model into memory, the runtime verifies the Cosign signature and SHA-256 hash. Missing or invalid signatures cause boot refusal.
4. **Chat-template scope.** Verification covers the GGUF file *and* the chat-template metadata bundled with it; template-only poisoning is detected.
5. **Air-gapped first.** The model pull path works against an internal registry with no public-internet dependency. Public registries are an opt-in remote source for internet-connected developers.

## Write tests for: **Air-gapped first.** The model pull path works against an internal registry with no public-internet dependency. Public registries are an opt-in remote source for internet-connected developers.
<!-- task-id: 013-oci-artifacts-for-model-distribution-md/air-gapped-first-the-model-pull-path-works-against-an-internal-registry-with-no-/test -->
<!-- adr: 013-oci-artifacts-for-model-distribution.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Aegis-Node packages and distributes models as OCI (Open Container Initiative) artifacts. Properties:

1. **Models = OCI artifacts.** GGUF (and equivalent) model files are wrapped as OCI artifacts and pushed to standard OCI registries (Harbor, Artifactory, AWS ECR, Quay).
2. **No background updates.** The runtime never pulls updates on its own. Model updates are explicit operator actions: `aegis pull internal.registry.com/models/llama-3-1b:v1.2`.
3. **Signature verification at load.** Before loading any model into memory, the runtime verifies the Cosign signature and SHA-256 hash. Missing or invalid signatures cause boot refusal.
4. **Chat-template scope.** Verification covers the GGUF file *and* the chat-template metadata bundled with it; template-only poisoning is detected.
5. **Air-gapped first.** The model pull path works against an internal registry with no public-internet dependency. Public registries are an opt-in remote source for internet-connected developers.

## **Air-gapped first.** The model pull path works against an internal registry with no public-internet dependency. Public registries are an opt-in remote source for internet-connected developers.
<!-- task-id: 013-oci-artifacts-for-model-distribution-md/air-gapped-first-the-model-pull-path-works-against-an-internal-registry-with-no-/production -->
<!-- adr: 013-oci-artifacts-for-model-distribution.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Aegis-Node packages and distributes models as OCI (Open Container Initiative) artifacts. Properties:

1. **Models = OCI artifacts.** GGUF (and equivalent) model files are wrapped as OCI artifacts and pushed to standard OCI registries (Harbor, Artifactory, AWS ECR, Quay).
2. **No background updates.** The runtime never pulls updates on its own. Model updates are explicit operator actions: `aegis pull internal.registry.com/models/llama-3-1b:v1.2`.
3. **Signature verification at load.** Before loading any model into memory, the runtime verifies the Cosign signature and SHA-256 hash. Missing or invalid signatures cause boot refusal.
4. **Chat-template scope.** Verification covers the GGUF file *and* the chat-template metadata bundled with it; template-only poisoning is detected.
5. **Air-gapped first.** The model pull path works against an internal registry with no public-internet dependency. Public registries are an opt-in remote source for internet-connected developers.

## Write tests for: **GGUF models, quantized 4–8 bit, 1B–8B parameters.** Optimized for CPU inference on modern laptops without external acceleration.
<!-- task-id: 014-cpu-first-gguf-inference-via-llama-cpp-md/gguf-models-quantized-4-8-bit-1b-8b-parameters-optimized-for-cpu-inference-on-mo/test -->
<!-- adr: 014-cpu-first-gguf-inference-via-llama-cpp.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Phase 1 inference runs entirely on CPU using `llama.cpp` (linked as a Rust FFI binding). Properties:

1. **GGUF models, quantized 4–8 bit, 1B–8B parameters.** Optimized for CPU inference on modern laptops without external acceleration.
2. **Rust FFI binding to `llama.cpp`.** The Rust runtime owns model load/unload, generation lifecycle, and integration with the runtime's policy enforcement boundaries (network deny, FS sandbox, identity binding).
3. **No GPU code path in Phase 1.** GPU support is tracked through public GitHub issues and is invited as community contribution; it is not a Phase 1 blocker.
4. **Pluggable backend abstraction prepared, not implemented.** The Rust crate exposes an internal trait that abstracts `llama.cpp` behind a `Backend` interface so vLLM/TGI/KServe backends in Phase 2 are additive, not refactors.
5. **Deterministic-by-default sampling settings.** Where the security review demands reproducibility (replay, regression tests), the runtime supports deterministic sampling parameters; non-determinism is opt-in.

## **GGUF models, quantized 4–8 bit, 1B–8B parameters.** Optimized for CPU inference on modern laptops without external acceleration.
<!-- task-id: 014-cpu-first-gguf-inference-via-llama-cpp-md/gguf-models-quantized-4-8-bit-1b-8b-parameters-optimized-for-cpu-inference-on-mo/production -->
<!-- adr: 014-cpu-first-gguf-inference-via-llama-cpp.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Phase 1 inference runs entirely on CPU using `llama.cpp` (linked as a Rust FFI binding). Properties:

1. **GGUF models, quantized 4–8 bit, 1B–8B parameters.** Optimized for CPU inference on modern laptops without external acceleration.
2. **Rust FFI binding to `llama.cpp`.** The Rust runtime owns model load/unload, generation lifecycle, and integration with the runtime's policy enforcement boundaries (network deny, FS sandbox, identity binding).
3. **No GPU code path in Phase 1.** GPU support is tracked through public GitHub issues and is invited as community contribution; it is not a Phase 1 blocker.
4. **Pluggable backend abstraction prepared, not implemented.** The Rust crate exposes an internal trait that abstracts `llama.cpp` behind a `Backend` interface so vLLM/TGI/KServe backends in Phase 2 are additive, not refactors.
5. **Deterministic-by-default sampling settings.** Where the security review demands reproducibility (replay, regression tests), the runtime supports deterministic sampling parameters; non-determinism is opt-in.

## Write tests for: **Rust FFI binding to `llama.cpp`.** The Rust runtime owns model load/unload, generation lifecycle, and integration with the runtime's policy enforcement boundaries (network deny, FS sandbox, identity binding).
<!-- task-id: 014-cpu-first-gguf-inference-via-llama-cpp-md/rust-ffi-binding-to-llama-cpp-the-rust-runtime-owns-model-load-unload-generation/test -->
<!-- adr: 014-cpu-first-gguf-inference-via-llama-cpp.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Phase 1 inference runs entirely on CPU using `llama.cpp` (linked as a Rust FFI binding). Properties:

1. **GGUF models, quantized 4–8 bit, 1B–8B parameters.** Optimized for CPU inference on modern laptops without external acceleration.
2. **Rust FFI binding to `llama.cpp`.** The Rust runtime owns model load/unload, generation lifecycle, and integration with the runtime's policy enforcement boundaries (network deny, FS sandbox, identity binding).
3. **No GPU code path in Phase 1.** GPU support is tracked through public GitHub issues and is invited as community contribution; it is not a Phase 1 blocker.
4. **Pluggable backend abstraction prepared, not implemented.** The Rust crate exposes an internal trait that abstracts `llama.cpp` behind a `Backend` interface so vLLM/TGI/KServe backends in Phase 2 are additive, not refactors.
5. **Deterministic-by-default sampling settings.** Where the security review demands reproducibility (replay, regression tests), the runtime supports deterministic sampling parameters; non-determinism is opt-in.

## **Rust FFI binding to `llama.cpp`.** The Rust runtime owns model load/unload, generation lifecycle, and integration with the runtime's policy enforcement boundaries (network deny, FS sandbox, identity binding).
<!-- task-id: 014-cpu-first-gguf-inference-via-llama-cpp-md/rust-ffi-binding-to-llama-cpp-the-rust-runtime-owns-model-load-unload-generation/production -->
<!-- adr: 014-cpu-first-gguf-inference-via-llama-cpp.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Phase 1 inference runs entirely on CPU using `llama.cpp` (linked as a Rust FFI binding). Properties:

1. **GGUF models, quantized 4–8 bit, 1B–8B parameters.** Optimized for CPU inference on modern laptops without external acceleration.
2. **Rust FFI binding to `llama.cpp`.** The Rust runtime owns model load/unload, generation lifecycle, and integration with the runtime's policy enforcement boundaries (network deny, FS sandbox, identity binding).
3. **No GPU code path in Phase 1.** GPU support is tracked through public GitHub issues and is invited as community contribution; it is not a Phase 1 blocker.
4. **Pluggable backend abstraction prepared, not implemented.** The Rust crate exposes an internal trait that abstracts `llama.cpp` behind a `Backend` interface so vLLM/TGI/KServe backends in Phase 2 are additive, not refactors.
5. **Deterministic-by-default sampling settings.** Where the security review demands reproducibility (replay, regression tests), the runtime supports deterministic sampling parameters; non-determinism is opt-in.

## Write tests for: **No GPU code path in Phase 1.** GPU support is tracked through public GitHub issues and is invited as community contribution; it is not a Phase 1 blocker.
<!-- task-id: 014-cpu-first-gguf-inference-via-llama-cpp-md/no-gpu-code-path-in-phase-1-gpu-support-is-tracked-through-public-github-issues-/test -->
<!-- adr: 014-cpu-first-gguf-inference-via-llama-cpp.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Phase 1 inference runs entirely on CPU using `llama.cpp` (linked as a Rust FFI binding). Properties:

1. **GGUF models, quantized 4–8 bit, 1B–8B parameters.** Optimized for CPU inference on modern laptops without external acceleration.
2. **Rust FFI binding to `llama.cpp`.** The Rust runtime owns model load/unload, generation lifecycle, and integration with the runtime's policy enforcement boundaries (network deny, FS sandbox, identity binding).
3. **No GPU code path in Phase 1.** GPU support is tracked through public GitHub issues and is invited as community contribution; it is not a Phase 1 blocker.
4. **Pluggable backend abstraction prepared, not implemented.** The Rust crate exposes an internal trait that abstracts `llama.cpp` behind a `Backend` interface so vLLM/TGI/KServe backends in Phase 2 are additive, not refactors.
5. **Deterministic-by-default sampling settings.** Where the security review demands reproducibility (replay, regression tests), the runtime supports deterministic sampling parameters; non-determinism is opt-in.

## **No GPU code path in Phase 1.** GPU support is tracked through public GitHub issues and is invited as community contribution; it is not a Phase 1 blocker.
<!-- task-id: 014-cpu-first-gguf-inference-via-llama-cpp-md/no-gpu-code-path-in-phase-1-gpu-support-is-tracked-through-public-github-issues-/production -->
<!-- adr: 014-cpu-first-gguf-inference-via-llama-cpp.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Phase 1 inference runs entirely on CPU using `llama.cpp` (linked as a Rust FFI binding). Properties:

1. **GGUF models, quantized 4–8 bit, 1B–8B parameters.** Optimized for CPU inference on modern laptops without external acceleration.
2. **Rust FFI binding to `llama.cpp`.** The Rust runtime owns model load/unload, generation lifecycle, and integration with the runtime's policy enforcement boundaries (network deny, FS sandbox, identity binding).
3. **No GPU code path in Phase 1.** GPU support is tracked through public GitHub issues and is invited as community contribution; it is not a Phase 1 blocker.
4. **Pluggable backend abstraction prepared, not implemented.** The Rust crate exposes an internal trait that abstracts `llama.cpp` behind a `Backend` interface so vLLM/TGI/KServe backends in Phase 2 are additive, not refactors.
5. **Deterministic-by-default sampling settings.** Where the security review demands reproducibility (replay, regression tests), the runtime supports deterministic sampling parameters; non-determinism is opt-in.

## Write tests for: **Pluggable backend abstraction prepared, not implemented.** The Rust crate exposes an internal trait that abstracts `llama.cpp` behind a `Backend` interface so vLLM/TGI/KServe backends in Phase 2 are additive, not refactors.
<!-- task-id: 014-cpu-first-gguf-inference-via-llama-cpp-md/pluggable-backend-abstraction-prepared-not-implemented-the-rust-crate-exposes-an/test -->
<!-- adr: 014-cpu-first-gguf-inference-via-llama-cpp.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Phase 1 inference runs entirely on CPU using `llama.cpp` (linked as a Rust FFI binding). Properties:

1. **GGUF models, quantized 4–8 bit, 1B–8B parameters.** Optimized for CPU inference on modern laptops without external acceleration.
2. **Rust FFI binding to `llama.cpp`.** The Rust runtime owns model load/unload, generation lifecycle, and integration with the runtime's policy enforcement boundaries (network deny, FS sandbox, identity binding).
3. **No GPU code path in Phase 1.** GPU support is tracked through public GitHub issues and is invited as community contribution; it is not a Phase 1 blocker.
4. **Pluggable backend abstraction prepared, not implemented.** The Rust crate exposes an internal trait that abstracts `llama.cpp` behind a `Backend` interface so vLLM/TGI/KServe backends in Phase 2 are additive, not refactors.
5. **Deterministic-by-default sampling settings.** Where the security review demands reproducibility (replay, regression tests), the runtime supports deterministic sampling parameters; non-determinism is opt-in.

## **Pluggable backend abstraction prepared, not implemented.** The Rust crate exposes an internal trait that abstracts `llama.cpp` behind a `Backend` interface so vLLM/TGI/KServe backends in Phase 2 are additive, not refactors.
<!-- task-id: 014-cpu-first-gguf-inference-via-llama-cpp-md/pluggable-backend-abstraction-prepared-not-implemented-the-rust-crate-exposes-an/production -->
<!-- adr: 014-cpu-first-gguf-inference-via-llama-cpp.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Phase 1 inference runs entirely on CPU using `llama.cpp` (linked as a Rust FFI binding). Properties:

1. **GGUF models, quantized 4–8 bit, 1B–8B parameters.** Optimized for CPU inference on modern laptops without external acceleration.
2. **Rust FFI binding to `llama.cpp`.** The Rust runtime owns model load/unload, generation lifecycle, and integration with the runtime's policy enforcement boundaries (network deny, FS sandbox, identity binding).
3. **No GPU code path in Phase 1.** GPU support is tracked through public GitHub issues and is invited as community contribution; it is not a Phase 1 blocker.
4. **Pluggable backend abstraction prepared, not implemented.** The Rust crate exposes an internal trait that abstracts `llama.cpp` behind a `Backend` interface so vLLM/TGI/KServe backends in Phase 2 are additive, not refactors.
5. **Deterministic-by-default sampling settings.** Where the security review demands reproducibility (replay, regression tests), the runtime supports deterministic sampling parameters; non-determinism is opt-in.

## Write tests for: **Deterministic-by-default sampling settings.** Where the security review demands reproducibility (replay, regression tests), the runtime supports deterministic sampling parameters; non-determinism is opt-in.
<!-- task-id: 014-cpu-first-gguf-inference-via-llama-cpp-md/deterministic-by-default-sampling-settings-where-the-security-review-demands-rep/test -->
<!-- adr: 014-cpu-first-gguf-inference-via-llama-cpp.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Phase 1 inference runs entirely on CPU using `llama.cpp` (linked as a Rust FFI binding). Properties:

1. **GGUF models, quantized 4–8 bit, 1B–8B parameters.** Optimized for CPU inference on modern laptops without external acceleration.
2. **Rust FFI binding to `llama.cpp`.** The Rust runtime owns model load/unload, generation lifecycle, and integration with the runtime's policy enforcement boundaries (network deny, FS sandbox, identity binding).
3. **No GPU code path in Phase 1.** GPU support is tracked through public GitHub issues and is invited as community contribution; it is not a Phase 1 blocker.
4. **Pluggable backend abstraction prepared, not implemented.** The Rust crate exposes an internal trait that abstracts `llama.cpp` behind a `Backend` interface so vLLM/TGI/KServe backends in Phase 2 are additive, not refactors.
5. **Deterministic-by-default sampling settings.** Where the security review demands reproducibility (replay, regression tests), the runtime supports deterministic sampling parameters; non-determinism is opt-in.

## **Deterministic-by-default sampling settings.** Where the security review demands reproducibility (replay, regression tests), the runtime supports deterministic sampling parameters; non-determinism is opt-in.
<!-- task-id: 014-cpu-first-gguf-inference-via-llama-cpp-md/deterministic-by-default-sampling-settings-where-the-security-review-demands-rep/production -->
<!-- adr: 014-cpu-first-gguf-inference-via-llama-cpp.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Phase 1 inference runs entirely on CPU using `llama.cpp` (linked as a Rust FFI binding). Properties:

1. **GGUF models, quantized 4–8 bit, 1B–8B parameters.** Optimized for CPU inference on modern laptops without external acceleration.
2. **Rust FFI binding to `llama.cpp`.** The Rust runtime owns model load/unload, generation lifecycle, and integration with the runtime's policy enforcement boundaries (network deny, FS sandbox, identity binding).
3. **No GPU code path in Phase 1.** GPU support is tracked through public GitHub issues and is invited as community contribution; it is not a Phase 1 blocker.
4. **Pluggable backend abstraction prepared, not implemented.** The Rust crate exposes an internal trait that abstracts `llama.cpp` behind a `Backend` interface so vLLM/TGI/KServe backends in Phase 2 are additive, not refactors.
5. **Deterministic-by-default sampling settings.** Where the security review demands reproducibility (replay, regression tests), the runtime supports deterministic sampling parameters; non-determinism is opt-in.

## Write tests for: **Phase 1 — Local CLI (MVP).** Single-binary install. CPU-first inference. F1–F10 features fully implemented locally. Target: developers, security engineers, initial PoCs.
<!-- task-id: 015-three-phase-deployment-roadmap-md/phase-1-local-cli-mvp-single-binary-install-cpu-first-inference-f1-f10-features-/test -->
<!-- adr: 015-three-phase-deployment-roadmap.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Adopt a deliberate three-phase deployment roadmap, with the same Permission Manifest and Trajectory Ledger formats spanning all three phases. Properties:

1. **Phase 1 — Local CLI (MVP).** Single-binary install. CPU-first inference. F1–F10 features fully implemented locally. Target: developers, security engineers, initial PoCs.
2. **Phase 2 — Kubernetes Runtime.** Same manifest semantics, same ledger format, now enforced as cluster workloads. Adds: namespace isolation, ServiceAccount → SPIFFE identity binding, NetworkPolicies, persistent ledger storage, GPU model backends (vLLM/TGI/KServe).
3. **Phase 3 — OpenShift Enterprise.** Same again, deeper enterprise integration. Adds: SCCs, OpenShift AI integration, disconnected install, GitOps deployment, RBAC mapping, automated CMMC/FedRAMP report exports.
4. **No format migration between phases.** A manifest written for Phase 1 runs unchanged in Phase 3 (within the limits of permission scope). A ledger from Phase 1 replays in the Phase 2 viewer and vice versa.
5. **Phase boundaries are real.** Phase 2 work does not start until Phase 1 hits its security-review milestone. Resists the temptation to chase enterprise revenue prematurely.

## **Phase 1 — Local CLI (MVP).** Single-binary install. CPU-first inference. F1–F10 features fully implemented locally. Target: developers, security engineers, initial PoCs.
<!-- task-id: 015-three-phase-deployment-roadmap-md/phase-1-local-cli-mvp-single-binary-install-cpu-first-inference-f1-f10-features-/production -->
<!-- adr: 015-three-phase-deployment-roadmap.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Adopt a deliberate three-phase deployment roadmap, with the same Permission Manifest and Trajectory Ledger formats spanning all three phases. Properties:

1. **Phase 1 — Local CLI (MVP).** Single-binary install. CPU-first inference. F1–F10 features fully implemented locally. Target: developers, security engineers, initial PoCs.
2. **Phase 2 — Kubernetes Runtime.** Same manifest semantics, same ledger format, now enforced as cluster workloads. Adds: namespace isolation, ServiceAccount → SPIFFE identity binding, NetworkPolicies, persistent ledger storage, GPU model backends (vLLM/TGI/KServe).
3. **Phase 3 — OpenShift Enterprise.** Same again, deeper enterprise integration. Adds: SCCs, OpenShift AI integration, disconnected install, GitOps deployment, RBAC mapping, automated CMMC/FedRAMP report exports.
4. **No format migration between phases.** A manifest written for Phase 1 runs unchanged in Phase 3 (within the limits of permission scope). A ledger from Phase 1 replays in the Phase 2 viewer and vice versa.
5. **Phase boundaries are real.** Phase 2 work does not start until Phase 1 hits its security-review milestone. Resists the temptation to chase enterprise revenue prematurely.

## Write tests for: **Phase 2 — Kubernetes Runtime.** Same manifest semantics, same ledger format, now enforced as cluster workloads. Adds: namespace isolation, ServiceAccount → SPIFFE identity binding, NetworkPolicies, persistent ledger storage, GPU model backends (vLLM/TGI/KServe).
<!-- task-id: 015-three-phase-deployment-roadmap-md/phase-2-kubernetes-runtime-same-manifest-semantics-same-ledger-format-now-enforc/test -->
<!-- adr: 015-three-phase-deployment-roadmap.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Adopt a deliberate three-phase deployment roadmap, with the same Permission Manifest and Trajectory Ledger formats spanning all three phases. Properties:

1. **Phase 1 — Local CLI (MVP).** Single-binary install. CPU-first inference. F1–F10 features fully implemented locally. Target: developers, security engineers, initial PoCs.
2. **Phase 2 — Kubernetes Runtime.** Same manifest semantics, same ledger format, now enforced as cluster workloads. Adds: namespace isolation, ServiceAccount → SPIFFE identity binding, NetworkPolicies, persistent ledger storage, GPU model backends (vLLM/TGI/KServe).
3. **Phase 3 — OpenShift Enterprise.** Same again, deeper enterprise integration. Adds: SCCs, OpenShift AI integration, disconnected install, GitOps deployment, RBAC mapping, automated CMMC/FedRAMP report exports.
4. **No format migration between phases.** A manifest written for Phase 1 runs unchanged in Phase 3 (within the limits of permission scope). A ledger from Phase 1 replays in the Phase 2 viewer and vice versa.
5. **Phase boundaries are real.** Phase 2 work does not start until Phase 1 hits its security-review milestone. Resists the temptation to chase enterprise revenue prematurely.

## **Phase 2 — Kubernetes Runtime.** Same manifest semantics, same ledger format, now enforced as cluster workloads. Adds: namespace isolation, ServiceAccount → SPIFFE identity binding, NetworkPolicies, persistent ledger storage, GPU model backends (vLLM/TGI/KServe).
<!-- task-id: 015-three-phase-deployment-roadmap-md/phase-2-kubernetes-runtime-same-manifest-semantics-same-ledger-format-now-enforc/production -->
<!-- adr: 015-three-phase-deployment-roadmap.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Adopt a deliberate three-phase deployment roadmap, with the same Permission Manifest and Trajectory Ledger formats spanning all three phases. Properties:

1. **Phase 1 — Local CLI (MVP).** Single-binary install. CPU-first inference. F1–F10 features fully implemented locally. Target: developers, security engineers, initial PoCs.
2. **Phase 2 — Kubernetes Runtime.** Same manifest semantics, same ledger format, now enforced as cluster workloads. Adds: namespace isolation, ServiceAccount → SPIFFE identity binding, NetworkPolicies, persistent ledger storage, GPU model backends (vLLM/TGI/KServe).
3. **Phase 3 — OpenShift Enterprise.** Same again, deeper enterprise integration. Adds: SCCs, OpenShift AI integration, disconnected install, GitOps deployment, RBAC mapping, automated CMMC/FedRAMP report exports.
4. **No format migration between phases.** A manifest written for Phase 1 runs unchanged in Phase 3 (within the limits of permission scope). A ledger from Phase 1 replays in the Phase 2 viewer and vice versa.
5. **Phase boundaries are real.** Phase 2 work does not start until Phase 1 hits its security-review milestone. Resists the temptation to chase enterprise revenue prematurely.

## Write tests for: **Phase 3 — OpenShift Enterprise.** Same again, deeper enterprise integration. Adds: SCCs, OpenShift AI integration, disconnected install, GitOps deployment, RBAC mapping, automated CMMC/FedRAMP report exports.
<!-- task-id: 015-three-phase-deployment-roadmap-md/phase-3-openshift-enterprise-same-again-deeper-enterprise-integration-adds-sccs-/test -->
<!-- adr: 015-three-phase-deployment-roadmap.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Adopt a deliberate three-phase deployment roadmap, with the same Permission Manifest and Trajectory Ledger formats spanning all three phases. Properties:

1. **Phase 1 — Local CLI (MVP).** Single-binary install. CPU-first inference. F1–F10 features fully implemented locally. Target: developers, security engineers, initial PoCs.
2. **Phase 2 — Kubernetes Runtime.** Same manifest semantics, same ledger format, now enforced as cluster workloads. Adds: namespace isolation, ServiceAccount → SPIFFE identity binding, NetworkPolicies, persistent ledger storage, GPU model backends (vLLM/TGI/KServe).
3. **Phase 3 — OpenShift Enterprise.** Same again, deeper enterprise integration. Adds: SCCs, OpenShift AI integration, disconnected install, GitOps deployment, RBAC mapping, automated CMMC/FedRAMP report exports.
4. **No format migration between phases.** A manifest written for Phase 1 runs unchanged in Phase 3 (within the limits of permission scope). A ledger from Phase 1 replays in the Phase 2 viewer and vice versa.
5. **Phase boundaries are real.** Phase 2 work does not start until Phase 1 hits its security-review milestone. Resists the temptation to chase enterprise revenue prematurely.

## **Phase 3 — OpenShift Enterprise.** Same again, deeper enterprise integration. Adds: SCCs, OpenShift AI integration, disconnected install, GitOps deployment, RBAC mapping, automated CMMC/FedRAMP report exports.
<!-- task-id: 015-three-phase-deployment-roadmap-md/phase-3-openshift-enterprise-same-again-deeper-enterprise-integration-adds-sccs-/production -->
<!-- adr: 015-three-phase-deployment-roadmap.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Adopt a deliberate three-phase deployment roadmap, with the same Permission Manifest and Trajectory Ledger formats spanning all three phases. Properties:

1. **Phase 1 — Local CLI (MVP).** Single-binary install. CPU-first inference. F1–F10 features fully implemented locally. Target: developers, security engineers, initial PoCs.
2. **Phase 2 — Kubernetes Runtime.** Same manifest semantics, same ledger format, now enforced as cluster workloads. Adds: namespace isolation, ServiceAccount → SPIFFE identity binding, NetworkPolicies, persistent ledger storage, GPU model backends (vLLM/TGI/KServe).
3. **Phase 3 — OpenShift Enterprise.** Same again, deeper enterprise integration. Adds: SCCs, OpenShift AI integration, disconnected install, GitOps deployment, RBAC mapping, automated CMMC/FedRAMP report exports.
4. **No format migration between phases.** A manifest written for Phase 1 runs unchanged in Phase 3 (within the limits of permission scope). A ledger from Phase 1 replays in the Phase 2 viewer and vice versa.
5. **Phase boundaries are real.** Phase 2 work does not start until Phase 1 hits its security-review milestone. Resists the temptation to chase enterprise revenue prematurely.

## Write tests for: **No format migration between phases.** A manifest written for Phase 1 runs unchanged in Phase 3 (within the limits of permission scope). A ledger from Phase 1 replays in the Phase 2 viewer and vice versa.
<!-- task-id: 015-three-phase-deployment-roadmap-md/no-format-migration-between-phases-a-manifest-written-for-phase-1-runs-unchanged/test -->
<!-- adr: 015-three-phase-deployment-roadmap.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Adopt a deliberate three-phase deployment roadmap, with the same Permission Manifest and Trajectory Ledger formats spanning all three phases. Properties:

1. **Phase 1 — Local CLI (MVP).** Single-binary install. CPU-first inference. F1–F10 features fully implemented locally. Target: developers, security engineers, initial PoCs.
2. **Phase 2 — Kubernetes Runtime.** Same manifest semantics, same ledger format, now enforced as cluster workloads. Adds: namespace isolation, ServiceAccount → SPIFFE identity binding, NetworkPolicies, persistent ledger storage, GPU model backends (vLLM/TGI/KServe).
3. **Phase 3 — OpenShift Enterprise.** Same again, deeper enterprise integration. Adds: SCCs, OpenShift AI integration, disconnected install, GitOps deployment, RBAC mapping, automated CMMC/FedRAMP report exports.
4. **No format migration between phases.** A manifest written for Phase 1 runs unchanged in Phase 3 (within the limits of permission scope). A ledger from Phase 1 replays in the Phase 2 viewer and vice versa.
5. **Phase boundaries are real.** Phase 2 work does not start until Phase 1 hits its security-review milestone. Resists the temptation to chase enterprise revenue prematurely.

## **No format migration between phases.** A manifest written for Phase 1 runs unchanged in Phase 3 (within the limits of permission scope). A ledger from Phase 1 replays in the Phase 2 viewer and vice versa.
<!-- task-id: 015-three-phase-deployment-roadmap-md/no-format-migration-between-phases-a-manifest-written-for-phase-1-runs-unchanged/production -->
<!-- adr: 015-three-phase-deployment-roadmap.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Adopt a deliberate three-phase deployment roadmap, with the same Permission Manifest and Trajectory Ledger formats spanning all three phases. Properties:

1. **Phase 1 — Local CLI (MVP).** Single-binary install. CPU-first inference. F1–F10 features fully implemented locally. Target: developers, security engineers, initial PoCs.
2. **Phase 2 — Kubernetes Runtime.** Same manifest semantics, same ledger format, now enforced as cluster workloads. Adds: namespace isolation, ServiceAccount → SPIFFE identity binding, NetworkPolicies, persistent ledger storage, GPU model backends (vLLM/TGI/KServe).
3. **Phase 3 — OpenShift Enterprise.** Same again, deeper enterprise integration. Adds: SCCs, OpenShift AI integration, disconnected install, GitOps deployment, RBAC mapping, automated CMMC/FedRAMP report exports.
4. **No format migration between phases.** A manifest written for Phase 1 runs unchanged in Phase 3 (within the limits of permission scope). A ledger from Phase 1 replays in the Phase 2 viewer and vice versa.
5. **Phase boundaries are real.** Phase 2 work does not start until Phase 1 hits its security-review milestone. Resists the temptation to chase enterprise revenue prematurely.

## Write tests for: **Phase boundaries are real.** Phase 2 work does not start until Phase 1 hits its security-review milestone. Resists the temptation to chase enterprise revenue prematurely.
<!-- task-id: 015-three-phase-deployment-roadmap-md/phase-boundaries-are-real-phase-2-work-does-not-start-until-phase-1-hits-its-sec/test -->
<!-- adr: 015-three-phase-deployment-roadmap.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Adopt a deliberate three-phase deployment roadmap, with the same Permission Manifest and Trajectory Ledger formats spanning all three phases. Properties:

1. **Phase 1 — Local CLI (MVP).** Single-binary install. CPU-first inference. F1–F10 features fully implemented locally. Target: developers, security engineers, initial PoCs.
2. **Phase 2 — Kubernetes Runtime.** Same manifest semantics, same ledger format, now enforced as cluster workloads. Adds: namespace isolation, ServiceAccount → SPIFFE identity binding, NetworkPolicies, persistent ledger storage, GPU model backends (vLLM/TGI/KServe).
3. **Phase 3 — OpenShift Enterprise.** Same again, deeper enterprise integration. Adds: SCCs, OpenShift AI integration, disconnected install, GitOps deployment, RBAC mapping, automated CMMC/FedRAMP report exports.
4. **No format migration between phases.** A manifest written for Phase 1 runs unchanged in Phase 3 (within the limits of permission scope). A ledger from Phase 1 replays in the Phase 2 viewer and vice versa.
5. **Phase boundaries are real.** Phase 2 work does not start until Phase 1 hits its security-review milestone. Resists the temptation to chase enterprise revenue prematurely.

## **Phase boundaries are real.** Phase 2 work does not start until Phase 1 hits its security-review milestone. Resists the temptation to chase enterprise revenue prematurely.
<!-- task-id: 015-three-phase-deployment-roadmap-md/phase-boundaries-are-real-phase-2-work-does-not-start-until-phase-1-hits-its-sec/production -->
<!-- adr: 015-three-phase-deployment-roadmap.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Adopt a deliberate three-phase deployment roadmap, with the same Permission Manifest and Trajectory Ledger formats spanning all three phases. Properties:

1. **Phase 1 — Local CLI (MVP).** Single-binary install. CPU-first inference. F1–F10 features fully implemented locally. Target: developers, security engineers, initial PoCs.
2. **Phase 2 — Kubernetes Runtime.** Same manifest semantics, same ledger format, now enforced as cluster workloads. Adds: namespace isolation, ServiceAccount → SPIFFE identity binding, NetworkPolicies, persistent ledger storage, GPU model backends (vLLM/TGI/KServe).
3. **Phase 3 — OpenShift Enterprise.** Same again, deeper enterprise integration. Adds: SCCs, OpenShift AI integration, disconnected install, GitOps deployment, RBAC mapping, automated CMMC/FedRAMP report exports.
4. **No format migration between phases.** A manifest written for Phase 1 runs unchanged in Phase 3 (within the limits of permission scope). A ledger from Phase 1 replays in the Phase 2 viewer and vice versa.
5. **Phase boundaries are real.** Phase 2 work does not start until Phase 1 hits its security-review milestone. Resists the temptation to chase enterprise revenue prematurely.

## Write tests for: **Community (Apache 2.0).** F1–F10 core features, CLI, local replay viewer, manifest validator, OCI-based model distribution. The runtime that must be auditable is the runtime that is open source.
<!-- task-id: 016-open-core-licensing-model-md/community-apache-2-0-f1-f10-core-features-cli-local-replay-viewer-manifest-valid/test -->
<!-- adr: 016-open-core-licensing-model.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Adopt an open-core licensing model with three tiers:

1. **Community (Apache 2.0).** F1–F10 core features, CLI, local replay viewer, manifest validator, OCI-based model distribution. The runtime that must be auditable is the runtime that is open source.
2. **Enterprise (commercial).** Community features + Management UI, SIEM integration packs, RBAC integrations, support SLA. Targeted at mid-market enterprises and hospital systems.
3. **Sovereign (commercial).** Enterprise features + TEE attestation (SGX/SEV), automated CMMC/FedRAMP reporting, CAC/PIV authentication. Targeted at defense contractors and government agencies.

Properties:
- The community runtime must be sufficient to pass a security review on its own. Enterprise features add convenience and integration; they do not unlock previously-blocked compliance.
- All ledger and manifest formats are open. Enterprise tiers consume them; they never extend them in proprietary directions.
- Apache 2.0 is chosen over copyleft for compatibility with enterprise legal review and for the explicit patent grant.

## **Community (Apache 2.0).** F1–F10 core features, CLI, local replay viewer, manifest validator, OCI-based model distribution. The runtime that must be auditable is the runtime that is open source.
<!-- task-id: 016-open-core-licensing-model-md/community-apache-2-0-f1-f10-core-features-cli-local-replay-viewer-manifest-valid/production -->
<!-- adr: 016-open-core-licensing-model.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Adopt an open-core licensing model with three tiers:

1. **Community (Apache 2.0).** F1–F10 core features, CLI, local replay viewer, manifest validator, OCI-based model distribution. The runtime that must be auditable is the runtime that is open source.
2. **Enterprise (commercial).** Community features + Management UI, SIEM integration packs, RBAC integrations, support SLA. Targeted at mid-market enterprises and hospital systems.
3. **Sovereign (commercial).** Enterprise features + TEE attestation (SGX/SEV), automated CMMC/FedRAMP reporting, CAC/PIV authentication. Targeted at defense contractors and government agencies.

Properties:
- The community runtime must be sufficient to pass a security review on its own. Enterprise features add convenience and integration; they do not unlock previously-blocked compliance.
- All ledger and manifest formats are open. Enterprise tiers consume them; they never extend them in proprietary directions.
- Apache 2.0 is chosen over copyleft for compatibility with enterprise legal review and for the explicit patent grant.

## Write tests for: **Enterprise (commercial).** Community features + Management UI, SIEM integration packs, RBAC integrations, support SLA. Targeted at mid-market enterprises and hospital systems.
<!-- task-id: 016-open-core-licensing-model-md/enterprise-commercial-community-features-management-ui-siem-integration-packs-rb/test -->
<!-- adr: 016-open-core-licensing-model.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Adopt an open-core licensing model with three tiers:

1. **Community (Apache 2.0).** F1–F10 core features, CLI, local replay viewer, manifest validator, OCI-based model distribution. The runtime that must be auditable is the runtime that is open source.
2. **Enterprise (commercial).** Community features + Management UI, SIEM integration packs, RBAC integrations, support SLA. Targeted at mid-market enterprises and hospital systems.
3. **Sovereign (commercial).** Enterprise features + TEE attestation (SGX/SEV), automated CMMC/FedRAMP reporting, CAC/PIV authentication. Targeted at defense contractors and government agencies.

Properties:
- The community runtime must be sufficient to pass a security review on its own. Enterprise features add convenience and integration; they do not unlock previously-blocked compliance.
- All ledger and manifest formats are open. Enterprise tiers consume them; they never extend them in proprietary directions.
- Apache 2.0 is chosen over copyleft for compatibility with enterprise legal review and for the explicit patent grant.

## **Enterprise (commercial).** Community features + Management UI, SIEM integration packs, RBAC integrations, support SLA. Targeted at mid-market enterprises and hospital systems.
<!-- task-id: 016-open-core-licensing-model-md/enterprise-commercial-community-features-management-ui-siem-integration-packs-rb/production -->
<!-- adr: 016-open-core-licensing-model.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Adopt an open-core licensing model with three tiers:

1. **Community (Apache 2.0).** F1–F10 core features, CLI, local replay viewer, manifest validator, OCI-based model distribution. The runtime that must be auditable is the runtime that is open source.
2. **Enterprise (commercial).** Community features + Management UI, SIEM integration packs, RBAC integrations, support SLA. Targeted at mid-market enterprises and hospital systems.
3. **Sovereign (commercial).** Enterprise features + TEE attestation (SGX/SEV), automated CMMC/FedRAMP reporting, CAC/PIV authentication. Targeted at defense contractors and government agencies.

Properties:
- The community runtime must be sufficient to pass a security review on its own. Enterprise features add convenience and integration; they do not unlock previously-blocked compliance.
- All ledger and manifest formats are open. Enterprise tiers consume them; they never extend them in proprietary directions.
- Apache 2.0 is chosen over copyleft for compatibility with enterprise legal review and for the explicit patent grant.

## Write tests for: **Sovereign (commercial).** Enterprise features + TEE attestation (SGX/SEV), automated CMMC/FedRAMP reporting, CAC/PIV authentication. Targeted at defense contractors and government agencies.
<!-- task-id: 016-open-core-licensing-model-md/sovereign-commercial-enterprise-features-tee-attestation-sgx-sev-automated-cmmc-/test -->
<!-- adr: 016-open-core-licensing-model.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Adopt an open-core licensing model with three tiers:

1. **Community (Apache 2.0).** F1–F10 core features, CLI, local replay viewer, manifest validator, OCI-based model distribution. The runtime that must be auditable is the runtime that is open source.
2. **Enterprise (commercial).** Community features + Management UI, SIEM integration packs, RBAC integrations, support SLA. Targeted at mid-market enterprises and hospital systems.
3. **Sovereign (commercial).** Enterprise features + TEE attestation (SGX/SEV), automated CMMC/FedRAMP reporting, CAC/PIV authentication. Targeted at defense contractors and government agencies.

Properties:
- The community runtime must be sufficient to pass a security review on its own. Enterprise features add convenience and integration; they do not unlock previously-blocked compliance.
- All ledger and manifest formats are open. Enterprise tiers consume them; they never extend them in proprietary directions.
- Apache 2.0 is chosen over copyleft for compatibility with enterprise legal review and for the explicit patent grant.

## **Sovereign (commercial).** Enterprise features + TEE attestation (SGX/SEV), automated CMMC/FedRAMP reporting, CAC/PIV authentication. Targeted at defense contractors and government agencies.
<!-- task-id: 016-open-core-licensing-model-md/sovereign-commercial-enterprise-features-tee-attestation-sgx-sev-automated-cmmc-/production -->
<!-- adr: 016-open-core-licensing-model.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Adopt an open-core licensing model with three tiers:

1. **Community (Apache 2.0).** F1–F10 core features, CLI, local replay viewer, manifest validator, OCI-based model distribution. The runtime that must be auditable is the runtime that is open source.
2. **Enterprise (commercial).** Community features + Management UI, SIEM integration packs, RBAC integrations, support SLA. Targeted at mid-market enterprises and hospital systems.
3. **Sovereign (commercial).** Enterprise features + TEE attestation (SGX/SEV), automated CMMC/FedRAMP reporting, CAC/PIV authentication. Targeted at defense contractors and government agencies.

Properties:
- The community runtime must be sufficient to pass a security review on its own. Enterprise features add convenience and integration; they do not unlock previously-blocked compliance.
- All ledger and manifest formats are open. Enterprise tiers consume them; they never extend them in proprietary directions.
- Apache 2.0 is chosen over copyleft for compatibility with enterprise legal review and for the explicit patent grant.

## Write tests for: The community runtime must be sufficient to pass a security review on its own. Enterprise features add convenience and integration; they do not unlock previously-blocked compliance.
<!-- task-id: 016-open-core-licensing-model-md/the-community-runtime-must-be-sufficient-to-pass-a-security-review-on-its-own-en/test -->
<!-- adr: 016-open-core-licensing-model.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Adopt an open-core licensing model with three tiers:

1. **Community (Apache 2.0).** F1–F10 core features, CLI, local replay viewer, manifest validator, OCI-based model distribution. The runtime that must be auditable is the runtime that is open source.
2. **Enterprise (commercial).** Community features + Management UI, SIEM integration packs, RBAC integrations, support SLA. Targeted at mid-market enterprises and hospital systems.
3. **Sovereign (commercial).** Enterprise features + TEE attestation (SGX/SEV), automated CMMC/FedRAMP reporting, CAC/PIV authentication. Targeted at defense contractors and government agencies.

Properties:
- The community runtime must be sufficient to pass a security review on its own. Enterprise features add convenience and integration; they do not unlock previously-blocked compliance.
- All ledger and manifest formats are open. Enterprise tiers consume them; they never extend them in proprietary directions.
- Apache 2.0 is chosen over copyleft for compatibility with enterprise legal review and for the explicit patent grant.

## The community runtime must be sufficient to pass a security review on its own. Enterprise features add convenience and integration; they do not unlock previously-blocked compliance.
<!-- task-id: 016-open-core-licensing-model-md/the-community-runtime-must-be-sufficient-to-pass-a-security-review-on-its-own-en/production -->
<!-- adr: 016-open-core-licensing-model.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Adopt an open-core licensing model with three tiers:

1. **Community (Apache 2.0).** F1–F10 core features, CLI, local replay viewer, manifest validator, OCI-based model distribution. The runtime that must be auditable is the runtime that is open source.
2. **Enterprise (commercial).** Community features + Management UI, SIEM integration packs, RBAC integrations, support SLA. Targeted at mid-market enterprises and hospital systems.
3. **Sovereign (commercial).** Enterprise features + TEE attestation (SGX/SEV), automated CMMC/FedRAMP reporting, CAC/PIV authentication. Targeted at defense contractors and government agencies.

Properties:
- The community runtime must be sufficient to pass a security review on its own. Enterprise features add convenience and integration; they do not unlock previously-blocked compliance.
- All ledger and manifest formats are open. Enterprise tiers consume them; they never extend them in proprietary directions.
- Apache 2.0 is chosen over copyleft for compatibility with enterprise legal review and for the explicit patent grant.

## Write tests for: All ledger and manifest formats are open. Enterprise tiers consume them; they never extend them in proprietary directions.
<!-- task-id: 016-open-core-licensing-model-md/all-ledger-and-manifest-formats-are-open-enterprise-tiers-consume-them-they-neve/test -->
<!-- adr: 016-open-core-licensing-model.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Adopt an open-core licensing model with three tiers:

1. **Community (Apache 2.0).** F1–F10 core features, CLI, local replay viewer, manifest validator, OCI-based model distribution. The runtime that must be auditable is the runtime that is open source.
2. **Enterprise (commercial).** Community features + Management UI, SIEM integration packs, RBAC integrations, support SLA. Targeted at mid-market enterprises and hospital systems.
3. **Sovereign (commercial).** Enterprise features + TEE attestation (SGX/SEV), automated CMMC/FedRAMP reporting, CAC/PIV authentication. Targeted at defense contractors and government agencies.

Properties:
- The community runtime must be sufficient to pass a security review on its own. Enterprise features add convenience and integration; they do not unlock previously-blocked compliance.
- All ledger and manifest formats are open. Enterprise tiers consume them; they never extend them in proprietary directions.
- Apache 2.0 is chosen over copyleft for compatibility with enterprise legal review and for the explicit patent grant.

## All ledger and manifest formats are open. Enterprise tiers consume them; they never extend them in proprietary directions.
<!-- task-id: 016-open-core-licensing-model-md/all-ledger-and-manifest-formats-are-open-enterprise-tiers-consume-them-they-neve/production -->
<!-- adr: 016-open-core-licensing-model.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Adopt an open-core licensing model with three tiers:

1. **Community (Apache 2.0).** F1–F10 core features, CLI, local replay viewer, manifest validator, OCI-based model distribution. The runtime that must be auditable is the runtime that is open source.
2. **Enterprise (commercial).** Community features + Management UI, SIEM integration packs, RBAC integrations, support SLA. Targeted at mid-market enterprises and hospital systems.
3. **Sovereign (commercial).** Enterprise features + TEE attestation (SGX/SEV), automated CMMC/FedRAMP reporting, CAC/PIV authentication. Targeted at defense contractors and government agencies.

Properties:
- The community runtime must be sufficient to pass a security review on its own. Enterprise features add convenience and integration; they do not unlock previously-blocked compliance.
- All ledger and manifest formats are open. Enterprise tiers consume them; they never extend them in proprietary directions.
- Apache 2.0 is chosen over copyleft for compatibility with enterprise legal review and for the explicit patent grant.

## Write tests for: Apache 2.0 is chosen over copyleft for compatibility with enterprise legal review and for the explicit patent grant.
<!-- task-id: 016-open-core-licensing-model-md/apache-2-0-is-chosen-over-copyleft-for-compatibility-with-enterprise-legal-revie/test -->
<!-- adr: 016-open-core-licensing-model.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

Adopt an open-core licensing model with three tiers:

1. **Community (Apache 2.0).** F1–F10 core features, CLI, local replay viewer, manifest validator, OCI-based model distribution. The runtime that must be auditable is the runtime that is open source.
2. **Enterprise (commercial).** Community features + Management UI, SIEM integration packs, RBAC integrations, support SLA. Targeted at mid-market enterprises and hospital systems.
3. **Sovereign (commercial).** Enterprise features + TEE attestation (SGX/SEV), automated CMMC/FedRAMP reporting, CAC/PIV authentication. Targeted at defense contractors and government agencies.

Properties:
- The community runtime must be sufficient to pass a security review on its own. Enterprise features add convenience and integration; they do not unlock previously-blocked compliance.
- All ledger and manifest formats are open. Enterprise tiers consume them; they never extend them in proprietary directions.
- Apache 2.0 is chosen over copyleft for compatibility with enterprise legal review and for the explicit patent grant.

## Apache 2.0 is chosen over copyleft for compatibility with enterprise legal review and for the explicit patent grant.
<!-- task-id: 016-open-core-licensing-model-md/apache-2-0-is-chosen-over-copyleft-for-compatibility-with-enterprise-legal-revie/production -->
<!-- adr: 016-open-core-licensing-model.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

Adopt an open-core licensing model with three tiers:

1. **Community (Apache 2.0).** F1–F10 core features, CLI, local replay viewer, manifest validator, OCI-based model distribution. The runtime that must be auditable is the runtime that is open source.
2. **Enterprise (commercial).** Community features + Management UI, SIEM integration packs, RBAC integrations, support SLA. Targeted at mid-market enterprises and hospital systems.
3. **Sovereign (commercial).** Enterprise features + TEE attestation (SGX/SEV), automated CMMC/FedRAMP reporting, CAC/PIV authentication. Targeted at defense contractors and government agencies.

Properties:
- The community runtime must be sufficient to pass a security review on its own. Enterprise features add convenience and integration; they do not unlock previously-blocked compliance.
- All ledger and manifest formats are open. Enterprise tiers consume them; they never extend them in proprietary directions.
- Apache 2.0 is chosen over copyleft for compatibility with enterprise legal review and for the explicit patent grant.

## Write tests for: Implement decision: Aegis-Node Architectural Decision Records
<!-- task-id: readme-md/implement-decision-aegis-node-architectural-decision-records/test -->
<!-- adr: README.md -->
<!-- tdd: test -->
**Status:** pending
**Priority:** medium

## Implement decision: Aegis-Node Architectural Decision Records
<!-- task-id: readme-md/implement-decision-aegis-node-architectural-decision-records/production -->
<!-- adr: README.md -->
<!-- tdd: production -->
**Status:** pending
**Priority:** medium

## Milestone Index

- **Phase 0 — Foundations (target: v0.1.0, 2026-05-25)** [phase-0-foundations-target-v0-1-0-2026-05-25]
- **Phase 1a — Core Security Primitives (target: v0.5.0, 2026-07-20)** [phase-1a-core-security-primitives-target-v0-5-0-2026-07-20]
- **Phase 1b — Reasoning + Approval (target: v0.8.0, 2026-08-31)** [phase-1b-reasoning-approval-target-v0-8-0-2026-08-31]
- **Phase 1c — Tooling and Replay (target: v0.9.0, 2026-10-05)** [phase-1c-tooling-and-replay-target-v0-9-0-2026-10-05]
- **Phase 1 GA — Security Review Milestone (target: v1.0.0, 2026-11-02 — CMMC deadline)** [phase-1-ga-security-review-milestone-target-v1-0-0-2026-11-02-cmmc-deadline]
- **Phase 2 — Kubernetes Runtime (target: v2.0.0, 2027-01-25)** [phase-2-kubernetes-runtime-target-v2-0-0-2027-01-25]
- **Phase 3 — OpenShift Enterprise (target: v3.0.0, 2027-04-19)** [phase-3-openshift-enterprise-target-v3-0-0-2027-04-19]
- **Cross-Cutting / Always-On** [cross-cutting-always-on]
- **v0.1.0 — Foundations** [v0-1-0-foundations]
- **v0.5.0 — Core Security Primitives** [v0-5-0-core-security-primitives]
- **v0.8.0 — Reasoning + Approval** [v0-8-0-reasoning-approval]
- **v0.9.0 — Tooling and Replay** [v0-9-0-tooling-and-replay]
- **v1.0.0 — Phase 1 GA / Security Review Milestone** [v1-0-0-phase-1-ga-security-review-milestone]
- **v2.0.0 — Kubernetes Runtime** [v2-0-0-kubernetes-runtime]
- **v3.0.0 — OpenShift Enterprise Runtime** [v3-0-0-openshift-enterprise-runtime]

<!-- /ADR-GENERATED-TASKS -->
