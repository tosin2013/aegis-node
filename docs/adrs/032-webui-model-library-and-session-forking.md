# 32. WebUI Model Library and Session Forking

**Status:** Proposed
**Date:** 2026-05-05
**Domain:** UI / supply chain (extends [ADR-013](013-oci-artifacts-for-model-distribution.md), [ADR-022](022-trust-boundary-format-agnosticism.md), [ADR-023](023-litertlm-as-second-inference-backend.md), supports [ADR-031](031-community-webui-for-local-collaboration.md))
**Targets:** v0.9.5 Phase 1d

## Context

Once the [Community UI (ADR-031)](031-community-webui-for-local-collaboration.md)
gives operators a chat surface, the next question is "how do I switch
the model the agent is running?" In the CLI workflow, this is
explicit: `aegis pull <oci-ref>` then `aegis run --model
~/.cache/aegis/...`. In a chat UI, the model name is invisible —
operators reach for a dropdown.

Two zero-trust constraints make naive UX dangerous:

1. **No arbitrary file uploads.** Per [ADR-013](013-oci-artifacts-for-model-distribution.md)
   and [ADR-021](021-huggingface-as-upstream-oci-as-trust-boundary.md),
   models enter the runtime exclusively via signed OCI artifacts.
   A "drop a `.gguf` file here" UX would bypass the entire supply
   chain, defeating the F1 binding to a verified model digest.
2. **No mid-session model swap.** F1 binds workload identity to the
   `(model_digest, manifest_digest, config_digest)` triple at session
   start ([ADR-003](003-cryptographic-workload-identity-spiffe-spire.md)).
   The hash-chained ledger ([ADR-011](011-hash-chained-tamper-evident-ledger.md))
   anchors the entire session to that triple. Hot-swapping a model
   mid-conversation would invalidate the chain or require a new chain
   that pretends the prior one used a different model — both unsafe.

The UI must offer "switch the model" as a feature without violating
either constraint.

## Decision

**Model loading in the WebUI is a visual wrapper around `aegis pull`
that streams progress, surfaces cosign verification results, and adds
the verified artifact to the local library — never accepting arbitrary
file uploads. Model switching is implemented as Session Forking: the
current session is gracefully ended, a new session is booted with the
new model digest, and the chat history is replayed as the user prompt
context. The UI auto-detects the required backend (llama / litertlm)
from the OCI artifact's media type.**

### Model Library — visual `aegis pull`

The "Model Library" view (in the Community UI's Settings / Models
sidebar) lists locally-cached models with:

- OCI ref + digest
- Cosign verification result (verified-by identity, signature timestamp)
- Backend kind (llama / litertlm)
- Cache size + last-used timestamp

**Adding a model:** the operator pastes an OCI ref (e.g.
`ghcr.io/tosin2013/aegis-node-models/gemma-4-e4b-it@sha256:...`).
The UI shells out to the existing `crates/cli/src/pull.rs` flow and
streams progress over WebSocket:

```text
[1/4] Resolving manifest digest …
[2/4] Pulling oras blobs …  ████████████░░░░  72% (2.1 GB / 2.9 GB)
[3/4] cosign verify --keyless …  ✓ verified by ${KEYLESS_IDENTITY}
[4/4] Caching to ~/.cache/aegis/models/<digest>/  ✓
```

If verification fails, the model is **not** added to the library and
a clear error surfaces. There is no "I trust this anyway" override.

**Removing a model:** purges from the local cache and the library
view. Does not affect any in-progress session that loaded the model
(the running session keeps the file via the cache symlink contract
in `examples/01-hello-world/setup.sh`-style staging).

**No file uploads.** The browser's `<input type="file">` element does
not appear anywhere in the Model Library. Operators who want to add
a model that isn't on a public registry first publish it to their
internal OCI registry (per [docs/MODEL_MIRRORING.md](../MODEL_MIRRORING.md))
and then paste that ref into the UI.

### Session Forking — switching models in chat

When the operator picks a different model from the chat dropdown:

