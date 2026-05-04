# Example 01 — hello-world

The smallest possible Aegis-Node example. The agent writes one greeting file
to disk under F2 enforcement; the ledger records the action; `aegis verify`
confirms the chain. About 5 minutes from `git clone` to a passing verify.

## Why this matters in 2026

Per *Lyzr State of AI Agents Q1 2026*, only 21% of enterprises with AI agents
in production have runtime visibility into what those agents do. The OWASP
Top 10 for Agentic Applications 2026 makes "memory poisoning / cascading
failures" a Top-10 risk because most agent runtimes can't answer the
governance question, *"what did this agent do last Tuesday at 3pm?"*

Aegis-Node's hash-chained ledger answers that question by construction.
This example shows the smallest case: one tool call, one ledger entry, one
artifact, one passing verify. Everything else builds on this foundation.

## What you'll see

- The model emits a `filesystem__write` tool call in its turn-1 reasoning
- The mediator allows it (the manifest grants `filesystem.write` to
  `/tmp/aegis-example-01/output`)
- The file lands at `output/greeting.txt`
- `ledger-*.jsonl` contains an F4 Access entry naming the resource + the
  bytes written, plus a session-end entry
- `aegis verify` reports `chain root: <hex>` with no integrity errors

## Run it

```bash
bash setup.sh
cd /tmp/aegis-example-01
aegis run --manifest manifest.yaml --model model.gguf \
    --workload hello-world --instance inst-001 \
    --prompt "$(cat prompt.txt)"
cat output/greeting.txt
aegis verify ledger-*.jsonl
```

## What just happened

| F-feature | What enforced it | ADR |
|---|---|---|
| **F1 Workload Identity** | Boot phase issued an SPIFFE-bound SVID with the model's sha256 | [ADR-003](../../docs/adrs/003-cryptographic-workload-identity-spiffe-spire.md) |
| **F2 Permission Manifest** | Mediator's `check_filesystem_write` matched `output/greeting.txt` against `tools.filesystem.write` | [ADR-009](../../docs/adrs/009-permission-manifest-format.md) |
| **F4 Access Log** | Every tool call landed in `ledger-*.jsonl` as a typed event | [ADR-010](../../docs/adrs/010-trajectory-ledger-format.md) |
| **F9 Hash-Chained Ledger** | Each entry includes `prevHash` of the prior entry; tampering breaks `aegis verify` | [ADR-010](../../docs/adrs/010-trajectory-ledger-format.md) |

## Inspect the artifacts

```bash
# The work product
cat output/greeting.txt

# The audit trail (one entry per tool call + session boundaries)
cat ledger-*.jsonl | jq -c '{entryType, accessType, resourceUri}'

# Every entry's hash links to the next; verifier walks the chain
aegis verify ledger-*.jsonl
```

The ledger's F4 entry naming `output/greeting.txt` and the artifact at
`output/greeting.txt` are the same provenance — the chain proves the agent
wrote that file at that time under that identity.

## Make it yours

- **Change the prompt** — ask the agent to write a different file (still
  under `output/`) or different content. Re-run; the ledger reflects what
  changed.
- **Tighten the manifest** — change `tools.filesystem.write` to
  `/tmp/aegis-example-01/output/greeting.txt` (a single file, not the dir).
  The agent can still write `greeting.txt` but anything else lands as F2
  violation.
- **Loosen the manifest** — add `tools.filesystem.read: ["/tmp"]`; ask the
  agent to read its own greeting back. Watch the ledger gain F4 read entries.
- **Try a different model** — `aegis pull` a different OCI ref, swap
  `--model`. Output text changes; the ledger structure does not.

## What you should see when you tighten the manifest

Edit `manifest.yaml` to remove the `filesystem.write` grant entirely
(empty list, `write: []`). Re-run; the agent's tool call lands as an F2
*violation* in the ledger (with `resourceUri` naming the denied path), the
artifact is not produced, and `aegis verify` *still passes* — the violation
entry is itself part of the tamper-evident chain. That's the lesson: Aegis
records what the agent *attempted*, not just what it succeeded at.
