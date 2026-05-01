# Demo 06 — Egress containment (F6 deny + signed network attestation)

Per [ADR-020](../../docs/adrs/020-recorded-demo-program.md) §"Six
scenarios" item 6. The 30-second clip shows how the network gate
catches an exfiltration attempt — the model emits a `network__connect`
tool call against an external host, the manifest's `tools.network.outbound:
deny` policy refuses it, and the end-of-session **F6 NetworkAttestation**
entry signs a complete record of the attempt for audit.

## What this demonstrates

| Layer | Behavior in this demo |
|---|---|
| **F2 Permission Manifest** | `tools.network.outbound: deny` — closed-by-default outbound. The catalog still advertises `network__connect` so the model knows to attempt; the deny fires at dispatch. |
| **F6 Network Deny-by-Default** | Mediator's `policy.check_network_outbound(host, port, proto)` returns `Deny { reason: "manifest sets deny" }`. F2 Violation entry lands in the ledger naming the resource. |
| **F6 Signed Network Attestation** | At session shutdown, `attestation::emit_network_attestation` walks `Session.network_log` (every connect attempt, regardless of outcome) and emits a single `NetworkAttestation` ledger entry with `allowedCount` / `approvedCount` / `deniedCount` / `totalConnections` + a per-attempt record (host, port, protocol, decision, timestamp) + a `connectionsDigestHex` hash + an HMAC `signatureHex`. |
| **F9 Hash-chained ledger** | Both the F2 Violation and the F6 attestation are tamper-evident; `aegis verify` confirms the chain root. |

## What you see in the GIF

| t | Frame | Note |
|---|---|---|
| 0–2s | `cd` + `grep network` showing the manifest's `outbound: deny` policy | Closed-by-default; this is the rule that drives the deny |
| 2–10s | `aegis run --prompt "..."` launches the model | ~3-second inference; CLI prints the live `# tool[0] network__connect → DENIED: network outbound denied: manifest sets deny` line |
| 10–14s | `grep violationReason` shows the F2 Violation entry | `accessType: network_outbound`, `violationReason: "network outbound denied: manifest sets deny"` |
| 14–22s | `grep network_attestation` shows the F6 signed attestation | `deniedCount: 1`, `totalConnections: 1`, `networkConnectionsObserved: [{host, port, protocol, decision: "denied", timestamp}]`, `signatureHex` |

## Why deny-by-default isn't enough on its own

Every modern security framework rejects "default-allow" outbound; the
interesting bit is what happens when an agent *attempts* to phone
home. F6 isn't just "block the connection" — it's "produce an audit
record that survives the session, signed end-to-end, that the SOC
can ingest into their SIEM and correlate against the F4 Access log."
This demo shows that pipeline working: model → mediator → Violation
entry → attestation summary → `aegis verify` confirms tamper-evident
chain.

## Run locally

```bash
# 1) Pull the cosign-verified Qwen artifact (one-time per machine)
aegis pull \
  ghcr.io/tosin2013/aegis-node-models/qwen2.5-1.5b-instruct-q4_k_m@sha256:c7404a910e65596a185e788ede19e09bc017dc3101cd106ba7d65fe1dd7dec37 \
  --keyless-identity '^https://github\.com/tosin2013/aegis-node/\.github/workflows/models-publish\.yml@.*$' \
  --keyless-oidc-issuer 'https://token.actions.githubusercontent.com'

# 2) Stage the demo workdir
mkdir -p /tmp/aegis-demo-06
cp ~/.cache/aegis/models/c7404a910e65596a185e788ede19e09bc017dc3101cd106ba7d65fe1dd7dec37/blob.bin \
   /tmp/aegis-demo-06/model.gguf

# 3) Render
make -C demos 06-egress-containment
```

The Makefile's `check-tools` step verifies `vhs` and a llama-featured
`aegis` are on PATH before running. If either is missing, follow
[demos/README.md](../README.md) §"Running locally."

## Reproducibility

Per ADR-020 hard requirements, `manifest.yaml` pins
`inference.determinism` (seed 42 + temperature 0). With the same
prompt + the same Qwen blob, every render produces a byte-identical
`demo.gif`. The CI snapshot test (Phase 2b, separate PR) will gate
on the GIF's SHA-256.

## Why Qwen 1.5B (not Gemma 4)

Per the per-demo model selection table in ADR-020 §"Decision" item 8,
mechanical demos (5, 6) stay on Qwen because:

- The model just needs to emit one `network__connect` tool call —
  any halfway-competent instruct model with seed 42 + temperature 0
  produces it deterministically.
- Qwen 1.5B's 3-second turn keeps the GIF tight; Gemma 4 E4B at
  10× the inference time would just inflate the recording without
  improving the security story.
- No re-render needed when LiteRT-LM lands; the demo's existing
  GIF stays valid.