1. **End current session.** UI emits a synthetic "session boundary"
   message ("Switching from `gemma-4-e4b-it@sha256:de89…` to
   `qwen-2.5-1.5b-instruct@sha256:c740…`"). The current
   `Session::run_turn` loop runs to completion if a turn is in
   flight; no in-flight turn is interrupted. The F9 ledger emits
   its `session_end` entry as normal.
2. **Boot new session.** A fresh `Session` is created with the new
   model digest. F1 mints a new SVID for the new identity triple.
   The F9 ledger starts a new chain anchored to the new digests.
3. **Replay chat history as context.** The new session's first turn
   receives the prior session's chat history as part of its prompt
   context (operator-visible, the same way the user types). The
   model has visibility into "we just switched models — here's what
   we discussed previously" without inheriting any of the prior
   session's enforcement state.

Properties:

- The two sessions share **no** identity, **no** manifest binding,
  **no** F9 chain, **no** F3 grants ([ADR-029](029-task-scoped-ephemeral-approval-grants.md))
  — by design.
- The two sessions share **the chat history (text only)** as a
  prompt input — by design, for UX continuity.
- An auditor inspecting either session's ledger sees a clean
  beginning-to-end record. Cross-session links are inferred from the
  UI's metadata (timestamps, the synthetic boundary message), not
  from the cryptographic chain.

### Backend auto-detection

The WebUI inspects the OCI artifact's media type or annotations to
choose the backend:

| Annotation / format | Backend | Required CLI feature |
|---|---|---|
| `org.opencontainers.image.title=*.gguf` (and llama-style cosign sig) | llama (per [ADR-014](014-cpu-first-gguf-inference-via-llama-cpp.md)) | `--features llama` |
| `*.litertlm` artifact, paired chat-template digest | litertlm (per [ADR-023](023-litertlm-as-second-inference-backend.md)) | `--features litertlm` |

If the running `aegis` binary lacks the required feature (built with
`--features llama` only, asked to load a `.litertlm`), the UI
surfaces the rebuild hint:

```text
This model requires the LiteRT-LM backend. Rebuild aegis with:
  cargo install --locked --path crates/cli --features "llama litertlm" --force
```

The detection is the same logic the CLI uses; the UI is
defense-in-depth for a confused user.

## Why not the alternatives

- **Drop-a-file model upload.** Defeats the entire OCI/cosign trust
  chain. Rejected unconditionally. Internal/private models go
  through the operator's own OCI registry per
  [MODEL_MIRRORING.md](../MODEL_MIRRORING.md).
- **Mid-session hot-swap.** Cryptographically incoherent — the F9
  ledger and F1 SVID are bound to the model digest at boot. Even if
  we tolerated re-binding mid-session, the chat history would carry
  reasoning produced by a model that's no longer running, with no
  way to verify which turn was authored by which model. Rejected.
- **Soft model switch (continue chain, just update digest field).**
  Same problem at lower volume. Rejected.
- **Restart from scratch on switch (lose chat history).** Honest
  but bad UX. Operators would learn to avoid switching.
  Session Forking with replayed-history is the compromise that
  preserves both UX and chain integrity.
- **Auto-pull on first use without confirmation.** Operators might
  paste a malicious OCI ref by accident. Show the verification step
  explicitly so the operator sees who signed the artifact before it
  enters their cache.

## Implementation tracking

- UI: Model Library page in `ui/src/pages/Models.tsx`.
- Backend: `crates/ui-server/src/handlers/models.rs` wraps
  `crates/cli/src/pull.rs` exposing streaming progress over
  WebSocket. No new auth / verification logic — reuses pull.rs
  exactly.
- Session forking: `crates/ui-server/src/handlers/sessions.rs`
  implements the end-current → boot-new → replay-history sequence.
  `crates/inference-engine/src/session.rs` gains no new public API
  (the existing `Session::new` + `Session::shutdown` cover this);
  the orchestration is in the UI server.
- Tracking issue: see v0.9.5 milestone tracker.

## Open questions for follow-up

- **Replay-history token budget.** A long chat history may not fit
  in the new model's context window (e.g., switching from a 32K
  Gemma 4 to an 8K Qwen). The UI summarizes/truncates older turns
  with a visible boundary marker. Strategy: keep last N turns in
  full, summarize older ones via a non-tool summarization pass,
  surface the truncation so the operator can manually re-send key
  context if needed.
- **OCI registry auth.** Operators using private GHCR / ECR /
  Artifactory need to provide credentials. Reuse the operator's
  existing `~/.docker/config.json` / `oras login` state — the UI's
  pull wrapper is just `oras` underneath. Document this rather than
  build credential UI.
- **Pre-cached models.** If a model is already in
  `~/.cache/aegis/models/`, the Library should detect it without
  re-running `aegis pull`. Trade-off: a malicious local process
  could plant a "verified" cache directory. Resolution: list cached
  models but don't mark them "verified" unless the runtime can
  re-verify the cosign signature against the cache's stored
  signature artifact.

## References

- [ADR-013](013-oci-artifacts-for-model-distribution.md) OCI artifacts
- [ADR-021](021-huggingface-as-upstream-oci-as-trust-boundary.md) HF as upstream
- [ADR-022](022-trust-boundary-format-agnosticism.md) Trust boundary
- [ADR-023](023-litertlm-as-second-inference-backend.md) LiteRT-LM
- [ADR-031](031-community-webui-for-local-collaboration.md) Community UI baseline
- [docs/MODEL_MIRRORING.md](../MODEL_MIRRORING.md) operator workflow
- `crates/cli/src/pull.rs` (the existing pull implementation that the
  UI wraps)
