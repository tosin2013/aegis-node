# Aegis-Node Demo Program

Six recorded scenarios that exercise F1–F10 against real CPU-bound model
output. Per [ADR-020](../docs/adrs/020-recorded-demo-program.md) and
[issue #73](https://github.com/tosin2013/aegis-node/issues/73).

Each demo is a `.tape` file (the [VHS](https://github.com/charmbracelet/vhs)
recording script) plus the manifest, prompt, and expected GIF it
produces. `make -C demos all` re-renders every demo from source; the
CI snapshot test re-renders one demo per PR and asserts SHA-256
against the committed expected GIF, so any drift fails loud.

## Why demos exist

The Aegis-Node thesis — "the only AI agent runtime designed to pass a
zero-trust infrastructure review" — is most credible when an outsider
can *watch the policy work* in under a minute. A 60-second clip of the
runtime denying a real model's attempt to read `/etc/passwd` (and the
violation entry naming the layer that rejected it) wins meetings the
README can only hint at.

## Six scenarios

| # | Scenario | Maps to | Notes |
|---|---|---|---|
| 1 | MCP, sandboxed twice | F2 (incl. ADR-018) | Lead. Manifest at [`schemas/manifest/v1/examples/agent-with-mcp.manifest.yaml`](../schemas/manifest/v1/examples/agent-with-mcp.manifest.yaml). |
| 2 | Read-only research assistant | F1 + F2 + F4 + F9 | Canonical "agent reads docs, cannot write." |
| 3 | Code review agent with time-bounded write | F2 exec_grant + F7 time-bounded | Includes deliberate clock skip showing post-expiry Deny. |
| 4 | Customer-support agent with approval gate | F2 + F3 + F4 | TTY/web channel pre-approves before write. |
| 5 | Tampered model halts session | F1 IdentityRebind | Short, dramatic, single turn. |
| 6 | Egress containment | F6 | Deny + signed network attestation entry. |

Per ADR-020, demos do **not** gate on v1.0.0 — they ship in v0.9.x or
early v1.0.0 alongside the marketing-site refresh.

## Hard requirements (per ADR-020 + issue #73)

- All demos use **Qwen2.5-1.5B-Instruct Q4_K_M** (the OCI artifact
  pinned in ADR-020 §"Pinned model"; pulled via `aegis pull`).
- Every demo manifest sets:

  ```yaml
  inference:
    determinism:
      seed: 42
      temperature: 0.0
      top_p: 1.0
      top_k: 0
      repeat_penalty: 1.0
  ```

  Per LLM-C ([#72](https://github.com/tosin2013/aegis-node/issues/72)):
  `seed=42` + `temperature=0.0` produces byte-identical output across
  runs, which is what `make demos` regeneration depends on.
- No staged scripts. Real model output drives every F5 `ReasoningStep`
  ledger entry.
- The CLI binary the `.tape` invokes must be built with the `llama`
  feature: `cargo install --path crates/cli --features llama`.

## Layout

```
demos/
├── README.md                 ← you are here
├── .gitattributes            ← marks *.gif, *.cast, *.png, *.mp4 as binary
├── Makefile                  ← `make all` re-renders every demo
└── <NN>-<scenario-name>/
    ├── manifest.yaml         ← Permission Manifest (per ADR-004)
    ├── prompt.txt            ← user message fed via `aegis run --prompt`
    ├── demo.tape             ← VHS recording script
    ├── demo.gif              ← expected output (binary; CI verifies SHA-256)
    └── README.md             ← what this demo demonstrates + how to read it
```

The `_template/` directory carries the same structure with placeholder
content — copy it and rename to start a new demo.

## Running locally

Prerequisites:

1. **Pull the model** via `aegis pull` (per ADR-013 / OCI-A):

   ```bash
   aegis pull \
     ghcr.io/tosin2013/aegis-node-models/qwen2.5-1.5b-instruct-q4_k_m@sha256:c7404a910e65596a185e788ede19e09bc017dc3101cd106ba7d65fe1dd7dec37 \
     --keyless-identity '^https://github\.com/tosin2013/aegis-node/\.github/workflows/models-publish\.yml@.*$' \
     --keyless-oidc-issuer 'https://token.actions.githubusercontent.com'
   ```

2. **Install VHS** (Charm — terminal recording, the de-facto standard
   for terminal GIFs):

   ```bash
   go install github.com/charmbracelet/vhs@latest
   ```

3. **Build `aegis` with the `llama` feature**:

   ```bash
   cargo install --path crates/cli --features llama
   ```

4. **Render**:

   ```bash
   make -C demos all          # every demo
   make -C demos 01           # just demo 01
   ```

The `Makefile` exposes one target per demo directory plus an `all`
target that rebuilds everything in parallel.

## CI integration

`.github/workflows/demos.yml` (lands with the first demo PR — see
issue #73's decomposition) installs VHS + the CLI, re-renders one
demo per PR (rotated by directory ordering), and asserts the rendered
GIF's SHA-256 matches the committed expected GIF. Any drift fails the
job; the PR author either accepts the new GIF (re-running locally
and committing the update) or fixes the underlying determinism
regression.
