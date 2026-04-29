# Aegis-Node Architectural Decision Records

This directory contains the architectural decision records (ADRs) generated from the Aegis-Node PRD v3.0. Each record documents one significant architectural decision, the context behind it, and its consequences.

The decisions are anchored in the Architecture Principles (PRD §7) and the ten-question security review checklist (PRD §2).

## Index

| ADR | Title | Maps to PRD |
|---|---|---|
| [001](001-zero-trust-security-review-as-product-specification.md) | Zero-Trust Security Review as Product Specification | §1, §2, §7 |
| [002](002-split-language-architecture-rust-and-go.md) | Split-Language Architecture: Rust + Go | §7 (#4) |
| [003](003-cryptographic-workload-identity-spiffe-spire.md) | Cryptographic Workload Identity (SPIFFE/SPIRE) | §4 F1 |
| [004](004-declarative-yaml-permission-manifest.md) | Declarative YAML Permission Manifest | §4 F2 |
| [005](005-human-approval-gate-for-sensitive-actions.md) | Human Approval Gate for Sensitive Actions | §4 F3 |
| [006](006-structured-access-log-jsonld-siem-format.md) | Structured Access Log (JSON-LD / SIEM) | §4 F4 |
| [007](007-pre-execution-reasoning-trajectory.md) | Reasoning + Action Trajectory Pre-Execution | §4 F5 |
| [008](008-network-deny-by-default-at-runtime-level.md) | Network-Deny-by-Default at Runtime Level | §4 F6 |
| [009](009-read-only-default-with-explicit-write-grants.md) | Read-Only Default + Explicit Write Grants | §4 F7 |
| [010](010-deterministic-trajectory-replay-offline-viewer.md) | Deterministic Trajectory Replay (Offline) | §4 F8 |
| [011](011-hash-chained-tamper-evident-ledger.md) | Hash-Chained Tamper-Evident Ledger | §4 F9 |
| [012](012-policy-as-code-validation.md) | Policy-as-Code Validation in CI/CD | §4 F10 |
| [013](013-oci-artifacts-for-model-distribution.md) | OCI Artifacts for Model Distribution | §6.1 |
| [014](014-cpu-first-gguf-inference-via-llama-cpp.md) | CPU-First GGUF Inference via llama.cpp | §5 Phase 1 |
| [015](015-three-phase-deployment-roadmap.md) | Three-Phase Deployment Roadmap | §5 |
| [016](016-open-core-licensing-model.md) | Open-Core Licensing Model | §9.1 |
| [017](017-local-development-environment-devcontainer-mise.md) | Local Development Environment: Devcontainer + mise | §7 (#3, #4), §6.1 |
| [018](018-adopt-mcp-protocol-for-agent-tool-boundary.md) | Adopt the Model Context Protocol (MCP) as the Agent-to-Tool Boundary | §4 F2, §4 F5 |
| [019](019-explicit-write-grant-takes-precedence.md) | Explicit Write Grant Takes Precedence Over Broad Path Coverage | §4 F7 |
| [020](020-recorded-demo-program.md) | Recorded Demo Program — VHS Tapes Driven by Real CPU-Bound Models | §1, §5 Phase 1 |

## Decision Coverage Matrix

Every feature ID in the PRD's security-review checklist has a dedicated ADR:

| Feature | Question | ADR |
|---|---|---|
| F1 | What identity is the agent running as? | ADR-003 |
| F2 | What tools can it access? | ADR-004 |
| F3 | Who approved the tool action? | ADR-005 |
| F4 | What data did it touch? | ADR-006 |
| F5 | Why did it act? | ADR-007 |
| F6 | Can it exfiltrate data? | ADR-008 |
| F7 | Can it mutate production? | ADR-009 |
| F8 | Can we replay what happened? | ADR-010 |
| F9 | Can logs be altered? | ADR-011 |
| F10 | Can policies be reviewed before runtime? | ADR-012 |
