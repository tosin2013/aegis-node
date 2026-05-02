# Demo 03 — Code review agent with time-bounded write grant.
# Per ADR-020 §"Six scenarios" item 3.
#
# Workdir setup (auto-prepared by `make 03-code-review-time-bounded`):
#   /tmp/aegis-demo-03/{model.litertlm, chat_template.sha256.txt}
#   /repo  — a real git repo with at least 2 commits
#   /data  — empty; review.md lands here
#
# Backend: LiteRT-LM / Gemma 4 E4B (per ADR-020 §"Decision" item 8).

Output demo.gif

Set FontSize 14
Set Width 1240
Set Height 760
Set Theme "Dracula"
Set TypingSpeed 25ms
Set Padding 12
Set Shell bash

Type "# Aegis-Node demo: Code review with time-bounded write grant"
Enter
Sleep 1.2s

Type "cd /tmp/aegis-demo-03 && rm -f ledger-*.jsonl /data/review.md"
Enter
Sleep 600ms

# Show the time-bounded write_grant + the exec_grant.
Type "grep -A3 'write_grants:' /root/aegis-node/demos/03-code-review-time-bounded/manifest.yaml"
Enter
Sleep 3s
Type "grep -A2 'exec_grants:' /root/aegis-node/demos/03-code-review-time-bounded/manifest.yaml"
Enter
Sleep 2s

# Run the agent. Gemma 4 E4B uses git diff + writes the review.
Type "aegis run --backend litertlm \"
Enter
Type "    --manifest /root/aegis-node/demos/03-code-review-time-bounded/manifest.yaml \"
Enter
Type "    --model model.litertlm --chat-template-sidecar chat_template.sha256.txt \"
Enter
Type "    --session-id demo-03 \"
Enter
Type "    --prompt 'Use exec__run to call git with arguments [diff, --stat, HEAD~1, HEAD] from /repo. Then write a one-paragraph code review to /data/review.md using filesystem__write.'"
Enter
Sleep 28s

# Surface the F-features the demo exercised:
Type "echo && echo '--- F2 Exec grant (git diff) ---'"
Enter
Sleep 600ms
Type "grep accessType.:.exec ledger-demo-03.jsonl | jq -c '{accessType, resourceUri}'"
Enter
Sleep 3s

Type "echo && echo '--- F7 Time-bounded write — within window ---'"
Enter
Sleep 600ms
Type "grep accessType.:.write ledger-demo-03.jsonl | jq -c '{accessType, resourceUri, bytesAccessed}'"
Enter
Sleep 4s

Type "echo && echo '--- F4 Access entries (full sequence) ---'"
Enter
Sleep 600ms
Type "grep entryType.:.access ledger-demo-03.jsonl | jq -c '{accessType, resourceUri}'"
Enter
Sleep 5s
