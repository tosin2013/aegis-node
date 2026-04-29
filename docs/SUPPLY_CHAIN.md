# Supply Chain Verification

This document is the practical "trust but verify" companion to [ADR-013](adrs/013-oci-artifacts-for-model-distribution.md) and [ADR-017](adrs/017-local-development-environment-devcontainer-mise.md).

The Aegis-Node thesis is that AI agent runtimes can pass zero-trust infrastructure reviews. Part of passing that review is letting the reviewer verify the build and dependency chain themselves, with their own tools, against a public transparency log. This document is the verification step they will take.

## What's signed

| Artifact | Registry | Tag pattern | Signature | Status |
|---|---|---|---|---|
| Devbox image | `ghcr.io/tosin2013/aegis-node-devbox` | `latest`, `sha-<commit>` | Cosign keyless via [Sigstore](https://sigstore.dev/), tied to GitHub Actions OIDC | ✅ live |
| Model OCI artifacts | `ghcr.io/tosin2013/aegis-node-models` (planned) | `<model>:<semver>` | Cosign | 🚧 Phase 1c |
| Aegis-Node release binaries | GitHub Releases | `v<semver>` | Cosign + SLSA provenance | 🚧 Phase 1 GA |

## Prerequisites

```bash
# cosign — signature verification
brew install cosign
# or: see https://docs.sigstore.dev/cosign/system_config/installation/

# oras — OCI registry CLI (works against air-gapped internal registries)
brew install oras
```

The devcontainer image already bundles both tools at the pinned versions.

## Verifying the devbox image

The devbox is signed at every push to `main`. The signature claims:
- The image was built from `github.com/tosin2013/aegis-node` workflow `.github/workflows/devbox.yml`.
- It was signed by GitHub Actions running on that workflow at the commit pinned to `sha-<commit>`.

```bash
cosign verify ghcr.io/tosin2013/aegis-node-devbox:latest \
  --certificate-identity-regexp '^https://github\.com/tosin2013/aegis-node/\.github/workflows/devbox\.yml@.*$' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

Successful verification prints the signature certificate and the Sigstore transparency-log entry (Rekor). Any other output means the image is not authentic — do not use it.

To pin to a specific commit instead of `latest`:

```bash
cosign verify ghcr.io/tosin2013/aegis-node-devbox:sha-<commit> \
  --certificate-identity-regexp '^https://github\.com/tosin2013/aegis-node/\.github/workflows/devbox\.yml@.*$' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

## Air-gapped reviewer workflow

Reviewers in air-gapped environments verify once on an internet-connected machine, then mirror the artifact to their internal registry:

```bash
# 1. On an internet-connected box: pull and verify
oras pull ghcr.io/tosin2013/aegis-node-devbox:sha-<commit>
cosign verify ghcr.io/tosin2013/aegis-node-devbox:sha-<commit> \
  --certificate-identity-regexp '^https://github\.com/tosin2013/aegis-node/\.github/workflows/devbox\.yml@.*$' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com

# 2. Mirror to your internal registry (Harbor, Artifactory, ECR, Quay)
oras cp ghcr.io/tosin2013/aegis-node-devbox:sha-<commit> \
        internal.example.com/aegis/devbox:sha-<commit>

# 3. On the air-gapped box: pull from internal registry by digest
oras pull internal.example.com/aegis/devbox@sha256:<digest>
```

Cosign verification on the air-gapped side normally fetches from the Sigstore transparency log over the public internet. Two acceptable patterns:

**Option A — verify online once and pin the digest.** The internal registry only ever serves images by digest, and digest equality is itself an integrity guarantee on the air-gap side.

**Option B — bundle the signature locally.** Use `cosign save` on the connected side and `cosign load` + `cosign verify --offline` on the air-gapped side, with the Sigstore root keys baked into your verifier image.

Either approach is acceptable for compliance evidence; document which one your environment uses in your security-review package.

## `aegis pull` (OCI-A, ADR-013)

The `aegis pull <ref>` subcommand wraps the same flow this doc describes — `oras pull` + `cosign verify` + SHA-256 recompute — and refuses to cache an artifact unless every gate passes. Output goes to a content-addressed cache the F1 boot path can find by digest.

```bash
# Reference must be digest-pinned (@sha256:<64 hex>). Tags-only refs
# are refused so the SVID's bound model digest can't be invalidated
# by a moving tag.
aegis pull ghcr.io/tosin2013/aegis-node-devbox@sha256:<digest> \
  --keyless-identity '^https://github\.com/tosin2013/aegis-node/\.github/workflows/devbox\.yml@.*$' \
  --keyless-oidc-issuer 'https://token\.actions\.githubusercontent\.com'
```

Successful pull prints:

```text
# verified
reference: ghcr.io/tosin2013/aegis-node-devbox@sha256:<digest>
sha256:    <digest>
blob_path: ~/.cache/aegis/models/<digest>/blob.bin
```

Refusal cases — every one of these exits non-zero with a typed error:

| Case | Error | Effect |
|---|---|---|
| Reference uses a tag instead of `@sha256:` | `UnpinnedRef` | refuse before any network call |
| `oras` or `cosign` not on `$PATH` | `MissingTool` | refuse before any network call |
| Cosign signature missing or fails | `CosignVerifyFailed` | refuse, blob not cached |
| Pulled blob's SHA-256 ≠ pinned digest | `Sha256Mismatch` | refuse, blob discarded |
| Cached blob corrupted between pulls | `Sha256Mismatch` | refuse, surface tampering |

**Smoke-testing without a model artifact.** Until we publish a Cosign-signed model OCI artifact under `ghcr.io/tosin2013/aegis-node-models`, the only signed thing we publish is the **devbox container image** — and `aegis pull` is intentionally not the right tool for container images (`oras pull` skips Docker-format layers without explicit flags that `pull::pull` deliberately omits, since real model artifacts are single-blob by design).

You can still verify the supply chain is sound — just use `cosign verify` directly against the devbox while we wait on a real model artifact:

```bash
DIGEST=$(oras manifest fetch --descriptor \
  ghcr.io/tosin2013/aegis-node-devbox:latest | jq -r .digest)

cosign verify ghcr.io/tosin2013/aegis-node-devbox@"${DIGEST}" \
  --certificate-identity-regexp '^https://github\.com/tosin2013/aegis-node/\.github/workflows/devbox\.yml@.*$' \
  --certificate-oidc-issuer 'https://token.actions.githubusercontent.com'
```

End-to-end `aegis pull` smoke-testing lands once `models-publish.yml` (per the ADR-021 plan) publishes a real model OCI artifact. GGUF + chat-template-bound verification is OCI-B (#67); operator workflow doc is OCI-C (#68).

## Reporting integrity issues

If verification ever fails on an artifact pulled from `ghcr.io/tosin2013/aegis-node-*`, do not use it. Open a private vulnerability report on the GitHub repo with:

- The artifact reference (`<registry>/<repo>:<tag>` and the digest).
- The verification command and full output.
- The time of the pull.
