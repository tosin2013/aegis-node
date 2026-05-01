# 23. LiteRT-LM as Second Inference Backend (CPU + Greedy, Phase 1)

**Status:** Accepted
**Date:** 2026-05-01
**Domain:** Runtime / model distribution (extends [ADR-014](014-cpu-first-gguf-inference-via-llama-cpp.md), reuses [ADR-022](022-trust-boundary-format-agnosticism.md), supports [ADR-020](020-recorded-demo-program.md))

## Context

Aegis-Node's current inference path is GGUF-only via `aegis-llama-backend`
(LLM-A → llama.cpp FFI; ADR-014). The lead demo model is
**Qwen2.5-1.5B-Instruct Q4_K_M**, hosted at HuggingFace and mirrored
to GHCR per ADR-021.

Two pressures push us toward a second backend:

1. **Enterprise origin.** Some target customers (US defense /
   regulated finance / EU public-sector) have written or de-facto
   restrictions on Chinese-origin AI models, regardless of license.
   Qwen is Apache-2.0 but Alibaba-origin; that's a legitimate
   compliance friction the Aegis-Node thesis ("survive a zero-trust
   security review") has to engage with. Western-origin GGUFs
   (Phi-4-mini, Gemma 3, Mistral 7B, Llama 3.2) cover that gap
   without runtime changes — but they don't unlock anything new on
   the inference side.

2. **Edge + first-party Google ecosystem.** The
   [LiteRT-LM project](https://ai.google.dev/edge/litert-lm) ships a
   Google-blessed inference runtime targeting edge devices and
   workstations. The `litert-community` HF org carries 90+ models in
   `.litertlm` format that **llama.cpp cannot read**. Notably:
   - **Gemma 4** (E2B 2.58 GB, E4B 3.65 GB) — agentic / multimodal,
     released exclusively in `.litertlm` for now.
   - **`functiongemma-270m-ft-mobile-actions`** — 270M-param model
     fine-tuned specifically for tool calls. At Q4 it would run
     >100 tok/s on a single CPU socket — demos finish in <2 seconds.
   - **FastVLM-0.5B**, **TranslateGemma-4B**, **EmbeddingGemma-300M** —
     task-specialized variants Google ships only here.

   These aren't "another way to run Gemma." They're an **adjacent
   ecosystem** of edge-targeted, task-specialized models, currently
   unreachable through llama.cpp.

The runtime's [LLM-B `Backend` / `LoadedModel` trait abstraction](../../crates/inference-engine/src/backend.rs)
was explicitly designed for this expansion (per ADR-014 §"Decision
item 3: trait abstraction so v2.0.0 GPU backends slot in cleanly").
Adding a second backend exercises that abstraction in a way docs
can't.

## Decision

Ship `aegis-litertlm-backend` as a second `Backend` impl alongside
`aegis-llama-backend`, with **five constraints** that bound scope:

1. **C ABI only.** The wrapper binds to LiteRT-LM's
   [`c/engine.h`](https://github.com/google-ai-edge/LiteRT-LM/blob/main/c/engine.h)
   surface (opaque `LiteRtLmEngine*` / `LiteRtLmSession*` /
   `LiteRtLmConversation*` handles, fully `extern "C"`,
   `bindgen`-friendly). C++ types like `absl::Status` are off-limits.

2. **Prebuilt binary distribution, not source build.** LiteRT-LM's
   primary build system is Bazel + TensorFlow + abseil + flatbuffers
   + sentencepiece + tokenizers-cpp + antlr4 + llguidance + minja +
   DXC; building from source inside a `build.rs` is unacceptable.
   Google ships **`litert_lm_main.linux_x86_64`** as a release asset
   per release; we pin the release tag, download the `.so` at
   `build.rs` time, sha-verify against an in-tree pinned digest, and
   link dynamically. Apache 2.0 — vendoring the header + bundling
   the prebuilt `.so` is permissive.

3. **CPU + greedy sampling only in Phase 1.** Upstream issues
   [#2080](https://github.com/google-ai-edge/LiteRT-LM/issues/2080)
   and PR
   [#2081](https://github.com/google-ai-edge/LiteRT-LM/pull/2081)
   document that LiteRT-LM's GPU/NPU samplers ignore the configured
   seed and temperature. CPU + `kLiteRtLmSamplerTypeGreedy` is the
   only currently-deterministic path. LLM-C's
   `inference.determinism.seed` + `temperature: 0.0` map to that.
   Phase 2 (when #2081 lands or we trust the upstream behavior)
   broadens to non-greedy + GPU; `aegis-litertlm-backend` refuses
   `temperature > 0` outside CPU until then.

4. **Reuse the existing `Backend` trait.** Zero changes to
   `inference-engine::backend`. `aegis-litertlm-backend::LiteRtLmBackend`
   implements `Backend` / `LoadedModel` exactly like
   `aegis-llama-backend::LlamaCppBackend` does. The CLI gains a new
   `litertlm` Cargo feature paralleling `llama`; the `Session::run_turn`
   driver doesn't know which backend it's talking to.

5. **OCI distribution via the existing pipeline.** ADR-022
   (trust-boundary format agnosticism) already commits to "publishers
   compute format-specific facts and assert them via cosign-signed
   manifest annotations." We add a new OCI artifact-type
   `application/vnd.aegis-node.model.litertlm.v1` and extend
   `models-publish.yml` to:
   - Download the `.litertlm` from `litert-community/<model>` on HF.
   - Compute SHA-256 of the file (existing pattern).
   - Parse the flatbuffer header per
     [`schema/core/litertlm_header_schema.fbs`](https://github.com/google-ai-edge/LiteRT-LM/blob/main/schema/core/litertlm_header_schema.fbs)
     and the `LlmMetadataProto` section per
     [`runtime/proto/llm_metadata.proto`](https://github.com/google-ai-edge/LiteRT-LM/blob/main/runtime/proto/llm_metadata.proto)
     to extract the chat template (`jinja_prompt_template` field).
     Hash that → `dev.aegis-node.chat-template.sha256` annotation.
     Same shape as the GGUF tokenizer.chat_template flow per ADR-022.
   - `oras push` with the new artifact-type, `cosign sign` keyless.
   - `aegis pull` already enforces the chat-template annotation per
     OCI-B (a) — runtime trust boundary doesn't change.

## Why not the alternatives

- **Build from Bazel + vendor TensorFlow.** Pulls hundreds of MB of
  source, fragile cross-compilation across Linux distros (glibc / abseil /
  protobuf ABI sensitivity), 3+ weeks of build-system work before the
  first inference call. Rejected.
- **Use the C++ runtime API directly via `cxx` or manual FFI.** The
  C++ types (`absl::Status`, `absl::AnyInvocable`, `nlohmann::json`)
  don't have stable ABIs across compilers. Maintenance overhead is
  permanent.
- **Wait for community LiteRT-LM Rust bindings.** None exist;
  Google's own `rust/` directory under their repo is just internal
  global-allocator / grammar-parser shims. No upstream signal that a
  binding is coming. Speculative.
- **Stay GGUF-only and swap Qwen for Phi-4-mini.** Solves the
  enterprise-origin concern but doesn't unblock the LiteRT-LM
  ecosystem (`functiongemma-270m`, `FastVLM-0.5B`, etc.). Filed as
  a smaller follow-up but not a substitute.

## Consequences

### Positive

- **Demo program gains an enterprise-clean inference path.** A
  customer that won't run Qwen can run Gemma 4 against the same
  manifest, same mediator, same ledger.
- **The `Backend` trait abstraction earns its keep.** A second impl
  proves the trait wasn't over-fit to llama.cpp.
- **`functiongemma-270m`** unlocks a class of demos (sub-second
  agent turns on commodity hardware) that 1.5B-class models can't
  hit.
- **Multimodal becomes plausible.** FastVLM lays the foundation for
  a future "agent reads a photo + reasons about it" demo, gated by
  the same F2/F4/F9 enforcement as text agents.

### Negative

- **Determinism is restricted in Phase 1.** Until upstream PR
  #2081 merges, the `inference.determinism` block can only set
  `temperature: 0.0` against the LiteRT-LM backend. Non-greedy
  sampling on GPU is silently broken upstream; we surface the
  restriction at boot rather than letting a manifest's `seed: 42 +
  temperature: 0.7` produce non-reproducible output.
- **Two FFI surfaces to maintain.** llama.cpp moves fast; LiteRT-LM
  also moves fast. We pin both at exact versions and bump only on
  explicit, reviewed changes (per ADR-014's `=0.1.145` precedent).
- **Build-time download.** `litertlm-sys`'s `build.rs` downloads
  ~29 MB on first build (cached after). Air-gapped contributors
  bring their own `LITERT_LM_PREBUILT_PATH` env var — same pattern
  as anyone vendoring a binary dep.
- **Two-format publish pipeline.** `models-publish.yml` needs a
  branch for GGUF vs `.litertlm`. Manageable — both share the
  cosign + oras + annotation scaffolding from OCI-A/B.
- **License nuance.** LiteRT-LM itself is Apache-2.0, but Gemma
  models ship under the **Gemma Terms of Use** (not Apache-2.0).
  That's permissive — allows redistribution + commercial use — but
  carries Google-specific use restrictions (no harmful applications
  list, etc.). ADR-021 §"License scope" already commits the
  Aegis-Node project's mirror to "Apache-2.0 / MIT / similarly
  permissive"; Gemma Terms qualify as "similarly permissive" but the
  decision should be explicit per model. Operators adapting the
  pipeline for restrictively-licensed models (Llama 3 / Qwen Vision
  / Cohere) follow ADR-021's operator path.

## Implementation plan

Three sub-issues, executed in order:

**LiteRT-A: `crates/litertlm-sys` + `crates/litertlm-backend` safe wrapper.**
- `litertlm-sys`: pin a LiteRT-LM release tag; `build.rs` downloads
  `litert_lm_main.linux_x86_64` + the engine `.so`, verifies sha
  against an in-tree pin, generates `bindgen` bindings from a
  vendored copy of `c/engine.h`. Apache-2.0; license file headers
  preserved.
- `litertlm-backend`: safe Rust wrapper with the same shape as
  `aegis-llama-backend` — typed errors, no `unsafe` blocks of our
  own (every unsafe goes through `litertlm-sys`'s extern "C" entry
  points), `abort_on_internal_panic` helper for FFI-callback safety,
  pinned upstream version.

**LiteRT-B: `Backend` / `LoadedModel` trait impl + `Session::run_turn` integration.**
- `LiteRtLmBackend::load(path) -> Box<dyn LoadedModel>` invokes the
  C ABI's `litert_lm_engine_settings_create` + `_engine_create`.
- `LoadedModel::infer(request)` opens a `LiteRtLmConversation`,
  calls `_set_tools(tools_json)` with the OpenAI-style catalog,
  `_set_messages(messages_json)`, `_send_message(prompt)`. Response
  JSON already has structured `tool_calls`; map directly into
  `InferResponse` — no `parse_response` needed (constrained
  decoding via llguidance is upstream-of-us).
- New CLI feature `litertlm` paralleling `llama`. `aegis run
  --backend litertlm --prompt ...` flag (or env `AEGIS_BACKEND`
  picks).

**LiteRT-C: `models-publish.yml` extension + first published `.litertlm` artifact.**
- Workflow learns a `format` input (`gguf` | `litertlm`).
- `gguf` path: existing flow.
- `litertlm` path: download from HF, parse flatbuffer header to
  extract `LlmMetadataProto.jinja_prompt_template`, hash, set
  `dev.aegis-node.chat-template.sha256` annotation, push with the
  new artifact-type, sign keyless.
- First published artifact: **`gemma-4-E2B-it`** (Google, ~2.6 GB).
- ADR-020 §"Pinned model" amended with both the Qwen pin and the
  Gemma 4 pin, framed as "Aegis-Node ships two reference models;
  operators select per their compliance posture."

Each sub-issue is a separate PR. Total estimate: 2–3 weeks of
focused work.

## Determinism + replay

LLM-C's `inference.determinism` semantics on the LiteRT-LM backend:

| Manifest knob | LiteRT-LM behavior (Phase 1, CPU only) |
|---|---|
| `seed` | Honored (CPU sampler reads it). Required for reproducibility. |
| `temperature: 0.0` | Maps to `kLiteRtLmSamplerTypeGreedy`. Always picks argmax. |
| `temperature > 0.0` | **Refused** at session boot until upstream PR #2081 lands. Typed error names the field + the upstream issue. |
| `top_p` / `top_k` / `repeat_penalty` | Honored on CPU. |

The AEGIS011 lint rule (per LLM-C) applies identically — manifests
that gate writes through F3 approvals but don't pin a seed get the
same warning.

## Update — 2026-05-01: Prebuilt path doesn't exist; Aegis publishes its own

The original §"Decision" item 2 above commits to "prebuilt binary
distribution, not source build" on the strength of the research
agent's claim that "every release ships `litert_lm_main.linux_x86_64`
as a release asset." **That claim is wrong.** Verified while starting
the LiteRT-A implementation:

- v0.10.2 (latest stable as of 2026-05-01) carries **zero release
  assets** on its [GitHub release page](https://github.com/google-ai-edge/LiteRT-LM/releases/tag/v0.10.2).
  Every prior release is the same.
- The PyPI wheel `litert-lm` is **30 KB of Python launcher scripts**
  — a Bazel orchestrator that invokes `bazel build` inside an internal
  venv. No bundled `.so`.
- [`docs/getting-started/build-and-run.md`](https://github.com/google-ai-edge/LiteRT-LM/blob/main/docs/getting-started/build-and-run.md)
  confirms: Linux x86_64 users **build from source via Bazel**
  (`bazel build //runtime/engine:litert_lm_main`).

That removes the cheap path. Three options at this point:

1. **Source-build via Bazel** inside `litertlm-sys/build.rs`. 3+
   weeks of fragile work; permanent ABI burden; `cargo build`
   becomes a 30-minute Bazel + TF + abseil + flatbuffers compile.
2. **Aegis maintains its own prebuilt** via the existing supply-chain
   pipeline. Aegis publishes a Linux x86_64 `.so` to GHCR, signed
   under our cosign keyless identity, exactly like the model
   artifacts shipped under [ADR-013](013-oci-artifacts-for-model-distribution.md)
   / [ADR-021](021-huggingface-as-upstream-oci-as-trust-boundary.md)
   / [ADR-022](022-trust-boundary-format-agnosticism.md). Eat our
   own dogfood.
3. **File issue with upstream**, pause LiteRT work, ship more demos
   on Qwen meanwhile. Unbounded delay.

### Decision (amendment): option 2.

Aegis-Node becomes the canonical upstream-of-upstream for LiteRT-LM
Linux x86_64 binaries used by the Aegis runtime. The same supply-chain
machinery that publishes signed model artifacts publishes signed
runtime artifacts.

#### Published artifact (current pin)

The [`litertlm-runtime-publish.yml`](../../.github/workflows/litertlm-runtime-publish.yml)
workflow ships a five-blob OCI artifact:

1. **`libaegis_litertlm_engine_cpu.so`** — the C ABI we bind, built
   via `cc_binary(linkshared=True, linkstatic=True)` against
   `//c:engine_cpu`. Mirror of upstream's
   `python/litert_lm/litert_lm_ext.so` pattern. Bundles all
   transitive cc_library + Rust crate archives — including
   `@crate_index__llguidance//:llguidance_cc` — statically into one
   `.so`. Linkopts: `-Wl,-Bsymbolic`, `-Wl,-rpath,'$ORIGIN'`,
   `-Wl,-z,noexecstack`.

2. **`libGemmaModelConstraintProvider.so`**, **`libLiteRt.so`**,
   **`libLiteRtTopKWebGpuSampler.so`**,
   **`libLiteRtWebGpuAccelerator.so`** — upstream-vendored,
   LFS-tracked under `prebuilt/linux_x86_64/`. Same set
   `python/litert_lm/BUILD`'s `_PREBUILT_LIBS` ships with the
   official Python wheel. The engine `.so` resolves them at
   session-load time via `rpath = $ORIGIN`.

All five files SHA-verify at build time inside
`litertlm-sys/build.rs` against the pinned constants below.

- **Reference:** `ghcr.io/tosin2013/aegis-node-runtime/litertlm-linux-amd64:latest`
- **Manifest digest:** `sha256:6add795dada783a61aeaf59892be7d249515ccf5cd13f0146b34eca2b841cbb4`
- **engine `.so` SHA-256:** `82d8f96c91ad28c6d3257b8235d88c6603660d2f2bd817241d1f86e4f45dd1e4` (~43.8 MB)
- **`libGemmaModelConstraintProvider.so` SHA-256:** `b30101a057a69d2c877266ac7373023864816ccaed7d9413d97b98ae12842009` (~22.8 MB)
- **`libLiteRt.so` SHA-256:** `e9844d634dbb69dbeb0bc51a71f7035bb7ba523e876384ff58192955b1da63e4` (~10.0 MB)
- **`libLiteRtTopKWebGpuSampler.so` SHA-256:** `f44b2eaded0a5b2e015c88a4eb6af960811c5a5df140f9101f84d845e8aff0ca` (~4.2 MB)
- **`libLiteRtWebGpuAccelerator.so` SHA-256:** `9523c6fd38f661599b904908f87d22448c2ff2c8da54291782e0c23fcf988863` (~17.6 MB)
- **`c/engine.h` SHA-256:** `cacee1d18aa9e2c22aeb8da2fc1576b25c03d7104e5319a0352c64a57bb691e9`
- **Upstream:** `google-ai-edge/LiteRT-LM` tag `v0.10.2`, commit `476c0bd49429569b2a4685c4db7a657d531d4b6e`
- **Bazel target:** `//c:libaegis_litertlm_engine_cpu.so` (Aegis overlay)
- **Bazel version:** 7.6.1
- **glibc target:** 2.39 (ubuntu-latest at build time)
- **Platform / kind:** linux/amd64 / cpu-only
- **Signed by:** `litertlm-runtime-publish.yml` workflow via Sigstore keyless

History — three publish iterations on this PR landed the right
shape:

1. [run 25223166187](https://github.com/tosin2013/aegis-node/actions/runs/25223166187)
   (post PR #110's LFS fix) — single-blob artifact at
   `sha256:75ac8138...`, engine `.so` only. The engine `.so`'s
   `DT_NEEDED` for `libGemmaModelConstraintProvider.so` couldn't
   resolve at session-load.
2. [run 25234002154](https://github.com/tosin2013/aegis-node/actions/runs/25234002154)
   — two-blob artifact at `sha256:e2296f31...` adding the
   constraint-provider. The `.so` loaded but failed at the first
   constrained-decoding call with `undefined symbol: llg_new_tokenizer`
   — `cc_shared_library` skips Bazel's Rust-managed cc deps, so
   the `llguidance` crate wasn't bundled.
3. [run 25234553974](https://github.com/tosin2013/aegis-node/actions/runs/25234553974)
   — the current artifact at `sha256:6add795d...`. Switched to
   `cc_binary(linkshared=True, linkstatic=True)` (mirroring
   `python/litert_lm/litert_lm_ext.so`) so the linker pulls all
   transitive archives, including `llguidance`, into one `.so`.
   Bundled the full `_PREBUILT_LIBS` set (all 4 prebuilts upstream's
   Python wheel ships). Confirmed via `nm`: `llg_*` symbols defined,
   no remaining external references beyond
   `libGemmaModelConstraintProvider.so` and system libs.

LiteRT-A's `litertlm-sys/build.rs` pins the current digest. Verified
end-to-end:

```bash
cosign verify ghcr.io/tosin2013/aegis-node-runtime/litertlm-linux-amd64@sha256:6add795dada783a61aeaf59892be7d249515ccf5cd13f0146b34eca2b841cbb4 \
  --certificate-identity-regexp '^https://github\.com/tosin2013/aegis-node/\.github/workflows/litertlm-runtime-publish\.yml@.*$' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

**New OCI artifact-type:**
`application/vnd.aegis-node.litertlm-runtime.v1`. Same cosign keyless
identity (the publish workflow's GitHub OIDC token), same `aegis pull`
verification flow, same chat-template-style annotations capturing the
upstream commit + Bazel toolchain version + `.so` SHA-256.

**New CI workflow:** `.github/workflows/litertlm-runtime-publish.yml`,
manual-dispatch only (per ADR-021's precedent). Inputs: upstream
LiteRT-LM tag (e.g., `v0.10.2`) + Bazel version. The workflow:

1. Checks out `google-ai-edge/LiteRT-LM` at the pinned tag.
2. Installs Bazel + clang + the upstream's transitive deps.
3. Runs `bazel build //c:engine` (or whatever target produces the
   shared library — the workflow probes the upstream BUILD files).
4. SHA-256s the resulting `.so`.
5. `oras push` to
   `ghcr.io/tosin2013/aegis-node-runtime/litertlm-linux-amd64`
   with `--artifact-type application/vnd.aegis-node.litertlm-runtime.v1`,
   plus annotations for the upstream commit + Bazel version + glibc
   target + abseil/protobuf ABI version.
6. `cosign sign` keyless via Sigstore.
7. Print the manifest digest for pinning.

**`litertlm-sys/build.rs`:** at build time, invokes `aegis pull`
against the pinned digest (CI runs in a working tree where `aegis`
is on PATH; non-CI / air-gapped contributors set
`LITERT_LM_RUNTIME_PATH` to a pre-staged `.so`). The SHA-256 sidecar
that `aegis pull` writes is verified before linking.

**Reproducibility.** A future contributor (or a security reviewer)
can rebuild the artifact from source via the published workflow:
the workflow file is the recipe, the input tag pins the upstream
state, the cosign signature ties the published bytes to the run
that produced them. Same audit posture as the model artifacts.

### Sub-issues (work order)

The original LiteRT-A scope splits into a new **LiteRT-0** (the
publish pipeline) plus a refined LiteRT-A (FFI wrapper that
*consumes* the published artifact):

| # | Issue | What | Status |
|---|---|---|---|
| LiteRT-0 (new) | TBD | `litertlm-runtime-publish.yml` workflow + first published runtime artifact (signed `.so` for `v0.10.2` against Bazel pin) | open |
| [#95 LiteRT-A](https://github.com/tosin2013/aegis-node/issues/95) | scope refined | `litertlm-sys`'s `build.rs` consumes the runtime artifact via `aegis pull`; `litertlm-backend` safe wrapper unchanged from original scope | open |
| [#96 LiteRT-B](https://github.com/tosin2013/aegis-node/issues/96) | unchanged | Backend / LoadedModel impl + CLI wiring | open |
| [#97 LiteRT-C](https://github.com/tosin2013/aegis-node/issues/97) | unchanged | `models-publish.yml` extension + first published Gemma 4 model | open |
| [#98](https://github.com/tosin2013/aegis-node/issues/98) | umbrella update | Track LiteRT-0 + the refined work order | open |

Total estimate revised: **~3–4 weeks** (was 2–3) of focused work
across the four sub-PRs. Most of the new week is LiteRT-0's CI
workflow + the first publish run; LiteRT-A's wrapper effort
*shrinks* once the artifact is in hand.

### What this amendment doesn't change

- The trait abstraction reuse (item 4).
- CPU + greedy sampling Phase 1 (item 3).
- C ABI only (item 1).
- Manifest annotation reuse for `.litertlm` chat templates (item 5).
- The Phase 2 deferred work (GPU/NPU when upstream determinism PR
  #2081 lands).
- Determinism semantics in §"Determinism + replay".

The amendment is purely a build-supply-chain change — the runtime
contract the rest of the ADR commits to is unchanged.

## Related

- [ADR-014 CPU-First GGUF Inference via llama.cpp](014-cpu-first-gguf-inference-via-llama-cpp.md) —
  the original FFI / safety-wrapper template this ADR mirrors.
- [ADR-021 HuggingFace as Canonical Upstream](021-huggingface-as-upstream-oci-as-trust-boundary.md) —
  the upstream-source policy this ADR extends to LiteRT-LM models.
- [ADR-022 Trust-Boundary Format Agnosticism](022-trust-boundary-format-agnosticism.md) —
  the manifest-annotation pattern this ADR reuses for `.litertlm`
  chat-template hashing AND the runtime artifact's annotations.
- LiteRT-LM upstream:
  [github.com/google-ai-edge/LiteRT-LM](https://github.com/google-ai-edge/LiteRT-LM)
  (Apache-2.0).
- LiteRT-LM determinism gap:
  [#2080](https://github.com/google-ai-edge/LiteRT-LM/issues/2080),
  [#2081](https://github.com/google-ai-edge/LiteRT-LM/pull/2081).
