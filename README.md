# Aegis-Node

> The only AI agent runtime designed to pass a zero-trust infrastructure review.

Every enterprise wants AI agents. Every enterprise security team blocks them. Aegis-Node is the agent runtime built to survive the security review — so organizations can finally say yes.

**Status:** Phase 1b complete — [**v0.8.0** *Reasoning + Approval*](https://github.com/tosin2013/aegis-node/releases/tag/v0.8.0) (pre-release, 2026-04-29). Builds on [v0.5.0](https://github.com/tosin2013/aegis-node/releases/tag/v0.5.0) with the F3 human approval gate (TTY / file / [localhost web UI](https://github.com/tosin2013/aegis-node/issues/35) / [mTLS+SPIFFE signed-API](https://github.com/tosin2013/aegis-node/issues/36)), F5 [pre-execution reasoning trajectory](https://github.com/tosin2013/aegis-node/issues/26), F6 [end-of-session signed network attestation](https://github.com/tosin2013/aegis-node/issues/37), F7 [time-bounded write_grants](https://github.com/tosin2013/aegis-node/issues/38) with [explicit-takes-precedence](docs/adrs/019-explicit-write-grant-takes-precedence.md). Phase 1c (v0.9.0) in progress — [MCP client adoption](docs/adrs/018-adopt-mcp-protocol-for-agent-tool-boundary.md) landed (closed-by-default `tools.mcp[]` + stdio transport + cross-language conformance + filesystem-server example); replay viewer, `aegis validate`, llama.cpp FFI, and OCI model pull still ahead. v1.0.0 GA targets the [CMMC 2.0 deadline 2026-11-02](RELEASE_PLAN.md).

## What it is

Aegis-Node is structured around the ten questions a zero-trust security team asks before approving an AI agent for production. Each question maps to a non-negotiable feature (F1–F10):

| Security Review Question | Feature | Shipping in |
|---|---|---|
| What identity is the agent running as? | F1 Workload Identity | ✅ v0.5.0 |
| What tools can it access? | F2 Permission Manifest | ✅ v0.5.0 |
| Who approved the tool action? | F3 Human Approval Gate | ✅ v0.8.0 |
| What data did it touch? | F4 Access Log | ✅ v0.5.0 |
| Why did it act? | F5 Reasoning Trajectory | ✅ v0.8.0 |
| Can it exfiltrate data? | F6 Network-Deny-by-Default | ✅ v0.5.0 + signed attestation v0.8.0 |
| Can it mutate production? | F7 Read-Only + Explicit Write Grants | ✅ v0.5.0 + time-bounded v0.8.0 |
| Can we replay what happened? | F8 Trajectory Replay | v0.9.0 |
| Can logs be altered? | F9 Hash-Chained Ledger | ✅ v0.5.0 |
| Can policies be reviewed before runtime? | F10 Policy-as-Code Validation | v0.9.0 |

The F-feature table maps directly to the [OWASP Top 10 for Agentic Applications 2026](https://genai.owasp.org/resource/owasp-top-10-for-agentic-applications-for-2026/) — F1 counters identity/privilege abuse, F2+ADR-024 counters tool misuse and agentic supply chain, F3 counters human-agent trust exploitation, F6 counters goal hijack via exfiltration, and F9 covers memory poisoning / cascading failures via tamper-evident audit. See [examples/](examples/) for runnable mappings.

## Quick Start

5-minute walkthrough — see [examples/](examples/) for six graduated, fork-friendly samples (hello-world → MCP research → customer support → coding agent → egress audit → finance/SQL).

```bash
mise install                                              # one-time toolchain
cargo install --locked --path crates/cli --features llama          # one-time: aegis on PATH + --prompt support
aegis identity init --trust-domain aegis-node.local       # one-time CA
cd examples/01-hello-world
bash setup.sh
cd /tmp/aegis-example-01
aegis run --manifest manifest.yaml --model model.gguf \
    --workload hello-world --instance inst-001 \
    --prompt "$(cat prompt.txt)"
cat output/greeting.txt          # the agent's work product
aegis verify ledger-*.jsonl      # the audit trail
```

You'll also need `oras`, `jq`, and `git` on PATH (see [examples/README.md](examples/README.md#extra-binaries-on-path) for the full install matrix including per-example MCP server installs).

Continue through `examples/02-mcp-research-assistant/` … `06-mcp-finance-sqlite/`.

## What works today

```bash
# One-time CA setup
aegis identity init --trust-domain aegis-node.local

# Run an agent under enforcement (manifest gates every I/O)
aegis run \
  --manifest schemas/manifest/v1/examples/read-only-research.manifest.yaml \
  --model /path/to/model.gguf \
  --workload research --instance inst-001 \
  --prompt "summarize the docs in /data"

# Verify the produced ledger end-to-end (chain integrity + summary)
aegis verify ledger-session-*.jsonl
```

Every tool call routes through: **identity rebind → policy decision → gate dispatch → access entry / violation entry**. Tampering the model file mid-session triggers an `IdentityRebind` violation and a halt. The Go validator (`pkg/manifest`) and Rust enforcer (`aegis_policy::Policy`) agree on every example manifest's allowed/denied operations — guarded by the [Conformance workflow](.github/workflows/conformance.yml) on every PR.

## Documentation

- **[Architectural Decision Records](docs/adrs/)** — 24 ADRs covering the security primitives, runtime architecture, supply chain, dev environment, agent ↔ tool protocol (MCP) plus the second-layer MCP arg pre-validation, write-grant precedence, the recorded demo program, HuggingFace-as-upstream model distribution, trust-boundary format agnosticism, and LiteRT-LM as a second inference backend.
- **[Compatibility Charter](docs/COMPATIBILITY_CHARTER.md)** — what the project promises not to break across versions (manifest, ledger, IPC).
- **[Supply Chain Verification](docs/SUPPLY_CHAIN.md)** — `cosign verify` / `oras pull` flow for the signed devbox image and model artifacts.
- **[Model Mirroring](docs/MODEL_MIRRORING.md)** — operator workflow for publishing a HuggingFace model to your internal OCI registry, signed with your org's cosign trust root (per ADR-013 + ADR-021).
- **[CONTRIBUTING.md](CONTRIBUTING.md)** — DCO sign-off, dev workflow, ADR process.
- **[RELEASE_PLAN.md](RELEASE_PLAN.md)** — 7 baseline milestones (v0.1.0 → v3.0.0). v1.0.0 anchors to the U.S. CMMC 2.0 deadline (2026-11-02).
- **[TODO.md](TODO.md)** — phase-grouped task plan plus paired test+production tasks decomposed from each ADR.

## Repository layout

```
.
├── proto/                  # aegis.proto — gRPC IPC contract (frozen at aegis.v1)
├── schemas/
│   ├── manifest/           # Permission Manifest JSON Schema (schemaVersion: "1")
│   └── ledger/             # Trajectory Ledger + Access Log JSON-LD @context (v1)
├── crates/                 # Rust workspace
│   ├── identity/           # F1: SPIFFE local CA + X.509-SVID issuance + cdylib FFI
│   ├── ledger-writer/      # F9: append-only hash-chained writer + verifier
│   ├── access-log/         # F4: typed event emitter
│   ├── policy/             # F2: closed-by-default decision engine + violation emit
│   ├── network-gate/       # F6: AegisTcpStream::connect (policy-checked std::net wrapper)
│   ├── filesystem-gate/    # F2: AegisFile-style policy-checked std::fs wrappers
│   ├── approval-gate/      # F3: ApprovalChannel trait + TTY/file/web/mTLS channels
│   ├── mcp-client/         # F2-MCP: McpClient trait + stdio JSON-RPC transport (ADR-018)
│   ├── inference-engine/   # F0: Session boot/shutdown + per-tool-call mediator
│   └── cli/                # `aegis` binary: identity / verify / run subcommands
├── pkg/
│   ├── manifest/           # F2 Go validator + Decide engine (mirrors Rust semantics)
│   ├── identity/ffi/       # cgo wrapper for crates/identity
│   └── version/            # version stamping
├── cmd/                    # Go binaries (aegis CLI alt-entry, operator scaffold)
├── tests/
│   ├── conformance/        # Cross-language Go ↔ Rust agreement battery (cases.json)
│   └── runtime/            # End-to-end golden-ledger fixture (manifest + script + golden)
├── .devcontainer/          # Canonical dev environment (per ADR-017)
├── .github/workflows/      # CI: rust, go, schemas, conformance, devbox
├── docs/adrs/              # 21 Architectural Decision Records
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

Three paths, same pinned tool versions (per [ADR-017](docs/adrs/017-local-development-environment-devcontainer-mise.md)). The Docker path is the fastest if you just want to try the examples:

### Docker (fastest — no toolchain install)

The signed devbox image bundles Rust, Go, `buf`, `ajv`, `cosign`, `oras`, `jq`, `golangci-lint`, `protoc`, and `node`. Pull it and bind-mount your checkout:

```bash
git clone https://github.com/tosin2013/aegis-node.git && cd aegis-node
docker run --rm -it \
    -v "$PWD:/workspaces/aegis-node" \
    -w /workspaces/aegis-node \
    ghcr.io/tosin2013/aegis-node-devbox:latest \
    bash

# inside the container:
cargo install --locked --path crates/cli --features llama
aegis identity init --trust-domain aegis-node.local
cd examples/01-hello-world && bash setup.sh
# then follow the example's README from there
```

### Devcontainer (canonical — VS Code)

Open the repo in VS Code and choose *"Reopen in Container"*. Same image as the Docker path; VS Code wires up the Rust + Go extensions automatically.

### Native via mise (no Docker)

Install [`mise`](https://mise.jdx.dev/) (toolchain version manager), then pin everything via `mise.toml`:

```bash
curl https://mise.run | sh                                # install mise itself (one-time)
eval "$(~/.local/bin/mise activate bash)"                 # activate in current shell (use 'zsh' if on zsh)
echo 'eval "$(~/.local/bin/mise activate bash)"' >> ~/.bashrc   # persist for new shells

cd /path/to/aegis-node
mise install                                              # installs Rust 1.85, Go 1.23, cosign, node per mise.toml
cargo install --locked --path crates/cli --features llama          # puts aegis on PATH (~/.cargo/bin); enables --prompt
aegis identity init --trust-domain aegis-node.local       # one-time CA

# you'll also need oras + jq + git on PATH; see examples/README.md for the full install matrix
make build          # builds Go + Rust (target/debug/ — for tests; the cargo install above is what example runs use)
make test           # runs all tests
make lint           # cargo fmt/clippy + go vet + golangci-lint
```

## Architecture in one sentence

A Rust inference engine (memory-safe, GC-free, llama.cpp-bound) plus a Go control plane (Kubernetes-native, OPA/SPIFFE-fluent), sharing a single Permission Manifest format and a hash-chained Trajectory Ledger across local laptop, Kubernetes, and OpenShift deployments.

See [ADR-002](docs/adrs/002-split-language-architecture-rust-and-go.md) for the rationale.

## License

[Apache 2.0](LICENSE) for the community runtime. Enterprise and Sovereign tiers (Management UI, SIEM packs, TEE attestation, automated CMMC/FedRAMP reporting) are commercial — see [ADR-016](docs/adrs/016-open-core-licensing-model.md).

The community-tier runtime alone is sufficient to pass a security review; commercial tiers add convenience and integration, not previously-blocked compliance.
