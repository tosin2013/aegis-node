# 13. OCI Artifacts for Model Distribution and Update

**Status:** Accepted
**Date:** 2026-04-27
**Domain:** Supply Chain / Distribution

## Context

Existing local LLM runtimes (Ollama, llama.cpp) handle model updates in ways incompatible with zero-trust environments:
- Silent background pulls from vendor-controlled registries.
- No signature verification before loading model weights.
- Vulnerability to GGUF chat-template poisoning (recent research has shown attacks that alter model behavior without changing the weights, bypassing weight-only scans).

For air-gapped enterprises, defense contractors, and any regulated environment, "the runtime auto-updates from a vendor registry" is an immediate disqualifier. A new bespoke registry is also a non-starter — every enterprise already has hardened, scanned, signed OCI registries for container images.

## Decision

Aegis-Node packages and distributes models as OCI (Open Container Initiative) artifacts. Properties:

1. **Models = OCI artifacts.** GGUF (and equivalent) model files are wrapped as OCI artifacts and pushed to standard OCI registries (Harbor, Artifactory, AWS ECR, Quay).
2. **No background updates.** The runtime never pulls updates on its own. Model updates are explicit operator actions: `aegis pull internal.registry.com/models/llama-3-1b:v1.2`.
3. **Signature verification at load.** Before loading any model into memory, the runtime verifies the Cosign signature and SHA-256 hash. Missing or invalid signatures cause boot refusal.
4. **Chat-template scope.** Verification covers the GGUF file *and* the chat-template metadata bundled with it; template-only poisoning is detected.
5. **Air-gapped first.** The model pull path works against an internal registry with no public-internet dependency. Public registries are an opt-in remote source for internet-connected developers.

## Consequences

**Positive:**
- Inherits enterprise OCI tooling: Cosign signing, image scanning, registry RBAC, vulnerability databases — without rebuilding any of it.
- Air-gapped deployments work natively. Operators sync models via standard `oras pull` / `skopeo copy` workflows already approved in their environments.
- Signature failure on tampered or substituted models is the default behavior, not a configuration option.

**Negative:**
- Adds an OCI dependency to the runtime: must include either an embedded OCI client or a thin shell-out to `oras`.
- GGUF-as-OCI is a community convention but not an OCI spec; layer media types and digest semantics must be documented to avoid registry interop issues.
- Operators unfamiliar with OCI tooling face a learning curve; ship clear quickstart and `aegis pull` ergonomics.

## Domain Considerations

The decision aligns with the broader supply-chain security story: SBOMs, Cosign, in-toto attestations, SLSA levels. A reviewer steeped in software supply chain security recognizes the pattern instantly.

## Implementation Plan

1. Define the OCI artifact layout for GGUF models: manifest, layer media types, annotations.
2. Implement `aegis pull <ref>` using an embedded OCI client (preferred) or a shell-out to `oras` (fallback for first iteration).
3. Implement Cosign signature verification at load time; refuse to boot unsigned or invalid-signature models.
4. Document the operator workflow: download upstream model, scan, sign with org Cosign key, push to internal registry.
5. CI test: a tampered GGUF must fail boot with a clear error pointing to the signature mismatch.

## Related PRD Sections

- §6.1 The Model Distribution and Update Strategy
- §6 Post-MVP — Automatic Model Updates (rationale for deferral)
- §7 Architecture Principles (#3: Offline by default)

## Domain References

- OCI Distribution Specification
- ORAS (OCI Registry As Storage)
- Cosign / Sigstore
- SLSA Supply Chain Levels
