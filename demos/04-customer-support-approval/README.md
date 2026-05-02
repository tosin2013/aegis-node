# Demo 04 — Customer support with F3 approval gate

Per [ADR-020](../../docs/adrs/020-recorded-demo-program.md) §"Six
scenarios" item 4. The agent reads a customer's case, drafts a
refund letter, and **the write requires F3 approval** before it
lands. The recording demonstrates the approved path; the rejected
path is a one-line edit (flip `decision` in the approval file).

## What this demonstrates

| Layer | Behavior in this demo |
|---|---|
| **F2 Permission Manifest** | Read `/tmp/aegis-demo-04/cases`, write `/tmp/aegis-demo-04/drafts` narrowed by an explicit `write_grant` with `approval_required: true`. No network, no exec, no MCP. |
| **F3 Human Approval Gate** | The explicit `write_grant`'s `approval_required: true` upgrades the policy decision from Allow to RequireApproval. The mediator routes through the F3 channel — here the **file channel** (per ADR-005 + the v0.8.0 implementation) — and writes only after a positive decision. |
| **F4 Access Log** | The post-approval write of `refund-letter.md` and the approval entry itself chain into the ledger. |
| **F7 Time-bounded** | `duration: PT1H` makes the grant valid for one hour after session start. Combined with F3 — the grant is *both* time-bounded and approval-gated. |
| **F9 Hash-chained ledger** | Tamper-evident; `aegis verify` confirms. |

## What you see in the GIF

| t | Frame | Note |
|---|---|---|
| 0–4s | `grep write_grants` showing `approval_required: true` | The grant exists; approval still required |
| 4–6s | `cat approval.json` shows the pre-staged human decision | F3 file channel: `{decision: granted, approver, reason}` |
| 6–28s | `aegis run --backend litertlm --prompt "..."` runs Gemma 4 E2B | RequireApproval + approval decision + write — ~22s |
| 28–32s | `grep approval` shows F3 approval entry | `entryType: approval`, `decision: granted`, `approver: alice@org` |
| 32–36s | `grep accessType.:.write` shows F2 + F7 write entry | Post-approval write to `/tmp/aegis-demo-04/drafts/refund-letter.md` |
| 36–40s | `aegis verify ledger-demo-04.jsonl` | F9 chain validates |

## Run locally

```bash
make -C demos 04-customer-support-approval
```

That's it. The Makefile invokes `setup.sh` which `aegis pull`s
Gemma 4 E2B, mkdirs `/tmp/aegis-demo-04/`, symlinks the model +
chat-template-sidecar, writes the sample case file, pre-stages
the F3 file-channel approval (`approval.json` with
`decision: granted`), and pre-cleans old ledgers / drafts. Then
VHS renders. Idempotent — re-runs cache-hit on `aegis pull`.

### What `setup.sh` does (in case you want to do it by hand)

```bash
# 1) Pull Gemma 4 E2B (one-time per machine)
aegis pull \
  ghcr.io/tosin2013/aegis-node-models/gemma-4-e2b-it@sha256:365c6a8b3b226ec825b74ed16404515ec61521b2d7f24490eac672d74466b2ea \
  --keyless-identity '^https://github\.com/tosin2013/aegis-node/\.github/workflows/models-publish\.yml@.*$' \
  --keyless-oidc-issuer 'https://token.actions.githubusercontent.com'

# 2) Stage the workdir under /tmp (no root-of-FS paths required)
mkdir -p /tmp/aegis-demo-04/cases /tmp/aegis-demo-04/drafts
ln -sf ~/.cache/aegis/models/365c6a8b3b226ec825b74ed16404515ec61521b2d7f24490eac672d74466b2ea/blob.bin \
       /tmp/aegis-demo-04/model.litertlm
ln -sf ~/.cache/aegis/models/365c6a8b3b226ec825b74ed16404515ec61521b2d7f24490eac672d74466b2ea/chat_template.sha256.txt \
       /tmp/aegis-demo-04/chat_template.sha256.txt
ln -sf "$PWD/demos/04-customer-support-approval/manifest.yaml" \
       /tmp/aegis-demo-04/manifest.yaml

cat > /tmp/aegis-demo-04/cases/case-1024.txt <<EOF
Customer #1024 reports their package arrived damaged on 2026-04-15.
Order total: \$87.43. They request a full refund.
EOF

cat > /tmp/aegis-demo-04/approval.json <<EOF
{
  "decision": "granted",
  "approver": "alice@org",
  "reason": "verified case; refund within policy"
}
EOF

# 3) Render
cd demos/04-customer-support-approval && vhs demo.tape
```

## The approval file format

The F3 file channel reads a JSON document the operator hand-edits
(or a workflow tool produces). Schema per ADR-005:

```json
{
  "decision": "granted",
  "approver": "alice@org",
  "reason": "verified case; refund within policy"
}
```

`decision` may be `"granted"` or `"rejected"`. The approval
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
