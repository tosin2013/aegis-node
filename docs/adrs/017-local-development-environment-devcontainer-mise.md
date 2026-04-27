# 17. Local Development Environment: Devcontainer (Canonical) + mise (Native Fallback)

**Status:** Accepted
**Date:** 2026-04-27
**Domain:** Developer Experience / Supply Chain

## Context

Aegis-Node is split across two language toolchains (Rust + Go, per ADR-002), plus a growing set of supporting tools — `buf` for protobuf linting, `ajv` for JSON Schema validation, `cosign` for signature verification (ADR-013), `oras` for OCI artifact pull (ADR-013), `golangci-lint`, and `protoc`. Contributors range from open-source developers on personal Mac/Linux laptops to enterprise security reviewers running on locked-down machines inside an air-gapped network.

A security product cannot afford "works on my machine" drift between contributor environments and CI. Inconsistent tool versions undermine the same reproducibility story the product itself sells. Equally, the dev environment cannot require public-internet installs at evaluation time, or it disqualifies itself from the air-gap reviewer scenario the product is built for.

Two failure modes we want to avoid:
- Each contributor installs their own Rust/Go/tool versions, drifts from CI, ships subtle differences.
- We force a heavyweight environment (e.g., Nix) on every contributor and lose adoption velocity.

## Decision

Adopt a two-track local development environment with a single source of truth for tool versions:

1. **Canonical: Devcontainer.** A pinned OCI image (e.g., `ghcr.io/tosin2013/aegis-node-devbox:<digest>`) bundling the full toolchain — Rust (with `rustfmt`, `clippy`), Go, `buf`, `ajv`, `cosign`, `oras`, `golangci-lint`, `protoc`. The repo includes `.devcontainer/devcontainer.json` and (where needed) a `Dockerfile`. Editors that support the Devcontainer spec (VS Code, JetBrains Gateway, GitHub Codespaces) provide one-click "Reopen in Container."
2. **Fallback: `mise`.** A `mise.toml` at the repo root pins the same Rust/Go/tool versions for contributors who cannot run Docker/Podman locally. `mise install` reproduces the canonical versions natively.
3. **CI uses the same image** (or the same `mise.toml`) — there is one source of truth for "the tool versions Aegis-Node is built and tested against."
4. **The devbox image is itself a signed OCI artifact** (Cosign-signed, per ADR-013). CI verifies the signature before pulling. Enterprises mirror it to their internal registry (Harbor/Artifactory/ECR) using the same workflow they already use for production container images.
5. **Bumps are PRs.** Tool-version bumps (Rust nightly date, Go minor, `buf`, etc.) are reviewed PRs that update the Dockerfile, `mise.toml`, and `.devcontainer/devcontainer.json` together. CI rebuilds and re-signs the devbox image on merge.

## Consequences

**Positive:**
- One-click contributor onboarding via Devcontainer — no host pollution, no `brew install` chain.
- Air-gapped contributors and reviewers can pull the signed image from their internal registry; the same tool versions developers use.
- The devbox is a small but real instance of Aegis-Node's own supply-chain thesis: a signed OCI artifact, pulled from a controlled registry, verified before use. Reviewers see the pattern in action before they evaluate the product.
- `mise` fallback supports contributors whose security policies forbid running container engines locally.
- Tool drift between contributors and CI is structurally prevented because CI uses the same definition.

**Negative:**
- Devbox image must be rebuilt and re-signed on every Rust/Go/tool version bump. Maintenance owner must be assigned.
- Two paths to test (devcontainer + native via `mise`); CI should validate both.
- `mise` is less ubiquitous than `asdf`; teams already invested in `asdf` can consume a generated `.tool-versions` file as a courtesy export.
- Devcontainer requires Docker/Podman, which some restricted enterprise laptops still cannot run; the `mise` path exists exactly for that reason but does require host-level installs.

## Domain Considerations

The Devcontainer specification is now standardized (`containers.dev`) and supported across major editors and cloud-IDE providers. `mise` (the active fork of `asdf`-style tool management) supports Rust toolchains and Cargo install-from-source, eliminating the need for a separate `rustup`-driven path. Both choices are conventional in cloud-native projects of this kind, so security reviewers and contributors who recognize one will recognize the other.

## Implementation Plan

1. Author `.devcontainer/devcontainer.json` and a thin `Dockerfile` based on a pinned upstream Rust+Go base image; install the supporting tools (`buf`, `ajv`, `cosign`, `oras`, `golangci-lint`, `protoc`).
2. Author `mise.toml` at the repo root pinning the same Rust/Go and tool versions.
3. Configure CI to either (a) run jobs inside the signed devbox image, or (b) install tools via `mise` matching the same versions. Pick (a) once the devbox image is published.
4. Set up Cosign keypair and signing in CI (`cosign sign` on push to `main`); document the public key and the verification command in `CONTRIBUTING.md`.
5. Add a CI workflow that rebuilds and re-signs the devbox image when `.devcontainer/`, `Dockerfile`, or `mise.toml` change.
6. Document the contributor workflow: VS Code "Reopen in Container" path; native path via `mise install`; air-gap path via `oras pull` + `cosign verify` from internal registry.

## Related PRD Sections

- §7 Architecture Principles (#3: Offline by default; #4: Split-Language Pragmatism)
- §6.1 The Model Distribution and Update Strategy (the devbox follows the same OCI + Cosign pattern)

## Domain References

- Devcontainer Specification (https://containers.dev/)
- `mise` tool-version manager (https://mise.jdx.dev/)
- Cosign / Sigstore
- ADR-002 (Split-Language Architecture)
- ADR-013 (OCI Artifacts for Model Distribution)
