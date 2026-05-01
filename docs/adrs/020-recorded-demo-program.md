# 20. Recorded Demo Program — VHS Tapes Driven by Real CPU-Bound Models

**Status:** Accepted
**Date:** 2026-04-29
**Domain:** Developer Relations / Marketing Artifacts / Reproducibility (extends [ADR-014](014-cpu-first-gguf-inference-via-llama-cpp.md))

## Context

Aegis-Node's product story — "the agent runtime built to survive the security
review" — is most credible when an outsider can *watch the policy work* in
under a minute. A static README and a list of features get past the first
filter; a 60-second clip of the runtime denying a real model's attempt to
read `/etc/passwd` (and the violation entry naming the layer that rejected
it) is what actually wins meetings.

Up to v0.8.0 we have no canonical demo set. Anyone evaluating the project has
to trust the docs or build a fixture themselves. v0.9.0 lands the last big
runtime piece (llama.cpp FFI per ADR-014) — the natural moment to commit to
a demo program and have the tooling on hand to record it.

Three separate decisions have been compressed into this ADR because they
co-evolve: which scenarios to record, how to record them reproducibly, and
which model drives the agent during recording. The third decision is the
hardest in our setting: ADR-014 commits Phase 1 to **CPU-only** inference,
which caps the model size we can use without the recording feeling slow.

## Decision

1. **Aegis-Node ships a canonical demo program** — six recorded scenarios
   that each map to one or more F1–F10 questions. They live under
   `demos/<scenario>/` in this repo and are buildable from source, not
   committed video files alone. The program is a maintained artifact:
   regenerable from the same fixtures CI uses, and re-recorded when the
   runtime evolves.

