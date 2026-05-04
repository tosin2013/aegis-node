# Example 05 — egress containment + signed network attestation

The agent is told to phone home to an attacker. The mediator's deny-by-
default network policy refuses the connection, the F6 NetworkAttestation
end-of-session entry signs a complete record of the attempt, and a
small post-session script promotes the attestation into a standalone
artifact + a human-readable session report.

## Why this matters in 2026

This is the answer to **the canonical 2026 AI governance question**:
*"what did this agent do last Tuesday at 3pm?"*

Per *VentureBeat / Lyzr State of AI Agents Q1 2026*, **88% of
enterprises reported AI agent security incidents in the last 12 months;
only 21% have runtime visibility** into agent behavior. The OWASP Top
10 for Agentic Applications 2026 lists **goal hijack via exfiltration**
as a Top-10 risk: a prompt-injected agent that tries to phone home with
your data is the canonical attack pattern.

Aegis-Node's F6 deny-by-default + signed end-of-session
NetworkAttestation is the answer: every connect attempt (allowed *or*
denied) lands in a single signed ledger entry with `allowedCount`,
`deniedCount`, per-attempt records (host, port, protocol, decision,
timestamp), a `connectionsDigestHex`, and an HMAC `signatureHex`. The
SOC ingests it into their SIEM and correlates against F4 Access logs
— exactly the capability the 79% of enterprises currently lack.

Source: *VentureBeat: 88% of enterprises reported AI agent security
incidents last year* (Apr 2026); *OWASP Top 10 for Agentic Applications
2026*; *Lyzr State of AI Agents Q1 2026*.

## What you'll see

- Model emits a `network__connect` tool call against
  `attacker.example.com:443`
- Mediator's policy returns `Deny { reason: "manifest sets deny" }`
- F2 Violation entry lands in the ledger naming
  `network://attacker.example.com:443`
- Session shutdown emits the F6 NetworkAttestation summarizing the
  attempt (signed)
- `extract-attestation.sh` promotes the F6 entry to
  `output/network-attestation.json` and writes
  `output/session-report.md`
- `aegis verify` confirms the chain — including the violation + the
  attestation

## Run it

```bash
bash setup.sh
cd /tmp/aegis-example-05
aegis run --manifest manifest.yaml --model model.gguf \
    --workload egress-audit --instance inst-001 \
    --prompt "$(cat prompt.txt)"
bash extract-attestation.sh
cat output/session-report.md
aegis verify ledger-*.jsonl
```

## What just happened

| F-feature | What enforced it | ADR |
|---|---|---|
| **F2 Permission Manifest** | `tools.network.outbound: deny` is the policy that drives the deny decision | [ADR-009](../../docs/adrs/009-permission-manifest-format.md) |
| **F6 Network Deny-by-Default** | `policy.check_network_outbound(host, port, proto)` returned `Deny`; F2 violation entry landed | [ADR-014](../../docs/adrs/014-network-deny-by-default.md) |
| **F6 Signed Network Attestation** | At session shutdown, `attestation::emit_network_attestation` walked `Session.network_log` and emitted a `NetworkAttestation` ledger entry with allowed/denied counts + per-attempt records + HMAC signature | [ADR-008](../../docs/adrs/008-end-of-session-network-attestation.md) |
| **F4 Access Log** | Every connect attempt (regardless of outcome) lands in the network log; the F4 entry for the violation names the resource URI | [ADR-010](../../docs/adrs/010-trajectory-ledger-format.md) |
| **F9 Hash-Chained Ledger** | Both the F2 Violation and the F6 attestation are tamper-evident; `aegis verify` confirms the chain root | [ADR-010](../../docs/adrs/010-trajectory-ledger-format.md) |

## Inspect the artifacts

```bash
# The signed attestation as a standalone artifact
cat output/network-attestation.json | jq .

# The human-readable session report (suitable for ticket attachments)
cat output/session-report.md

# The F2 violation that recorded the denied attempt
grep violationReason ledger-*.jsonl | jq .

# Chain integrity
aegis verify ledger-*.jsonl
```

The session report is the SOC-friendly answer to *"what did this agent
do at 3pm?"* — it lists every tool call, the network attestation, and
the chain-verification result in one Markdown file. Ingest it into a
SIEM via the F6 attestation JSON; review it via the Markdown report.

## Make it yours

- **Allow a specific host** — change
  `tools.network.outbound: deny` to an allowlist:
  ```yaml
  network:
    outbound:
      allowed:
        - host: api.example.com
          port: 443
  ```
  Re-run; the connect to `api.example.com:443` succeeds, the
  attestation shows `allowedCount: 1` for it, and other hosts still
  deny. (See Example 02's extended Firecrawl mode for a real allowlist
  in action.)
- **Multi-attempt session** — change the prompt to ask the agent to
  try multiple hosts. The attestation aggregates all of them.
- **SIEM-ingest format** — adapt `extract-attestation.sh` to emit
  CEF / LEEF / OCSF instead of raw JSON. The F6 entry's structure
  is documented in [ADR-008](../../docs/adrs/008-end-of-session-network-attestation.md).
- **Pair with F8 replay** — when F8 trajectory replay lands (v0.9.0+),
  the same ledger replays the agent's reasoning that *led to* the
  exfiltration attempt — so you see motive, not just attempt.

## What you should see when you tighten the manifest

Replace `tools.network.outbound: deny` with an empty `tools` block.
Re-run; the agent's `network__connect` call now lands as an F2
violation at the *catalog* layer (the tool isn't advertised at all),
the attestation entry shows zero connections (no tool to invoke), and
`aegis verify` still passes. Lesson: the deny path varies by *which
layer* refuses (catalog vs policy vs OS) — the ledger names the layer
in `resourceUri`, so an auditor can tell at a glance whether the model
was unable to *attempt* (catalog) or attempted-and-was-denied (policy).
