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

## Why this is the same pattern the runtime uses for models

When `aegis pull <ref>` lands in Phase 1c, it will follow this exact verification flow before loading model weights into memory:

1. Pull the model artifact from the configured registry (internal in air-gap, public for development).
2. Run `cosign verify` against the configured trust root.
3. Recompute the SHA-256 digest of the GGUF file *and* the chat-template metadata (defends against template-only poisoning per ADR-013).
4. Refuse to boot if any check fails.

The devbox image is a small live demo of that pattern: a signed OCI artifact, verifiable today, in a registry the reviewer's enterprise infrastructure already supports.

## Reporting integrity issues

If verification ever fails on an artifact pulled from `ghcr.io/tosin2013/aegis-node-*`, do not use it. Open a private vulnerability report on the GitHub repo with:

- The artifact reference (`<registry>/<repo>:<tag>` and the digest).
- The verification command and full output.
- The time of the pull.
