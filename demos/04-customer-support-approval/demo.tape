# Demo 04 — Customer-support agent with F3 approval gate.
# Per ADR-020 §"Six scenarios" item 4.
#
# The agent reads a customer's case file and writes a refund-letter
# draft. The write requires F3 approval (approval_required: true on
# the explicit write_grant). The recording uses the F3 file-channel:
# AEGIS_APPROVAL_FILE points at /tmp/aegis-demo-04/approval.json,
# pre-staged with `decision: granted`. A second take with
# `decision: rejected` would show the deny path.
#
# Backend: LiteRT-LM / Gemma 4 E2B (per ADR-020 §"Decision" item 8).
#
# The .tape runs from /tmp/aegis-demo-04/, prepared by setup.sh
# (auto-invoked by `make 04-customer-support-approval` per the
# demos/Makefile %/demo.gif rule). Everything the .tape touches is
# inside the workdir — no `/cases/`, `/drafts/` or other
# root-of-FS paths.

Output demo.gif

Set FontSize 14
Set Width 1240
Set Height 760
Set Theme "Dracula"
Set TypingSpeed 25ms
Set Padding 12
Set Shell bash

Type "# Aegis-Node demo: Customer support with F3 approval gate"
Enter
Sleep 1.2s

Type "echo '## STEP 1: prepare the workdir (clean any prior ledger and draft)'"
Enter
Sleep 600ms
Type "cd /tmp/aegis-demo-04 && rm -f ledger-*.jsonl drafts/refund-letter.md"
Enter
Sleep 600ms

Type "echo '## STEP 2: show the explicit write_grant with approval_required: true'"
Enter
Sleep 600ms
Type "grep -A3 'write_grants:' manifest.yaml"
Enter
Sleep 3s

Type "echo '## STEP 3: show the pre-staged F3 approval (the human approvers decision)'"
Enter
Sleep 600ms
Type "cat approval.json"
Enter
Sleep 2s

Type "echo '## STEP 4: run the agent — F3 gates the write; on grant, Gemma 4 E2B drafts the letter (model output below)'"
Enter
Sleep 600ms
Type "AEGIS_APPROVAL_FILE=/tmp/aegis-demo-04/approval.json aegis run \"
Enter
Type "    --backend litertlm \"
Enter
Type "    --manifest manifest.yaml \"
Enter
Type "    --model model.litertlm --chat-template-sidecar chat_template.sha256.txt \"
Enter
Type "    --session-id demo-04 \"
Enter
Type "    --prompt 'Write a one-paragraph refund-letter draft to /tmp/aegis-demo-04/drafts/refund-letter.md using filesystem__write. The letter should acknowledge a customer whose package arrived damaged and explain that a full refund will be issued.'"
Enter
Sleep 22s

Type "echo '## STEP 5: inspect the ledger — F3 approval chain + F2/F7 write + F9 verify'"
Enter
Sleep 600ms

Type "echo && echo '--- F3 Approval entry ---'"
Enter
Sleep 600ms
Type "grep approval ledger-demo-04.jsonl | jq -c '{entryType, accessType, resourceUri}'"
Enter
Sleep 4s

Type "echo && echo '--- F2 + F7: post-approval write of the draft ---'"
Enter
Sleep 600ms
Type "grep accessType.:.write ledger-demo-04.jsonl | jq -c '{accessType, resourceUri, bytesAccessed}'"
Enter
Sleep 4s

Type "echo && echo '--- F9 Tamper-evident chain ---'"
Enter
Sleep 600ms
Type "aegis verify ledger-demo-04.jsonl"
Enter
Sleep 4s
