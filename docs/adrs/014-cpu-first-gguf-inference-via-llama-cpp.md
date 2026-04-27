# 14. CPU-First GGUF Inference via `llama.cpp` for Phase 1

**Status:** Accepted
**Date:** 2026-04-27
**Domain:** Inference Backend / Edge Deployment

## Context

Phase 1 of Aegis-Node targets a developer's laptop and the most constrained edge environments (air-gapped tactical deployments, embedded-class devices). These environments often have no GPU, limited memory, and no network. The goal is "if it works here, it works anywhere downstream."

Picking a GPU-only inference backend (vLLM, TGI) blocks this market entirely. A from-scratch CPU-optimized inference engine is years of work that does not differentiate Aegis-Node's value proposition (security, not speed).

`llama.cpp` is the established, battle-tested CPU/GGUF inference library; it has aggressive quantization support, broad model coverage, and a stable C API.

## Decision

Phase 1 inference runs entirely on CPU using `llama.cpp` (linked as a Rust FFI binding). Properties:

1. **GGUF models, quantized 4–8 bit, 1B–8B parameters.** Optimized for CPU inference on modern laptops without external acceleration.
2. **Rust FFI binding to `llama.cpp`.** The Rust runtime owns model load/unload, generation lifecycle, and integration with the runtime's policy enforcement boundaries (network deny, FS sandbox, identity binding).
3. **No GPU code path in Phase 1.** GPU support is tracked through public GitHub issues and is invited as community contribution; it is not a Phase 1 blocker.
4. **Pluggable backend abstraction prepared, not implemented.** The Rust crate exposes an internal trait that abstracts `llama.cpp` behind a `Backend` interface so vLLM/TGI/KServe backends in Phase 2 are additive, not refactors.
5. **Deterministic-by-default sampling settings.** Where the security review demands reproducibility (replay, regression tests), the runtime supports deterministic sampling parameters; non-determinism is opt-in.

## Consequences

**Positive:**
- Frictionless install on developer laptops — single binary, no GPU drivers, no CUDA dependency.
- Air-gap-friendly: model + runtime is the entire footprint.
- Inherits `llama.cpp`'s broad model support, quantization formats, and ongoing performance improvements.
- The "if Aegis-Node passes the security review on a laptop, it passes on a cluster" message becomes credible because the security model is identical across both backends.

**Negative:**
- Performance ceiling on CPU constrains the model sizes practical for Phase 1; some real-world agent tasks will be slow.
- FFI boundary requires careful memory and panic-safety handling; segfaults in `llama.cpp` must not corrupt the Rust runtime's state.
- Tracking `llama.cpp` upstream (it moves fast) is recurring maintenance overhead.
- Backend abstraction is theoretical until Phase 2 actually adds a second backend; design risk that the trait does not generalize cleanly.

## Domain Considerations

The CPU-first stance aligns the local CLI tier with the realities of regulated edge deployments where GPUs are not available, not approved, or not allowed.

## Implementation Plan

1. Build the Rust FFI binding to `llama.cpp` with a strict safety wrapper (no unwrap on FFI returns, defined behavior on panic).
2. Define the `Backend` trait abstraction for future GPU/vLLM backends; implement the `LlamaCppBackend` against it.
3. Wire the inference path through the policy enforcement points (network deny, FS sandbox, identity binding).
4. Define determinism config knobs (seed, temperature, top-p) and expose them through the manifest where reproducibility is required.
5. Pin a known-good `llama.cpp` revision; track upstream with deliberate, reviewed bumps.

## Related PRD Sections

- §5 Phase 1: Local CLI (CPU-First, Scale Up)
- §7 Architecture Principles (#3: Offline by default; #4: Split-Language)

## Domain References

- `llama.cpp` project (ggml-org)
- GGUF file format
- ONNX Runtime as a comparison point for backend abstraction
