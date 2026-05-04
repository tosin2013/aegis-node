# Example 03 — customer-support agent with F3 approval gate

The agent reads a customer case file and drafts a refund letter. The
write requires F3 human approval — pre-staged via the file channel for
this example, but the same `approval_required: true` flag works with TTY
prompts, the localhost web UI, and the mTLS+SPIFFE signed-API channel.

## Why this matters in 2026

Customer support is the **#1 enterprise AI agent use case** in 2026 — 47%
adoption in banking and insurance per *Deloitte State of AI in the
Enterprise 2026*. It's also where the **OWASP "human-agent trust
exploitation" risk** lands hardest: customers and reviewers assume the
agent's actions are reviewed when they may not be, and a $87 refund
slipping through unsigned looks no different from a $87,000 one.

Aegis-Node's F3 approval gate (`approval_required: true` on a write_grant)
upgrades the manifest's `Allow` decision to `RequireApproval` — the
mediator routes through a configured channel (file / TTY / web / signed
API) and produces a chain-of-evidence ledger entry naming the approver
and reason. *Who approved this refund? When? On what evidence?* The
ledger answers all three.

Source: *Deloitte State of AI in the Enterprise 2026*; *OWASP Top 10
for Agentic Applications 2026*.

## What you'll see

- Phase 1: agent emits `filesystem__read` for the case file → mediator
  allows → F4 entry in ledger
- Phase 2: agent emits `filesystem__write` for the draft → mediator
  upgrades to `RequireApproval` (per the explicit write_grant)
- Phase 3: F3 file-channel reads `approval.json` → decision is `granted`
  → write proceeds → F4 + F3 entries land in the ledger
- The refund letter at `output/refund-letter.md` cites the case
- `aegis verify` confirms the chain — including the approval entry that
  authorized the write

## Run it

```bash
bash setup.sh
cd /tmp/aegis-example-03
AEGIS_APPROVAL_FILE=/tmp/aegis-example-03/approval.json \
aegis run --manifest manifest.yaml --model model.gguf \
    --workload support-agent --instance inst-001 \
    --prompt "$(cat prompt.txt)"
cat output/refund-letter.md
grep approval ledger-*.jsonl | jq -c '{entryType, approver, decision}'
aegis verify ledger-*.jsonl
```

## What just happened

| F-feature | What enforced it | ADR |
|---|---|---|
| **F2 Permission Manifest** | Read of the case file gated against `tools.filesystem.read` | [ADR-009](../../docs/adrs/009-permission-manifest-format.md) |
| **F3 Human Approval Gate** | Write of `refund-letter.md` upgraded to `RequireApproval`; F3 file channel read `approval.json`; approver + reason captured in the ledger | [ADR-005](../../docs/adrs/005-f3-human-approval-gate.md) |
| **F7 Time-Bounded Write Grant** | The explicit grant carries `duration: "PT1H"` — outside the window the same write is denied | [ADR-019](../../docs/adrs/019-explicit-write-grant-takes-precedence.md) |
| **F4 Access Log** | Every read + write + approval lands as a typed F4 entry in the ledger | [ADR-010](../../docs/adrs/010-trajectory-ledger-format.md) |
| **F9 Hash-Chained Ledger** | The approval entry hash-chains to surrounding entries; tampering is detectable | [ADR-010](../../docs/adrs/010-trajectory-ledger-format.md) |

## Inspect the artifacts

```bash
# The work product
cat output/refund-letter.md

# The F3 approval evidence
grep approval ledger-*.jsonl | jq .

# The post-approval write
grep 'accessType.:.write' ledger-*.jsonl | jq -c '{resourceUri, bytesAccessed}'

# Chain integrity
aegis verify ledger-*.jsonl
```

The refund letter (work product) and the approval entry (governance
evidence) live in two files but share one tamper-evident chain. That is
the answer to *"who approved this refund and when?"* — both the artifact
and the proof.

## Make it yours

- **Change the case** — edit `fixtures/case-1024.txt` to your own customer
  scenario, rerun. The refund letter content reflects the new case.
- **Reject the approval** — edit `approval.json` and set `"decision":
  "rejected"`. Re-run. The write fails; F3 violation lands in the ledger;
  `aegis verify` still passes.
- **Add a $-threshold approval** — modify the prompt to "*flag any case
  over $1000 for additional review*"; the agent's letter calls out the
  threshold logic but the manifest still requires the single approval.
  (For per-row gating over a threshold, see Example 06.)
- **Switch channels** — drop the `AEGIS_APPROVAL_FILE` env var and use
  the TTY channel instead (the agent prompts you live). Or wire the
  localhost web UI per ADR-005 §"Web channel."

## What you should see when you tighten the manifest

Remove the `write_grants` block entirely. Re-run. The broad
`tools.filesystem.write: ["/tmp/aegis-example-03/output"]` still allows
the write *without approval* (per ADR-019: closed-by-default → broad
grant Allows; explicit grants narrow). Now also remove the broad
`filesystem.write` entry. Re-run; the write fails with an F2 violation
naming `output/refund-letter.md`, no artifact lands, `aegis verify`
still passes. That's the lesson: there are two layers (broad grant +
explicit grant with approval); explicit takes precedence per ADR-019;
both layers must permit a write for it to succeed.
