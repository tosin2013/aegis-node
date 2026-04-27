# Contributing to Aegis-Node

Welcome. Aegis-Node is built to pass a zero-trust infrastructure security review (see [ADR-001](docs/adrs/001-zero-trust-security-review-as-product-specification.md)). That goal shapes how we accept contributions: the runtime is read by enterprise security teams, so the bar is correctness, clarity, and traceability over feature breadth.

## License and sign-off

The community runtime is [Apache 2.0](LICENSE). By contributing, you agree your work is licensed under Apache 2.0.

We use the [Developer Certificate of Origin (DCO)](https://developercertificate.org/), not a CLA. Every commit must carry a `Signed-off-by:` trailer:

```bash
git commit -s -m "feat: ..."
```

That trailer is your assertion that you have the right to submit the contribution. Linux, Docker, Kubernetes, and many others use the same model — the `-s` flag is the only friction.

DCO enforcement on PRs is on the roadmap; until it's in CI, maintainers verify sign-off at review.

(Why DCO over CLA: lower friction, well-understood by enterprise legal teams, and sufficient for Apache 2.0. Commercial-tier contributions [Enterprise/Sovereign] follow a separate flow.)

## Development environment

Two equivalent paths, same pinned tool versions ([ADR-017](docs/adrs/017-local-development-environment-devcontainer-mise.md)):

- **Devcontainer**: open the repo in VS Code → *Reopen in Container*.
- **Native via `mise`**:
  ```bash
  mise install
  make build test lint
  ```

The CI image (`ghcr.io/tosin2013/aegis-node-devbox`) is Cosign-signed; see [docs/SUPPLY_CHAIN.md](docs/SUPPLY_CHAIN.md) for the verification flow.

## Feature contributions

Every new feature must either:
1. Map to one of the F1–F10 security-review questions ([ADR-001](docs/adrs/001-zero-trust-security-review-as-product-specification.md)), **or**
2. Be marked explicitly as post-MVP and justified.

A feature that does neither will be asked to defer until v2 even if the code is great.

## Schema, proto, and ledger format changes

These are governed by the [Compatibility Charter](docs/COMPATIBILITY_CHARTER.md). Read it before proposing changes to:
- `proto/aegis/v1/*.proto`
- `schemas/manifest/*/manifest.schema.json`
- `schemas/ledger/*/context.jsonld`

`buf breaking` checks gate the proto. Manifest schema and ledger context changes require an updated example or a new test fixture.

## Tests

Per [ADR-002](docs/adrs/002-split-language-architecture-rust-and-go.md), we run a cross-language conformance suite: any manifest accepted by the Go validator must be enforced consistently by the Rust runtime, and vice versa. New manifest semantics require fixtures on both sides.

Run locally:
```bash
make test          # Go + Rust
make lint          # cargo fmt/clippy + go vet + golangci-lint
```

## ADRs

Architectural decisions live in [docs/adrs/](docs/adrs/). When introducing a non-obvious design choice:
1. Copy an existing ADR's structure (Status, Date, Context, Decision, Consequences, Implementation Plan).
2. Number sequentially.
3. Update [docs/adrs/README.md](docs/adrs/README.md) index.

Aim for the same length and concreteness as existing ADRs — terse is fine, vague is not.

## Commit messages

Conventional commits (`feat:`, `fix:`, `docs:`, `chore:`, etc.) — short subject, body explains the *why*. Trailing `Signed-off-by:` is required (`git commit -s` adds it). See `git log` for examples.

## PR review

CI must be green. PRs that touch the policy enforcement path (Rust runtime, Go validator, ledger writer) get an extra security-conscious reviewer.

## Reporting security issues

Do **not** open a public issue for security vulnerabilities. Email the maintainers privately (contact in the repo's GitHub `SECURITY.md` once it lands). Until then, use a GitHub private vulnerability report.
