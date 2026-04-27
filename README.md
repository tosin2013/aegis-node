# Aegis-Node

> The only AI agent runtime designed to pass a zero-trust infrastructure review.

Every enterprise wants AI agents. Every enterprise security team blocks them. Aegis-Node is the agent runtime built to survive the security review — so organizations can finally say yes.

**Status:** Phase 0 — Foundations. Substrate scaffolded; schemas, IPC contract, and security primitives in flight. Not yet usable.

## What it is

Aegis-Node is structured around the ten questions a zero-trust security team asks before approving an AI agent for production. Each question maps to a non-negotiable feature (F1–F10):

| Security Review Question | Feature |
|---|---|
| What identity is the agent running as? | F1 Workload Identity |
| What tools can it access? | F2 Permission Manifest |
| Who approved the tool action? | F3 Human Approval Gate |
| What data did it touch? | F4 Access Log |
| Why did it act? | F5 Reasoning Trajectory |
| Can it exfiltrate data? | F6 Network-Deny-by-Default |
| Can it mutate production? | F7 Read-Only + Explicit Write Grants |
| Can we replay what happened? | F8 Trajectory Replay |
| Can logs be altered? | F9 Hash-Chained Ledger |
| Can policies be reviewed before runtime? | F10 Policy-as-Code Validation |

## Documentation

- **[Architectural Decision Records](docs/adrs/)** — 17 ADRs covering the security primitives, runtime architecture, supply chain, and dev environment.
- **[Compatibility Charter](docs/COMPATIBILITY_CHARTER.md)** — what the project promises not to break across versions (manifest, ledger, IPC).
- **[Supply Chain Verification](docs/SUPPLY_CHAIN.md)** — `cosign verify` / `oras pull` flow for the signed devbox image and (later) model artifacts.
- **[CONTRIBUTING.md](CONTRIBUTING.md)** — DCO sign-off, dev workflow, ADR process.
- **[RELEASE_PLAN.md](RELEASE_PLAN.md)** — 7 baseline milestones (v0.1.0 → v3.0.0). v1.0.0 anchors to the U.S. CMMC 2.0 deadline (2026-11-02).
- **[TODO.md](TODO.md)** — phase-grouped task plan plus paired test+production tasks decomposed from each ADR.

## Repository layout

```
.
├── proto/                  # aegis.proto (gRPC IPC contract — Phase 0 schema task)
├── schemas/
│   ├── manifest/           # Permission Manifest JSON Schema
│   └── ledger/             # Trajectory Ledger + Access Log JSON-LD @context
├── crates/                 # Rust workspace (inference engine, network gate, ledger, identity)
├── cmd/                    # Go binaries (aegis CLI, operator)
├── pkg/                    # Go libraries
├── .devcontainer/          # Canonical dev environment (per ADR-017)
├── .github/workflows/      # CI: rust, go, schemas, conformance, devbox
├── docs/adrs/              # Architectural Decision Records
├── Cargo.toml              # Rust workspace
├── go.mod                  # Go module
├── mise.toml               # Native-install tool versions (devcontainer fallback)
├── Makefile                # Cross-language orchestration
├── LICENSE                 # Apache-2.0
├── NOTICE
├── RELEASE_PLAN.md
└── TODO.md
```

## Local development

Two paths, same pinned tool versions (per [ADR-017](docs/adrs/017-local-development-environment-devcontainer-mise.md)):

**Devcontainer (canonical):** open the repo in VS Code and choose *"Reopen in Container"*. The image bundles Rust, Go, `buf`, `ajv`, `cosign`, `oras`, `golangci-lint`, and `protoc`.

**Native via mise:**

```bash
mise install        # installs the versions pinned in mise.toml
make build          # builds Go + Rust
make test           # runs all tests
make lint           # cargo fmt/clippy + go vet + golangci-lint
```

## Architecture in one sentence

A Rust inference engine (memory-safe, GC-free, llama.cpp-bound) plus a Go control plane (Kubernetes-native, OPA/SPIFFE-fluent), sharing a single Permission Manifest format and a hash-chained Trajectory Ledger across local laptop, Kubernetes, and OpenShift deployments.

See [ADR-002](docs/adrs/002-split-language-architecture-rust-and-go.md) for the rationale.

## License

[Apache 2.0](LICENSE) for the community runtime. Enterprise and Sovereign tiers (Management UI, SIEM packs, TEE attestation, automated CMMC/FedRAMP reporting) are commercial — see [ADR-016](docs/adrs/016-open-core-licensing-model.md).

The community-tier runtime alone is sufficient to pass a security review; commercial tiers add convenience and integration, not previously-blocked compliance.