2. **Recording standard is [VHS](https://github.com/charmbracelet/vhs)**
   (Charm). Each demo is a `.tape` file plus the manifests / scripts /
   prompts it drives. VHS produces a deterministic terminal recording
   given a deterministic terminal program — which the next decision
   ensures.

3. **Demos are driven by a real model, not a hand-rolled tool-call script.**
   The whole point of the F5 reasoning trajectory is the *authentic*
   chain from "input → reasoning → tool selection → access" — staged
   scripts can't show that, and a 30-second clip of fake reasoning is
   worse than no demo. Demos therefore depend on the llama.cpp FFI
   from ADR-014 / issues [#70](https://github.com/tosin2013/aegis-node/issues/70)
   (FFI), [#71](https://github.com/tosin2013/aegis-node/issues/71) (Backend
   trait), and [#72](https://github.com/tosin2013/aegis-node/issues/72)
   (determinism knobs). Demos do **not** ship before those land.

4. **Pinned model: Qwen2.5-1.5B-Instruct Q4_K_M.** The CPU-only
   constraint (ADR-014) is the binding constraint on this choice.
   Measured CPU performance for tool-calling-capable instruct models in
   Q4_K_M:

   | Model | Size | tok/s (CPU, llama.cpp) | 300-tok demo turn |
   |---|---|---|---|
   | Qwen2.5-0.5B | ~400 MB | 50–80 | ~5 s |
   | TinyLlama 1.1B | ~640 MB | 30–50 | ~7 s |
   | **Qwen2.5-1.5B-Instruct (pinned)** | **~900 MB** | **15–25** | **~15 s** |
   | Qwen2.5-3B | ~1.8 GB | 7–12 | ~30 s |
   | Hermes 2 Pro Llama 3 8B | ~4.5 GB | 3–5 | 60–100 s |

   Qwen2.5-1.5B is the smallest model that reliably calls tools by
   schema while staying snappy on a single CPU socket. Larger models are
   used only for recordings where the per-turn budget allows it; the
   default in `demos/` references 1.5B.

   **Published OCI artifact** (per [ADR-021](021-huggingface-as-upstream-oci-as-trust-boundary.md)):

   - **Reference:** `ghcr.io/tosin2013/aegis-node-models/qwen2.5-1.5b-instruct-q4_k_m:latest`
   - **Digest:** `sha256:c7404a910e65596a185e788ede19e09bc017dc3101cd106ba7d65fe1dd7dec37`
   - **Blob SHA-256:** `6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e`
   - **Upstream:** [Qwen/Qwen2.5-1.5B-Instruct-GGUF](https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF) · file `qwen2.5-1.5b-instruct-q4_k_m.gguf` · revision `91cad51170dc346986eccefdc2dd33a9da36ead9`
   - **Signed by:** `models-publish.yml` workflow ([run 25172111227](https://github.com/tosin2013/aegis-node/actions/runs/25172111227)) via Sigstore keyless. Carries the `dev.aegis-node.chat-template.sha256=d5495a1e...` annotation per [ADR-022](022-trust-boundary-format-agnosticism.md). The original publish (run 25135210278, digest `sha256:240ece32...`) predates ADR-022 and is no longer the operator pin.

   Demos pull this artifact at boot via:

   ```bash
   aegis pull ghcr.io/tosin2013/aegis-node-models/qwen2.5-1.5b-instruct-q4_k_m@sha256:c7404a910e65596a185e788ede19e09bc017dc3101cd106ba7d65fe1dd7dec37 \
     --keyless-identity '^https://github\.com/tosin2013/aegis-node/\.github/workflows/models-publish\.yml@.*$' \
     --keyless-oidc-issuer 'https://token.actions.githubusercontent.com'
   ```

5. **Recording determinism is mandatory, not optional.** Every demo
   manifest sets the LLM-C determinism knobs:

   ```yaml
   inference:
     determinism:
       seed: 42
       temperature: 0.0
       top_p: 1.0
       top_k: 0
       repeat_penalty: 1.0
   ```

   A demo that doesn't pin determinism is not accepted into the program;
   the whole point is that anyone running `make demos` regenerates the
   same `.gif` byte-for-byte (modulo the GIF encoder, which VHS pins).
   This elevates LLM-C from P1 to P0 — without it, the program is not
   reproducible.

6. **The six scenarios** (ordered for marketing impact, not technical
   complexity):

   1. **MCP, sandboxed twice** — Anthropic filesystem MCP server +
      `tools.mcp[]` allowlist + `tools.filesystem.read` syscall gate.
      Three calls: allowed, MCP-permitted-but-syscall-denied, MCP-denied.
      Maps to F2 (incl. ADR-018 MCP). **Lead demo** — uniquely
      Aegis-Node, hottest topic in agentic AI.
   2. **Read-only research assistant** — canonical "agent reads docs,
      cannot write." Maps to F1 + F2 + F4 + F9.
   3. **Code review agent with time-bounded write** — F2 exec_grant
      runs `git diff`, F7 time-bounded `write_grant` permits review
      notes for one hour; the recording includes a deliberate clock
      skip showing the post-expiry Deny entry.
   4. **Customer-support agent with approval gate** — F2 + F3 + F4 with
      a TTY/web channel pre-approving before the write lands.
   5. **Tampered model halts session** — swap the model bytes mid-
      session; F1 IdentityRebind violation + halt within one turn.
      Visceral, ~30 s clip.
   6. **Egress containment** — agent attempts to phone home; F6 deny +
      end-of-session signed network attestation entry.

7. **Demos do not gate on v1.0.0.** They land as a parked tracking
   issue (no milestone) that becomes actionable once #70 / #71 / #72
   close. The ADR is the long-lived artifact; the recordings ship in
   v0.9.x or early v1.0.0 alongside the marketing site refresh.

8. **Model selection per demo** (amended 2026-05-01 alongside
   [ADR-023](023-litertlm-as-second-inference-backend.md)). The
   pinned-Qwen decision in item 4 above is no longer one-size-fits-
   all once the LiteRT-LM second backend lands ([#98](https://github.com/tosin2013/aegis-node/issues/98)).
   Each demo picks the model whose strengths match its narrative:

   | # | Demo | Pinned model | Backend | Reason |
   |---|---|---|---|---|
   | 1 | MCP, sandboxed twice | Qwen2.5-1.5B-Q4_K_M | llama | (blocked on [#91](https://github.com/tosin2013/aegis-node/issues/91); choice revisited when unblocked) |
   | 2 | Read-only research | **Gemma 4 E4B** | **litertlm** | Bigger model produces more useful "summarize" output; enterprise-clean origin matters for the canonical "agent reads docs" pitch |
   | 3 | Code review + time-bounded write | **Gemma 4 E4B** | **litertlm** | Code understanding + agentic tool calls favor a 4B Western-origin model; F7 timer demo leans on credible review output |
   | 4 | Customer-support + approval gate | **Gemma 4 E2B** | **litertlm** | Customer-support narrative reads better with realistic agent voice; E2B keeps the ~30-second budget |
   | 5 | **Tampered model halts** | **Qwen2.5-1.5B-Q4_K_M** | **llama** | Mechanical F1 IdentityRebind demo. Model quality irrelevant; Qwen 1.5B is faster (3-second turn) and the existing GIF (PR [#93](https://github.com/tosin2013/aegis-node/pull/93)) works. Don't re-render. |
   | 6 | Egress containment | Qwen2.5-1.5B-Q4_K_M | llama | F6 deny-by-default is mechanical; same reasoning as Demo 5 |
   | **7** | **Multimodal vision agent** ([#100](https://github.com/tosin2013/aegis-node/issues/100)) | **Gemma 4 E4B (multimodal)** | **litertlm** | Genuinely new capability — agent reads a screenshot, F2 + F6 gate the side-effects. No competing runtime can show this. |

   Mechanical demos (5, 6) stay on the original Qwen pin so re-renders are
   cheap and `make demos` reproducibility is preserved through the LiteRT
   transition. Demos that lean on agent reasoning quality (2, 3, 4) move
   to Gemma 4 once the LiteRT-LM pipeline is live. Demo 7 is the program's
   first multimodal scenario and depends on multipart `ChatMessage` +
   `aegis run --image` plumbing tracked in [#100](https://github.com/tosin2013/aegis-node/issues/100).

   The published OCI artifact pins for both reference models are
   recorded below. Qwen for the mechanical demos (5, 6); Gemma 4 for
   demos that lean on agent reasoning quality (2, 3, 4) plus the
   multimodal Demo 7.

   **Published OCI artifact — Gemma 4 E2B (LiteRT-LM)** (per
   [ADR-021](021-huggingface-as-upstream-oci-as-trust-boundary.md) §"License scope"
   amendment + [ADR-023](023-litertlm-as-second-inference-backend.md)):

   - **Reference:** `ghcr.io/tosin2013/aegis-node-models/gemma-4-e2b-it:latest`
   - **Manifest digest:** `sha256:365c6a8b3b226ec825b74ed16404515ec61521b2d7f24490eac672d74466b2ea`
   - **Blob SHA-256:** `ab7838cdfc8f77e54d8ca45eadceb20452d9f01e4bfade03e5dce27911b27e42` (~2.58 GB)
   - **Chat-template SHA-256:** `02b3091acf53c0b722e3db0c7a1b4980363edcc2d85549dafa339ff5dbfff629` (11995 bytes — sibling `chat_template.jinja` from the HF repo, the canonical jinja prompt template the LiteRT-LM runtime uses)
   - **Upstream:** [`litert-community/gemma-4-E2B-it-litert-lm`](https://huggingface.co/litert-community/gemma-4-E2B-it-litert-lm) · file `gemma-4-E2B-it.litertlm` · revision `84b6978eff6e4eea02825bc2ee4ea48579f13109`
   - **License:** Gemma Terms of Use (per ADR-021 §"License scope" — permissive with Google's prohibited-use list at the linked HF page)
   - **Backend:** LiteRT-LM (`aegis run --backend litertlm`)
   - **Signed by:** `models-publish.yml` workflow ([run 25235731230](https://github.com/tosin2013/aegis-node/actions/runs/25235731230)) via Sigstore keyless. Carries `dev.aegis-node.chat-template.sha256` per [ADR-022](022-trust-boundary-format-agnosticism.md).

   Demos pull this artifact at boot via:

   ```bash
   aegis pull ghcr.io/tosin2013/aegis-node-models/gemma-4-e2b-it@sha256:365c6a8b3b226ec825b74ed16404515ec61521b2d7f24490eac672d74466b2ea \
     --keyless-identity '^https://github\.com/tosin2013/aegis-node/\.github/workflows/models-publish\.yml@.*$' \
     --keyless-oidc-issuer 'https://token.actions.githubusercontent.com'
   ```

   E4B (`gemma-4-E4B-it.litertlm`, ~3.7 GB) is published the same way
   when a demo's per-turn budget can absorb the larger model — same
   workflow inputs, just a different `litertlm_filename`.

## Why these decisions

- **Why VHS specifically.** VHS is the de-facto standard for terminal
  GIFs in 2026, ships as a single static binary, deterministic given a
  deterministic terminal program, and integrates with `Sleep` / `Type` /
  `Enter` directives so a `.tape` reads as a script reviewer can audit.
  Asciinema produces `.cast` files that need a player; VHS produces a
  GIF that drops into a README. The marketing motion is GIFs.
- **Why one ADR for six demos.** The five-vs-six demos were debated
  during planning; the conclusion was that the ADR records the demo
  *program* (the architectural decision to ship one), not each
  scenario. Adding or removing a scenario is content evolution, not a
  new ADR.
- **Why real models on CPU and not staged scripts.** The F5 reasoning
  trajectory is the only feature in the F1–F10 set that no competitor
  has at all. Staged scripts can't demonstrate it convincingly. CPU-
  bound demos are slower than what GPU-backed runs would produce, but
  pacing is solvable by VHS (`@10x` speedup over quiet stretches and
  honest pacing during the policy decisions); fake reasoning is not
  solvable.
- **Why Qwen2.5-1.5B and not the smaller 0.5B class.** Tool-calling
  reliability degrades sharply below 1B at Q4. With `seed=42` we can
  pick prompts the 0.5B class always handles correctly, but that
  selection bias risks demos that don't survive a curious viewer trying
  the same prompts. 1.5B is the smallest size where the demo is robust
  to prompt variation.
- **Why elevate LLM-C determinism to P0 for demos but not for the
  runtime.** The runtime is fine without determinism — many production
  agents want stochastic behavior. Demos cannot survive without it; a
  GIF that rerenders differently every commit is not a demo, it's a
  liability.

## Consequences

### Positive

- Marketing materials are *regenerable from source* — a future
  contributor can reproduce them and verify nothing has been edited
  to overstate the runtime's behavior. This matches Aegis-Node's audit
  posture: the demos are auditable artifacts, not glossy renders.
- The demo program forces the F5 + F4 + F2 + ledger + replay viewer
  story to compose convincingly end-to-end, before v1.0.0 GA. If the
  recording feels weird or thin, that's a runtime smell to fix, not a
  demo problem.
- Producing the recordings requires LLM-A / LLM-B / LLM-C to actually
  work at the user-visible level — the program is a forcing function on
  the inference stack.

### Negative

- Demos slip onto the critical path for v0.9.x — they cannot be
  recorded until #70 / #71 / #72 ship, and Qwen2.5-1.5B-Instruct adds
  a download step to the demo build (~900 MB). We mitigate the second
  point by hosting the model as a Cosign-signed OCI artifact (per ADR-
  013), so the demo build re-uses `aegis pull` and double-counts as
  a real OCI-A / OCI-B exercise.
- LLM-C is now load-bearing for marketing, not just compliance. A
  bug in determinism breaks the demo program. We mitigate by requiring
  determinism to ship with a regression test that re-renders one demo
  in CI and asserts the output GIF's SHA-256 against a checked-in
  expected value.
- Demo bitrate is bounded by CPU speed. Bigger models would tell a
  better story but break the recording flow. We accept this and revisit
  in v2.0.0 when GPU backends land (ADR-015 Phase 2).

## Implementation plan

The implementation is parked behind LLM-A/B/C; the schedule below is
*after* those land:

1. **`demos/` scaffold** — README, build script (`make demos` →
   regenerates every `.gif`), `.gitattributes` so binaries are
   tracked correctly, and a sample `.tape` that exercises one boot.
2. **Demo 1 (MCP, sandboxed twice)** — first because it's the lead and
   the manifest already exists at
   `schemas/manifest/v1/examples/agent-with-mcp.manifest.yaml`.
3. **Demo 5 (Tampered model halts)** — short, dramatic, single turn.
   Good second to ship; tests the determinism-and-CI flow on a small
   fixture.
4. **Demos 2–4, 6** — order TBD by which f-feature documentation lands
   alongside.
5. **CI snapshot test** — re-renders one demo per PR and asserts SHA
   match against the checked-in expected GIF. Fast (one demo only),
   catches accidental drift.
6. **Marketing-site lift** — embed the GIFs into the project README and
   the release notes for v0.9.x / v1.0.0.

## Related

- [ADR-014 CPU-First GGUF Inference via llama.cpp](014-cpu-first-gguf-inference-via-llama-cpp.md)
- [ADR-013 OCI Artifacts for Model Distribution](013-oci-artifacts-for-model-distribution.md) — model distribution for the demo fixture
- [ADR-015 Three-Phase Deployment Roadmap](015-three-phase-deployment-roadmap.md) — Phase 2 GPU backend will eventually unlock larger demo models
- LLM-A [#70](https://github.com/tosin2013/aegis-node/issues/70) Rust FFI to llama.cpp
- LLM-B [#71](https://github.com/tosin2013/aegis-node/issues/71) Backend trait + LlamaCppBackend
- LLM-C [#72](https://github.com/tosin2013/aegis-node/issues/72) Determinism knobs
