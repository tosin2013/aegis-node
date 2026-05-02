# Demo 04 — Customer support with F3 approval gate

Per [ADR-020](../../docs/adrs/020-recorded-demo-program.md) §"Six
scenarios" item 4. The agent reads a customer's case, drafts a
refund letter, and **the write requires F3 approval** before it
lands. The recording demonstrates the approved path; the rejected
path is a one-line edit (flip `decision` in the approval file).

## What this demonstrates

| Layer | Behavior in this demo |
|---|---|
| **F2 Permission Manifest** | Read `/cases`, write `/drafts` narrowed by an explicit `write_grant` with `approval_required: true`. No network, no exec, no MCP. |
| **F3 Human Approval Gate** | The explicit `write_grant`'s `approval_required: true` upgrades the policy decision from Allow to RequireApproval. The mediator routes through the F3 channel — here the **file channel** (per ADR-005 + the v0.8.0 implementation) — and writes only after a positive decision. |
| **F4 Access Log** | The read of `/cases/case-1024.txt`, the post-approval write of `/drafts/refund-letter.md`, and the approval entry itself all chain into the ledger. |
| **F7 Time-bounded** | `duration: PT1H` makes the grant valid for one hour after session start. Combined with F3 — the grant is *both* time-bounded and approval-gated. |
| **F9 Hash-chained ledger** | Tamper-evident; `aegis verify` confirms. |

## What you see in the GIF

| t | Frame | Note |
|---|---|---|
| 0–4s | `grep write_grants` showing `approval_required: true` | The grant exists; approval still required |
| 4–6s | `echo > approval.json` pre-stages the human decision | F3 file channel: `{decision: approved, approver, reason}` |
| 6–24s | `aegis run --backend litertlm --prompt "..."` runs Gemma 4 E2B | Read + RequireApproval + approval decision + write — ~18s |
| 24–28s | `grep accessType.:.read` shows F4 read entry | The case file the agent grounded its draft in |
| 28–32s | `grep approval` shows F3 approval entry | `entryType: approval`, `decision: approved`, `approver: alice@org` |
| 32–37s | `grep accessType.:.write` shows F2 + F7 write entry | Post-approval write to `/drafts/refund-letter.md` |

## Run locally

```bash
# 1) Pull the cosign-verified Gemma 4 E2B artifact
aegis pull \
  ghcr.io/tosin2013/aegis-node-models/gemma-4-e2b-it@sha256:365c6a8b3b226ec825b74ed16404515ec61521b2d7f24490eac672d74466b2ea \
  --keyless-identity '^https://github\.com/tosin2013/aegis-node/\.github/workflows/models-publish\.yml@.*$' \
  --keyless-oidc-issuer 'https://token.actions.githubusercontent.com'

# 2) Stage the demo workdir + a sample case
mkdir -p /tmp/aegis-demo-04 /cases /drafts
cp ~/.cache/aegis/models/<gemma-4-e2b-blob-sha>/blob.bin \
   /tmp/aegis-demo-04/model.litertlm
ln -sf ~/.cache/aegis/models/<gemma-4-e2b-blob-sha>/chat_template.sha256.txt \
   /tmp/aegis-demo-04/chat_template.sha256.txt
cat > /cases/case-1024.txt <<EOF
Customer #1024 reports their package arrived damaged on 2026-04-15.
Order total: \$87.43. They request a full refund.
EOF

# 3) Render
make -C demos 04-customer-support-approval
```

## The approval file format

The F3 file channel reads a JSON document the operator hand-edits
(or a workflow tool produces). Schema per ADR-005:

```json
{
  "decision": "approved",
  "approver": "alice@org",
  "reason": "verified case; refund within policy"
}
```

`decision` may be `"approved"` or `"rejected"`. The approval
entry that lands in the ledger carries the decision verbatim plus
the approver/reason for the audit trail.

## Glibc requirement

Same as Demos 2 and 3 — LiteRT-LM runtime needs **glibc 2.38+**.
Render on ubuntu-24.04+ host or in CI. Pre-LiteRT demos (5, 6)
using llama.cpp work on older hosts.

## Why Gemma 4 E2B (not Qwen 1.5B or E4B)

Per ADR-020 §"Decision" item 8: customer-support narrative reads
better with a realistic agent voice — Qwen 1.5B's draft tends to
sound like auto-completion. E2B (~2.6 GB) is a sweet spot: better
voice than Qwen, faster than E4B, and the per-turn budget (~15s)
keeps the GIF tight.
