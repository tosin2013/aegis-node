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

## Related

- [ADR-014 CPU-First GGUF Inference via llama.cpp](014-cpu-first-gguf-inference-via-llama-cpp.md) —
  the original FFI / safety-wrapper template this ADR mirrors.
- [ADR-021 HuggingFace as Canonical Upstream](021-huggingface-as-upstream-oci-as-trust-boundary.md) —
  the upstream-source policy this ADR extends to LiteRT-LM models.
- [ADR-022 Trust-Boundary Format Agnosticism](022-trust-boundary-format-agnosticism.md) —
  the manifest-annotation pattern this ADR reuses for `.litertlm`
  chat-template hashing.
- LiteRT-LM upstream:
  [github.com/google-ai-edge/LiteRT-LM](https://github.com/google-ai-edge/LiteRT-LM)
  (Apache-2.0).
- LiteRT-LM determinism gap:
  [#2080](https://github.com/google-ai-edge/LiteRT-LM/issues/2080),
  [#2081](https://github.com/google-ai-edge/LiteRT-LM/pull/2081).
