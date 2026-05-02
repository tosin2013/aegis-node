# Demo 04 — Customer-support agent with F3 approval gate.
# Per ADR-020 §"Six scenarios" item 4.
#
# Workdir setup (auto-prepared by `make 04-customer-support-approval`):
#   /tmp/aegis-demo-04/{model.litertlm, chat_template.sha256.txt, approval.json}
#   /cases/case-1024.txt          — the customer complaint
#   /drafts                       — empty; refund-letter.md lands here
#
# The recording uses the F3 file-channel: AEGIS_APPROVAL_FILE
# points at /tmp/aegis-demo-04/approval.json which we pre-stage
# with `decision: granted` so the write proceeds. A second take
# with `decision: rejected` would show the deny path.
#
# Backend: LiteRT-LM / Gemma 4 E2B (per ADR-020 §"Decision" item 8).

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

Type "cd /tmp/aegis-demo-04 && rm -f ledger-*.jsonl /drafts/refund-letter.md"
Enter
Sleep 600ms

# Show the explicit write_grant + approval_required.
Type "grep -A3 'write_grants:' /root/aegis-node/demos/04-customer-support-approval/manifest.yaml"
Enter
Sleep 3s

# Show the pre-staged approval file (the human approver's
# decision — pre-staged by `make 04-customer-support-approval`
# before vhs runs; VHS's `Type` syntax doesn't tolerate the
# escaped-quote shell heredoc the file's content needs).
Type "cat approval.json"
Enter
Sleep 2s

# Run the agent. AEGIS_APPROVAL_FILE wires the F3 file-channel.
Type "AEGIS_APPROVAL_FILE=/tmp/aegis-demo-04/approval.json aegis run \"
Enter
Type "    --backend litertlm \"
Enter
Type "    --manifest /root/aegis-node/demos/04-customer-support-approval/manifest.yaml \"
Enter
Type "    --model model.litertlm --chat-template-sidecar chat_template.sha256.txt \"
Enter
Type "    --session-id demo-04 \"
Enter
Type "    --prompt 'Read /cases/case-1024.txt to understand the customer complaint. Then use filesystem__write to save a one-paragraph refund-letter draft to /drafts/refund-letter.md.'"
Enter
Sleep 18s

# Surface the F-features the demo exercised:
Type "echo && echo '--- F2 + F4: read of the case ---'"
Enter
Sleep 600ms
Type "grep accessType.:.read ledger-demo-04.jsonl | jq -c '{accessType, resourceUri}'"
Enter
Sleep 3s

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
Sleep 5s
