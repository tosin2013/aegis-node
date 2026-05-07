# Chat Surface — Operator Guide

The Phase 1d Community UI's chat surface (`/chat` route) drives a real
`Session::run_turn` against a loaded inference backend per
[ADR-031](adrs/031-community-webui-for-local-collaboration.md). Every
turn passes through the runtime's F1–F10 enforcement and writes
verifiable F9 ledger entries; the SPA renders inline tool-call cards
with the manifest's gate decision and a verifiable badge anchored
to the F5 reasoning-step UUID.

This document covers:

- Prerequisites + first-run setup
- Running with each inference backend (llama.cpp, LiteRT-LM)
- **Switching between backends** (current procedure + the dropdown UI
  that lands in [#153](https://github.com/tosin2013/aegis-node/issues/153) /
  sub-phase 1d.2e)
- Known limitations for v0.9.5

## Prerequisites

Three things, all one-time:

### 1. Identity CA

```bash
aegis identity init --trust-domain aegis-node.local
```

Creates `~/.config/aegis/identity/{ca.crt,ca.key}` — the local SPIFFE
CA per [ADR-003](adrs/003-cryptographic-workload-identity-spiffe-spire.md).
Skip if already done; `aegis identity init` is idempotent against an
existing CA but will refuse to overwrite a different trust-domain.

### 2. A pulled model artifact

```bash
# Qwen2.5-1.5B-Instruct (llama.cpp / GGUF, per ADR-014):
aegis pull ghcr.io/tosin2013/aegis-node-models/qwen2.5-1.5b-instruct-q4_k_m@sha256:c7404a910e65596a185e788ede19e09bc017dc3101cd106ba7d65fe1dd7dec37

# Gemma-4-E2B-IT (LiteRT-LM, per ADR-023):
aegis pull ghcr.io/tosin2013/aegis-node-models/gemma-4-e2b-it@sha256:365c6a8b3b226ec825b74ed16404515ec61521b2d7f24490eac672d74466b2ea

# Gemma-4-E4B-IT (LiteRT-LM, larger):
aegis pull ghcr.io/tosin2013/aegis-node-models/gemma-4-e4b-it@sha256:de89d03b650a86410d1c9f48ee2239fdf7d5f8895ad00621e20b9c2ed195f931
```

`aegis pull` writes to `~/.cache/aegis/models/<digest>/blob.bin` after
cosign verification. The digest sub-directory is what you pass to
`--model`.

### 3. A manifest

The manifest gates what the agent can do per
[ADR-004](adrs/004-declarative-yaml-permission-manifest.md). For a
first chat smoke test, `examples/01-hello-world/manifest.yaml` works
out of the box. For more interesting demos with tool-call cards,
fork `examples/02-mcp-research-assistant/manifest.yaml` (filesystem
MCP) or `examples/06-mcp-finance-sqlite/manifest.yaml` (SQLite MCP).

## Build feature flags

The CLI gates inference backends behind Cargo features so workspace-wide
`cargo build` doesn't pay the C++ build cost:

| Feature | Backend | Models | Build cost |
|---|---|---|---|
| `--features llama` | llama.cpp | GGUF (Qwen, Llama, Mistral, …) | ~50s cold (cmake + bindgen) |
| `--features litertlm` | LiteRT-LM | `.litertlm` (Gemma 4 family) | ~10s (oras pulls a prebuilt `.so`) |
| `--features "llama litertlm"` | both | both | union of the two |

Build whichever you need:

```bash
# Most operators want llama only — Qwen/Llama/Mistral cover most use cases.
cargo install --locked --path crates/cli --features llama --force

# To run Gemma 4 (LiteRT-LM family).
cargo install --locked --path crates/cli --features litertlm --force

# Both backends in one binary.
cargo install --locked --path crates/cli --features "llama litertlm" --force
```

> **Glibc requirement for LiteRT-LM**: the upstream prebuilt
> `libaegis_litertlm_engine_cpu.so` requires **glibc ≥ 2.38** and
> **libstdc++ from GCC ≥ 13.2** (provides `GLIBCXX_3.4.31`). This is
> Ubuntu 24.04 / Debian Trixie / RHEL 10 territory. Older distros
> (Ubuntu 22.04, RHEL 9, Debian Bookworm) will fail at link time
> with `undefined reference to __isoc23_strtoull@GLIBC_2.38`. CI's
> `litertlm.yml` job runs on `ubuntu-24.04` for this reason.

## Running the chat surface

Start the UI with explicit `--manifest` + `--model` + `--backend`:

### Qwen 2.5 (llama.cpp)

```bash
aegis ui \
  --listen 127.0.0.1:7777 \
  --manifest examples/01-hello-world/manifest.yaml \
  --model ~/.cache/aegis/models/c7404a910e65596a185e788ede19e09bc017dc3101cd106ba7d65fe1dd7dec37/blob.bin \
  --backend llama \
  --workload hello-world \
  --instance inst-001 \
  --ledger /tmp/aegis-ui-chat.jsonl
```

### Gemma 4 (LiteRT-LM)

```bash
aegis ui \
  --listen 127.0.0.1:7777 \
  --manifest examples/01-hello-world/manifest.yaml \
  --model ~/.cache/aegis/models/365c6a8b3b226ec825b74ed16404515ec61521b2d7f24490eac672d74466b2ea/blob.bin \
  --backend litertlm \
  --workload hello-world \
  --instance inst-001 \
  --ledger /tmp/aegis-ui-chat.jsonl
```

The `--workload` and `--instance` segments **must match** the SPIFFE ID
in the manifest. For `examples/01-hello-world/manifest.yaml` whose
`identity.spiffeId` is
`spiffe://aegis-node.local/agent/hello-world/inst-001`, that's
`--workload hello-world --instance inst-001`.

After the model loads (5–15s for Qwen 1.5B; 30–60s for Gemma 4
depending on hardware), open `http://127.0.0.1:7777/chat` in your
browser. Each prompt drives `Session::run_turn`; assistant text
streams chunked at ~80 chars / 30 ms; tool calls render as inline
cards with the manifest's gate decision; the verifiable badge below
each turn carries the F5 reasoning-step UUID.

## Switching between backends today (v0.9.5)

The model is bound at process startup — there's **no in-UI dropdown
yet**. To switch from one to the other:

```bash
# 1. Stop the running aegis ui process
pkill -f "aegis ui"   # or Ctrl-C in its terminal

# 2. Verify the previous session's ledger is intact (optional but
#    nice — confirms the F9 chain is closed cleanly before you start
#    the next session against a fresh ledger).
aegis verify /tmp/aegis-ui-chat.jsonl

# 3. Restart with the other backend + model
aegis ui \
  --listen 127.0.0.1:7777 \
  --manifest examples/01-hello-world/manifest.yaml \
  --model <path-to-other-model>/blob.bin \
  --backend <llama|litertlm> \
  --workload hello-world \
  --instance inst-001 \
  --ledger /tmp/aegis-ui-chat-other.jsonl   # fresh ledger per session
```

**Per-session ledgers** keep the F1 binding sane: each session writes
its own JSONL, the chain root hashes to a different value, and
`aegis verify` works on each independently. Mixing two sessions'
entries in one ledger would invalidate the chain.

> **Gotcha**: the ledger writer refuses to open a path that already
> exists (`io: File exists (os error 17)`). This is a safety feature —
> appending to a previous session's ledger would corrupt the F9 chain.
> If you stop and restart `aegis ui` with the same `--ledger` path,
> the second start fails fast with that error. Use a unique path per
> run, or `rm` the old one (only if you don't need the audit trail
> from the previous session).

> **Why no hot-swap dropdown yet:** [ADR-032 §"Session Forking"](adrs/032-webui-model-library-and-session-forking.md)
> specifies the proper UX — graceful end of the current session, boot
> a new session against the new digest triple, replay chat history as
> the new session's prompt context. F1 binds workload identity to
> `(model_digest, manifest_digest, config_digest)` at boot, so a true
> hot-swap would invalidate the F9 chain.
> [#153](https://github.com/tosin2013/aegis-node/issues/153) tracks
> the implementation as sub-phase 1d.2e.

## Known limitations (v0.9.5)

### LiteRT-LM CPU sampler bug (upstream)

Gemma 4 chat output may be **non-deterministic** even with `seed=42 +
temperature=0.0`. The CPU sampler in upstream LiteRT-LM falls back to
baked-default sampler params under some conditions
(LiteRT-LM #2080 / #2081); the deterministic-mode promise from
[ADR-023](adrs/023-litertlm-as-second-inference-backend.md) doesn't
hold reliably until the upstream fix lands. Demos 2/3/4 are excluded
from the snapshot test for the same reason
([#119](https://github.com/tosin2013/aegis-node/issues/119)).

If you see Gemma 4 producing different tokens for the same prompt
between runs, that's why. Switch to Qwen (llama backend) for
reproducible chat output.

### Single model per process

The chat surface holds one Session in `Arc<Mutex<…>>` for the
process's lifetime. To swap models you stop and restart `aegis ui`.
[#153](https://github.com/tosin2013/aegis-node/issues/153) /
1d.2e adds the dropdown + Session Forking endpoint that resolves
this without process restart.

### No system prompts (deliberate)

Operator-supplied system prompts ("you are X assistant…") are **not
exposed** in any v0.9.x sub-phase by design. Per
[#155](https://github.com/tosin2013/aegis-node/issues/155), system
prompts ship in v1.0.0 paired with an
[ADR-028](adrs/028-adversarial-pre-filter-gate.md)-style content
scanner — operator prompts are as risky as attacker-injected ones
(jailbreaks, conflicting role instructions, off-manifest grants
smuggled via the prompt) and shouldn't ship raw.

What you *can* do: type whatever prompt you want in the chat input
itself — that's the user message, not a system preamble.

### No auth (Community tier; deliberate)

The Community UI ships zero auth — no login, no token, no session
cookie — per ADR-031 §"Single agent, single user." The localhost
bind + OS process boundary IS the auth model. Multi-user / SSO /
RBAC is exclusively v2.5.0
[Enterprise tier](adrs/034-enterprise-management-dashboard-and-rbac.md).

## Verifying a chat session after the fact

Every turn writes to the F9 ledger. After a chat session ends:

```bash
aegis verify /tmp/aegis-ui-chat.jsonl
```

The output reports the chain's root hash, entry count, and time
range. The verifiable badge in the SPA carries the F5 reasoning-step
UUID for each turn — once the
[ADR-010 replay viewer](adrs/010-deterministic-trajectory-replay-offline-viewer.md)
gains the `/replay/<anchor>` route, clicking the badge will
navigate to the per-turn replay.

## Related

- [ADR-031](adrs/031-community-webui-for-local-collaboration.md) — Community UI design
- [ADR-032](adrs/032-webui-model-library-and-session-forking.md) — Session Forking + Model Library
- [ADR-014](adrs/014-cpu-first-gguf-inference-via-llama-cpp.md) — llama.cpp backend
- [ADR-023](adrs/023-litertlm-as-second-inference-backend.md) — LiteRT-LM backend
- [docs/plans/v0.9.5-ui-implementation.md](plans/v0.9.5-ui-implementation.md) — Phase 1d implementation plan
- [`#147`](https://github.com/tosin2013/aegis-node/issues/147) — chat surface umbrella tracker
- [`#153`](https://github.com/tosin2013/aegis-node/issues/153) — model picker + Session Forking (1d.2e)
- [`#155`](https://github.com/tosin2013/aegis-node/issues/155) — scanned system prompts (v1.0.0+)
