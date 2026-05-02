# Demo 02 — Read-only research assistant

Per [ADR-020](../../docs/adrs/020-recorded-demo-program.md) §"Six
scenarios" item 2. The canonical "agent reads docs, cannot write"
narrative — the agent gets read access to a single directory plus
deny-by-default network, and the closed-by-default manifest
refuses everything else.

## What this demonstrates

The agent (Gemma 4 E4B) is given exactly enough surface to do its
job: read `/data` and produce a summary. Every other capability
is closed-by-default at the manifest layer.

| Layer | Behavior in this demo |
|---|---|
| **F2 Permission Manifest** | `tools.filesystem.read: ["/data"]` is the only I/O grant. No `tools.filesystem.write`, no `tools.network.outbound: allow`, no `exec_grants`, no `tools.mcp[]`. Any attempt at writing, networking, or MCP dispatch lands as a Violation. |
| **F4 Access Log** | The agent's read of `/data/research-notes.txt` lands as an Access entry. Auditor sees what the agent touched, byte-for-byte. |
| **F6 Network Deny-by-Default** | Closed outbound + inbound. The session's NetworkAttestation at shutdown summarizes zero connections (`totalConnections: 0`) — proof that the agent never even tried to phone home. |
| **F9 Hash-chained ledger** | All entries chain into a tamper-evident root; `aegis verify` confirms. |

## What you see in the GIF

| t | Frame | Note |
|---|---|---|
| 0–4s | `grep -A4 'tools:'` showing read-only posture | One read grant, no write/network/exec |
| 4–28s | `aegis run --backend litertlm --prompt "..."` runs Gemma 4 E4B | E4B is ~3× the size of E2B; CPU inference budget is ~25s |
| 28–32s | `grep access` shows the F4 Access entry | `accessType: read`, `resourceUri: file:///data/research-notes.txt` |
| 32–36s | `grep network_attestation` shows zero connections | `totalConnections: 0` |
| 36–40s | `aegis verify` confirms the chain | Tamper-evident, root hash matches |

## Run locally

```bash
# 1) Pull the cosign-verified Gemma 4 E4B artifact
aegis pull \
  ghcr.io/tosin2013/aegis-node-models/gemma-4-e4b-it@sha256:de89d03b650a86410d1c9f48ee2239fdf7d5f8895ad00621e20b9c2ed195f931 \
  --keyless-identity '^https://github\.com/tosin2013/aegis-node/\.github/workflows/models-publish\.yml@.*$' \
  --keyless-oidc-issuer 'https://token.actions.githubusercontent.com'

# 2) Stage the demo workdir
mkdir -p /tmp/aegis-demo-02 /data
cp ~/.cache/aegis/models/<gemma-4-e4b-blob-sha>/blob.bin \
   /tmp/aegis-demo-02/model.litertlm
ln -sf ~/.cache/aegis/models/<gemma-4-e4b-blob-sha>/chat_template.sha256.txt \
   /tmp/aegis-demo-02/chat_template.sha256.txt
echo "Q3 2025: revenue $147M, EBITDA $42M, headcount 380." > /data/research-notes.txt

# 3) Render
make -C demos 02-read-only-research
```

## Glibc requirement

The LiteRT-LM runtime (`libaegis_litertlm_engine_cpu.so`, per
[ADR-023](../../docs/adrs/023-litertlm-as-second-inference-backend.md))
is built against ubuntu-24.04 and requires **glibc 2.38+**. Render
on:

- A native ubuntu-24.04+ host, OR
- A docker container based on ubuntu:24.04, OR
- The `litertlm.yml` CI workflow (which runs on `ubuntu-24.04`).

Ubuntu 22.04 hosts (glibc 2.35) cannot load the `.so` directly.
Pre-LiteRT demos (5, 6) used the llama.cpp backend which has no
such requirement.

## Reproducibility

Per ADR-020 hard requirements, `manifest.yaml` pins
`inference.determinism` (seed 42 + temperature 0). With the same
prompt + the same Gemma 4 E4B blob, every render produces
byte-identical text output.

## Why Gemma 4 E4B (not Qwen 1.5B)

Per the per-demo model selection table in ADR-020 §"Decision" item 8,
this demo runs against **Gemma 4 E4B**:

- The narrative payoff is the *summary itself* — bigger model =
  more useful output, which sells the "read-only research
  assistant" pitch better.
- The 4B model + Apache-2.0/Gemma-Terms licensing matters for
  enterprise audiences who can't (or won't) use Chinese-origin
  weights.
- E4B is ~3× the inference time of E2B, but the per-turn budget
  (~25s) still fits within the ~30s ADR-020 GIF target.

E2B (`sha256:365c6a8b...`) is a drop-in replacement for hosts
that can't budget the larger model.
