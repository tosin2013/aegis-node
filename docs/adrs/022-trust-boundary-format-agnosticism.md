# 22. Trust-Boundary Format Agnosticism — Verify Signed Claims, Don't Parse Files

**Status:** Accepted
**Date:** 2026-04-30
**Domain:** Supply chain / runtime trust boundary (extends [ADR-013](013-oci-artifacts-for-model-distribution.md), [ADR-021](021-huggingface-as-upstream-oci-as-trust-boundary.md); supports [ADR-020](020-recorded-demo-program.md))

## Context

[ADR-013 §"Decision" item 4](013-oci-artifacts-for-model-distribution.md)
commits to verifying both the GGUF file *and* the chat-template metadata
at pull time, defending against template-only poisoning. [Issue #67](https://github.com/tosin2013/aegis-node/issues/67)
(OCI-B) decomposes that work: pull-side extraction (this ADR), session-
side SVID binding (follow-up).

The first attempt at the pull-side work — on branch
`feat/oci-b-chat-template`, never merged — added a hand-written 200-line
GGUF v3 metadata parser to `crates/cli/src/gguf.rs`. That implementation
worked; the unit tests passed; the real-image fixture extracted the
expected 2509-byte Qwen template and produced
`d5495a1e5db0611132a97e46a65dbb64a642a499421228b9c8b93229097fa9a4`. We
discarded it anyway. This ADR records why.

The user's objection sharpened the question. *If a custom parser is the
right answer for GGUF, we will write a custom parser for ONNX next, and
Safetensors after that, and a vendor-bundle parser the year after.* The
trust boundary accumulates one format-aware codebase per format, forever.
Each codebase is its own audit surface, its own CVE class, its own
"what does this byte mean in version N+1" debate. That is not a
defensible long-term posture for a security-focused agent runtime.

April 2026 research (logged in the predecessor branch) found:

- **Off-the-shelf Rust GGUF crates are wrong-shaped.** `gguf` (Jimexist)
  is unmaintained since 2023; `gguf-rs` pulls CLI dependencies into the
  library path; `gguf-rs-lib` is 7.8k LoC of read/write surface for
  what is fundamentally a one-key read; `candle-core` drags 50+
  transitive dependencies including the C library `onig`. None match
  the "small, stdlib-only, just-this-one-key" profile that a trust-
  boundary parser actually needs.
- **The llama.cpp FFI binding (`llama-cpp-2`, [LLM-A #70](https://github.com/tosin2013/aegis-node/issues/70))
  is the wrong layer.** llama.cpp parses GGUF at *inference* time,
  inside the trust boundary. The pull path runs *before* tensors load —
  using llama.cpp's parser there means dragging the inference-side
  dependency tree (CUDA/Metal/Vulkan toggles, half-precision math, the
  full ggml surface) into a step that should ship in seconds.
- **The CVE landscape cuts the same direction.** GGUF parsers have an
  active CVE history — buffer overflow in llama.cpp vocab loading
  ([CVE-2025-49847](https://nvd.nist.gov/vuln/detail/CVE-2025-49847)),
  integer overflow in the GGUF parser ([GHSA-vgg9-87g3-85w8](https://github.com/ggml-org/llama.cpp/security/advisories/GHSA-vgg9-87g3-85w8),
  CVE-2025-53630), SGLang RCE via malicious GGUF
  ([CVE-2026-5760](https://thehackernews.com/2026/04/sglang-cve-2026-5760-cvss-98-enables.html)),
  llama-cpp-python SSTI in model metadata
  ([CVE-2024-34359](https://github.com/advisories/GHSA-56xg-wfcc-g829)).
  The bugs all sit in length casts, bounds checks, and field
  validation. A hand-rolled trust-boundary parser is most likely to
  ship the *next* CVE in that family. A third-party crate may or may
  not — but we own the consequence either way.

The structural problem isn't *which parser*. The structural problem is
having format-aware code at the trust boundary at all.

## Decision

**The runtime trust boundary verifies signed claims, never parses model
file formats.**

Specifically:

1. **Publishers compute format-specific facts** — chat-template hash,
   weight count, license string, quantization level, tokenizer-vocab
   hash, anything else operators want to pin — at publish time, using
   whatever tool agrees with the consumer. For GGUF that means the
   official `gguf` Python package maintained by ggml-org; for ONNX,
   `onnx-py`; for Safetensors, the format author's tooling. Maintenance
   lives where the format definition lives.

2. **Publishers assert those facts via cosign-signed manifest annotations**
   on the OCI artifact. The signature is over the manifest JSON, which
   includes `annotations` — so cosign covers the claim transitively.
   Operators who don't trust a publisher's keyless identity already
   refuse the artifact at `cosign verify`; trusting a publisher's
   *claim* on top of trusting the publisher's *identity* is
   axiomatically not an additional trust escalation.

3. **The runtime reads the annotation and persists it as a sidecar.**
   `aegis pull` runs `oras manifest fetch`, parses the JSON, extracts
   the named annotation (`dev.aegis-node.chat-template.sha256` for the
   first instance; analogous keys for future facts), validates the
   value's shape (e.g., 64-char lowercase hex for SHA-256), and writes
   `chat_template.sha256.txt` alongside the cached blob. Future F1
   binding (OCI-B (b)) reads that sidecar.

4. **Defense in depth comes from the consumer, not the trust boundary.**
   At session boot — once [LLM-A #70](https://github.com/tosin2013/aegis-node/issues/70)
   lands — llama.cpp will parse the GGUF as part of inference setup.
   At that point, the F1 binding code can ask llama.cpp for its view
   of the chat-template bytes and compare against the SVID-bound
   digest. If they disagree, the session refuses. Llama.cpp is the
   parser we can't avoid (we have to load the model anyway); reusing
   it for cross-checking is free. The trust boundary itself stays
   format-agnostic.

5. **Annotation policy is fail-closed for declared model artifacts.**
   For OCI artifacts whose `artifactType` is
   `application/vnd.aegis-node.model.gguf.v1`, the chat-template
   annotation is **required**. Missing → typed
   `MissingChatTemplateAnnotation` refusal. For artifacts that don't
   declare the model media type (e.g., the devbox image,
   third-party tooling), the annotation is optional and `aegis pull`
   stays general-purpose.

This generalizes beyond GGUF. The same shape — publisher computes,
publisher signs, runtime verifies — handles ONNX, Safetensors,
vendor-specific formats, and whatever model packaging arrives next. New
formats add publisher-side parser dependencies (in CI) and runtime
annotation keys (one constant); they don't add trust-boundary parsing
code.

## Why not the alternatives

- **Custom parser at the runtime trust boundary** — the rejected
  approach this ADR records. Accumulating audit surface per format,
  forever, with no way out. Even a small parser this year is the
  template for the next four formats that will want similar handling.
- **Pull in `gguf-rs-lib` (or similar third-party crate)** — moves
  the audit cost from "we wrote it" to "we depend on it," but the
  audit cost is still ours. 7.8k LoC of crate-imported parser still
  sits inside the trust boundary; transitive dependencies still expand
  it.
- **Embed `llama-cpp-2` FFI just for header reads** — drags
  inference-time deps into the pull path; runs the *consumer's*
  parser at the *boundary* (wrong layer); blows compile time and
  binary size for the pull subcommand by an order of magnitude.
- **Use OCI sidecar layers for the chat-template instead of an
  annotation** — viable but heavier (each fact becomes a separate
  layer descriptor). Annotations are simpler and fit the "one-fact"
  pattern. We may reach for sidecar layers when a fact's *bytes*
  matter (e.g., a full SBOM, a license file). Pure hashes belong in
  annotations.
- **Skip the chat-template binding entirely; rely on cosign-of-blob**
  — the original "what cosign covers" model. ADR-013 §4 already
  explicitly rejects this: an attacker with push access *and* a
  valid signing identity can re-publish the same weights with a
  swapped template, and a blob-only binding doesn't catch it.

## Consequences

### Positive

- **Trust-boundary code is small and immutable** with respect to
  format evolution. New formats don't propagate into `crates/cli/src/pull.rs`.
- **Format-author tooling stays the format author's problem.** When
  GGUF v4 ships, the project updates one workflow step (the Python
  `gguf` install pin), not the runtime.
- **Auditing is bounded.** The full trust-boundary code is ~30 lines
  of JSON-annotation-read in `pull.rs` + cosign + oras. That's
  reviewable in one sitting; a custom parser is not.
- **Operators can adopt the same pattern for their own facts.** An
  operator who needs to pin (say) the embedding-vocab hash adds an
  org-prefixed annotation, signs it with their identity, and pins
  the value in their config. The runtime reads it the same way it
  reads chat-template.
- **Generalizes without re-litigating the architecture.** Future
  ONNX / Safetensors / vendor-format ADRs reference this one and add
  per-format publisher tooling; they don't re-justify the layering.

### Negative

- **Publishers are responsible for computing the right hash.** A
  publisher's parser disagreeing with the consumer's parser — e.g.,
  `gguf-py` returning different bytes than llama.cpp would — would
  surface as a session-boot rebind violation under OCI-B (b). We
  mitigate by recommending the format author's official tooling on
  the publisher side (same maintainer set as the consumer), and by
  the planned defense-in-depth check at session boot. We do not
  attempt to verify alignment by re-parsing in the runtime — that
  would re-introduce exactly what this ADR forbids.
- **Operators can no longer pull "raw" untrusted GGUFs from random
  registries and expect chat-template binding.** A model artifact
  that doesn't declare the Aegis-Node media type, or doesn't carry
  the annotation, falls through to the `chat_template_sha256_hex =
  None` path and offers no F1 template-binding protection. We
  consider this a *feature*: the binding is supposed to require an
  attesting publisher.
- **Workflow build cost grows.** `models-publish.yml` now installs
  the `gguf` Python package on every dispatch (~10 MB, negligible).
- **Re-publish required for already-published artifacts.** The
  pre-existing Qwen artifact at
  `ghcr.io/tosin2013/aegis-node-models/qwen2.5-1.5b-instruct-q4_k_m@sha256:240ece32...`
  was published before this change and has no annotation. The OCI-B
  PR that lands this ADR re-runs `models-publish.yml` against the
  same HF revision (`91cad51170dc346986eccefdc2dd33a9da36ead9`); a
  new manifest digest results because annotations change the
  manifest. Old digest stays accessible for replay but is no longer
  the operator pin.

## Implementation

Lands across two PRs both tracked under [OCI-B (#67)](https://github.com/tosin2013/aegis-node/issues/67):

**OCI-B (a) — pull-side annotation read** (PR #85):

1. `crates/cli/src/pull.rs` — add `run_oras_manifest_fetch` and
   `extract_chat_template_annotation`; remove the discarded GGUF
   parser.
2. `.github/workflows/models-publish.yml` — install `gguf>=0.10.0`,
   compute `tokenizer.chat_template` SHA-256 via `GGUFReader`, set
   `dev.aegis-node.chat-template.sha256` annotation on `oras push`.
3. Re-publish Qwen with the annotation; update the pinned manifest
   digest in `tests/pull_real_image.rs`, ADR-020 §"Pinned model",
   and `docs/SUPPLY_CHAIN.md`.
4. Tests in `crates/cli/tests/pull.rs` cover annotation-present /
   annotation-missing-on-model-artifact / non-model-artifact-without-
   annotation / bad-hex-annotation paths.

**OCI-B (b) — session-side SVID binding**:

1. `crates/identity/src/svid.rs` + `ca.rs` — add a *separate*
   `CHAT_TEMPLATE_BINDING_OID` X.509 extension carrying the 32-byte
   SHA-256, alongside the existing 96-byte `(model, manifest, config)`
   extension. Separate extension keeps the Compatibility Charter
   freeze on the digest-binding payload format intact (pre-OCI-B SVIDs
   stay valid; the new extension is optional).
2. `crates/identity/src/ca.rs::issue_svid_with_chat_template` — new
   public entry point taking `Option<Digest>`; the original
   `issue_svid` now delegates with `None`.
3. `crates/identity/src/binding.rs` — `verify_chat_template_binding`
   helper, plus a `DigestField::ChatTemplate` variant.
4. `crates/inference-engine/src/session.rs` — `BootConfig` grows
   `chat_template_sidecar: Option<PathBuf>`. When set, `Session::boot`
   reads `chat_template.sha256.txt` (the file written by `aegis pull`),
   binds the digest into the SVID, surfaces it as
   `chatTemplateDigestHex` in `SessionStart`, and exposes it via
   `Session::bound_chat_template`. Malformed/absent sidecar →
   `Error::ChatTemplateSidecar`.
5. `aegis run --chat-template-sidecar <path>` flag wires the option
   end-to-end.

Per-tool-call rebind for chat-template is intentionally **not** wired
under (b): the chat-template digest is read-only attestation. A
defense-in-depth re-derivation against llama.cpp's parser at session
boot lands once [LLM-A #70](https://github.com/tosin2013/aegis-node/issues/70)
exposes the parsed bytes — that's the only point we can ask "what does
the consumer actually use?" without re-introducing a runtime parser.

## Related

- [ADR-013 OCI Artifacts for Model Distribution](013-oci-artifacts-for-model-distribution.md)
  — §"Decision" item 4 motivates this ADR's mechanism.
- [ADR-021 HuggingFace as Canonical Upstream](021-huggingface-as-upstream-oci-as-trust-boundary.md)
  — same pipeline; this ADR adds the annotation step to it.
- [ADR-020 Recorded Demo Program](020-recorded-demo-program.md) —
  pinned Qwen artifact moves to a new manifest digest after re-publish.
- [LLM-A #70](https://github.com/tosin2013/aegis-node/issues/70) —
  llama.cpp FFI; once landed, becomes the defense-in-depth re-derivation
  of the chat-template hash at session boot.
- Future ONNX / Safetensors / vendor-format ADRs (none filed yet) will
  follow this same publisher-asserts / runtime-verifies pattern.
