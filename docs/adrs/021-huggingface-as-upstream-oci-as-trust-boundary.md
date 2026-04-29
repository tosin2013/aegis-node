# 21. HuggingFace as Canonical Upstream; OCI + cosign as Trust Boundary

**Status:** Accepted
**Date:** 2026-04-29
**Domain:** Supply chain / model distribution (extends [ADR-013](013-oci-artifacts-for-model-distribution.md), supports [ADR-020](020-recorded-demo-program.md), reuses [ADR-017](017-local-development-environment-devcontainer-mise.md))

## Context

[ADR-013](013-oci-artifacts-for-model-distribution.md) (OCI Artifacts for
Model Distribution) requires that what Aegis-Node signs and ships is OCI
artifacts, but it took no position on where models *come from*.
[ADR-020](020-recorded-demo-program.md) (Recorded Demo Program) pinned
**Qwen2.5-1.5B-Instruct Q4_K_M** but did not specify *where* the artifact
lives. [OCI-C](https://github.com/tosin2013/aegis-node/issues/68)
(operator workflow doc) explicitly contemplates HuggingFace as the upstream
source — operators downloading via `huggingface-cli` and re-signing for
their internal registry — but the project itself has not declared an
upstream-source policy, and zero HuggingFace references existed in the
codebase before this ADR.

[PR #79](https://github.com/tosin2013/aegis-node/pull/79) (the OCI-A
`aegis pull` subcommand) surfaced a sharper version of the gap. The CI
real-image test attempted to round-trip the only signed artifact we
publish — `ghcr.io/tosin2013/aegis-node-devbox` — through `pull::pull`,
and it failed. Diagnosis: the devbox is a multi-layer Docker container
image with `application/vnd.docker.*` media types; `oras pull` correctly
skips Docker-format layers without flags `pull::pull` deliberately omits.
Container images and OCI artifacts are not interchangeable for a
single-blob model pull, and trying to pretend they are produces silent
"no files in staging dir" failures. We need a real OCI *artifact* — not
a re-purposed container image — to validate the supply chain end-to-end.

April 2026 research on HuggingFace's distribution surface confirmed three
load-bearing facts:

1. **HuggingFace publishes no native OCI registry endpoint.** `oras pull`
   against a HuggingFace URL does not work; only Docker bridges HF→OCI
   on demand. There is no `registry.huggingface.co`.
2. **HuggingFace publishes no Sigstore signatures.** Trust today rests
   on TLS + commit-hash pinning + LFS SHA-256. The Red Hat / Sigstore
   "Model Authenticity and Transparency" initiative is aspirational, not
   shipped.
3. **The Qwen2.5-1.5B-Instruct GGUF Q4_K_M file** exists at
   [`Qwen/Qwen2.5-1.5B-Instruct-GGUF`](https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF)
   (official Qwen team upload, 1.12 GB, Apache 2.0 — redistribution
   clean).

Without a declared upstream-source policy, every demo run, every operator
setup, and every air-gap reviewer faces the same unanswered question:
*where does the model bytes come from before we sign them?* This ADR
closes that question.

## Decision

1. **HuggingFace is the documented canonical upstream for community
   models.** Aegis-Node's published mirror, the demo program, and the
   operator workflow doc (OCI-C) all reference HF by name as the
   supported source. Vendor-direct (e.g. Microsoft for Phi) and
   self-trained are also acceptable upstreams.

2. **The Aegis-Node runtime never reaches HuggingFace.** `aegis pull`
   (per ADR-013) consumes only signed OCI artifacts; the runtime trust
   boundary stays at OCI + cosign. There is no native
   `aegis pull hf:Qwen/...` form, and there will not be — adding it
   would split the runtime trust model across two unrelated signing
   systems.

3. **A mirror pipeline bridges HF → OCI outside the runtime.** The
   Aegis-Node project ships
   [`.github/workflows/models-publish.yml`](../../.github/workflows/models-publish.yml),
   a manual-dispatch GitHub Actions workflow that downloads from HF
   (commit-pinned), verifies the LFS SHA-256, pushes to GHCR via
   `oras push`, and signs via Sigstore keyless tied to that workflow's
   identity.

4. **The Aegis-Node project publishes Qwen2.5-1.5B-Instruct Q4_K_M** at
   `ghcr.io/tosin2013/aegis-node-models/qwen2.5-1.5b-instruct-q4_k_m`,
   signed via `models-publish.yml`. This becomes the canonical
   demo-program model. Operators verify the project's signing identity
   via `cosign verify`; air-gapped operators mirror once on a connected
   box and consume by digest internally.

5. **Operators replicate the same pipeline against their internal
   registry + cosign trust root.** OCI-C
   ([#68](https://github.com/tosin2013/aegis-node/issues/68)) documents
   the operator workflow with `models-publish.yml` as the reference
   implementation. Operators control license review, scanning, and
   signing identity for their org.

6. **License scope is limited.** Only Apache-2.0 / MIT / similarly-
   permissive models go through the project's `models-publish.yml`
   without legal review. Llama-licensed models and other restrictively-
   licensed artifacts are out of scope for the project's published
   mirror — operators sign their own.

## Why these decisions

- **Why HF can't be the runtime trust root.** No OCI registry endpoint
  → `oras pull` can't reach HF natively. No Sigstore signatures →
  `cosign verify` has nothing to verify. F1's SVID extension binds a
  SHA-256 model digest from a stable, signed source; HF's commit-hash
  pinning + LFS SHA-256 are integrity hints but not signed
  attestations. Air-gap reviewers can't reach HF; relying on HF at
  runtime breaks that audience entirely.
- **Why a separate mirror pipeline (and not a Rust subcommand).** The
  mirror is operator-facing tooling, not runtime tooling — putting it
  in the Rust binary expands the supply-chain surface (a new HF SDK
  dependency, or hand-rolled HTTP) for a one-time-per-model operation.
  A scripted GHA workflow + Makefile target keeps the runtime clean
  and reuses the existing Sigstore keyless flow ADR-017 already
  established for the devbox image.
- **Why the project ships its own published mirror at all.** A first-
  time evaluator reproducing the demo program shouldn't need to run a
  multi-step publishing pipeline before any demo works. Friction kills
  adoption. Shipping one canonical demo model (Qwen2.5-1.5B Q4_K_M,
  Apache 2.0) makes demos work out of the box; orgs in production
  still publish their own per their license/scan policy.
- **Why we rejected Docker's HF→OCI bridge as a transparent transport.**
  Docker's bridge converts HF repos into Docker container images
  (multi-layer with `application/vnd.docker.*` media types), not OCI
  artifacts. PR #79 surfaced exactly this incompatibility at runtime.
  Re-publishing as a single-blob OCI artifact under our own signing
  identity keeps `pull::pull` clean and gives us full provenance over
  the bytes.

## Consequences

### Positive

- ADR-013's "OCI is the trust boundary" stays intact. The runtime
  never adopts HF as a transport, never trusts HF's TLS / commit-hash
  story for boot-time integrity, never breaks for air-gap reviewers.
- The demo program (ADR-020) has a concrete, pullable artifact that
  doesn't depend on operators bringing their own model.
- Operators see a documented end-to-end path from HF upstream to
  internal-registry signed-OCI consumption — same shape as the
  Aegis-Node project's own pipeline.
- Demo recordings (ADR-020) remain reproducible: the model bytes are
  pinned by SHA-256, the cosign signature is verifiable independently
  of the project, the GHA workflow that produced both is auditable
  (workflow source + run logs + Sigstore Rekor entry).
- The runtime stays free of HF SDK / HTTP code. The new supply-chain
  surface is one GHA workflow that runs only on manual dispatch — no
  scheduled or implicit fetches.

### Negative

- The Aegis-Node project takes on responsibility for keeping its
  mirrored model artifacts current with upstream HF releases.
  Practically: an additional `models-publish.yml` run when bumping
  Qwen versions; the published artifact is one we maintain.
- ~1 GB of GHCR storage per pinned model. Acceptable given GHCR's
  free-tier quotas for public packages; reviewable as model count
  grows.
- HuggingFace wire/auth changes propagate into our publishing
  pipeline. Mitigation: the workflow is `workflow_dispatch` only — no
  scheduled runs, no implicit dependency at runtime.
- Operators in restrictive air-gap environments still need a one-time
  internet-connected mirror step. ADR-017's air-gap reviewer flow
  already documents this; the model pipeline reuses the same pattern.

## Implementation plan

1. **`.github/workflows/models-publish.yml`** — manual-dispatch
   workflow: HF download (commit-pinned) → LFS SHA-256 verify →
   `oras push` → cosign keyless sign → print resulting `<ref>@sha256`.
2. **Update [ADR-013](013-oci-artifacts-for-model-distribution.md)** —
   append a section recognizing this ADR's upstream-source policy.
3. **Update [ADR-020](020-recorded-demo-program.md)** — pin the
   resulting OCI URI in the demo program once the workflow's first run
   publishes it.
4. **Expand OCI-C ([#68](https://github.com/tosin2013/aegis-node/issues/68))
   acceptance criteria** — operator workflow doc references
   `models-publish.yml` as the reference implementation.
5. **Update [`docs/SUPPLY_CHAIN.md`](../SUPPLY_CHAIN.md)** —
   "Mirroring an upstream model" section + flip the "Model OCI
   artifacts" row from "🚧 Phase 1c" to "✅ live" once the workflow
   ships its first artifact.
6. **First publish run** — `Qwen/Qwen2.5-1.5B-Instruct-GGUF` ·
   `qwen2.5-1.5b-instruct-q4_k_m.gguf` · current commit SHA on the HF
   main branch at run time. Captured digest pinned in ADR-020 and
   SUPPLY_CHAIN.md.

## Alternatives considered

- **Native `aegis pull hf:Qwen/...` transport in the runtime.** Rejected:
  HF has no Sigstore signatures and no OCI endpoint, so adding HF as a
  peer transport would force `pull::pull` to mix two unrelated trust
  models (OCI+cosign vs HF's TLS+commit-hash). The runtime would become
  harder to reason about and the F1 SVID-binding promise would degrade
  for HF-sourced models. Air-gap reviewers would lose coverage of
  HF-pulled models entirely.
- **A new `aegis import-from-hf` Rust subcommand.** Rejected as too
  heavy. The mirror pipeline is operator-facing tooling, not runtime
  tooling. Putting it in the Rust binary expands the supply-chain
  surface for a one-time-per-model operation.
- **Skip the project-published mirror; require every operator to publish
  their own.** Rejected for the demo program: a first-time evaluator
  would need to run a multi-step publishing pipeline before any demo
  works. The project ships one canonical demo model; orgs in
  production still publish their own per their license/scan policy.
- **Use Docker's HF→OCI bridge as a transparent transport.** Rejected:
  Docker's bridge converts HF repos into Docker container images, not
  OCI artifacts. PR #79 demonstrated empirically that container images
  and `pull::pull` are incompatible. Re-publishing as a single-blob OCI
  artifact under our own signing identity is cleaner.

## Related

- [ADR-013 OCI Artifacts for Model Distribution](013-oci-artifacts-for-model-distribution.md)
  — extended by this ADR's upstream-source policy.
- [ADR-017 Local Development Environment](017-local-development-environment-devcontainer-mise.md)
  — provides the same Sigstore keyless flow `models-publish.yml` reuses.
- [ADR-020 Recorded Demo Program](020-recorded-demo-program.md) — the
  Qwen2.5-1.5B Q4_K_M pin; this ADR fills in *where* it comes from.
- OCI-A [#66](https://github.com/tosin2013/aegis-node/issues/66) —
  shipped the consuming `aegis pull` subcommand.
- OCI-C [#68](https://github.com/tosin2013/aegis-node/issues/68) —
  operator workflow doc; this ADR sets its scope.
