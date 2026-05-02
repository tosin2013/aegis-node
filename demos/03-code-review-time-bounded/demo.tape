# Demo 03 — Code review agent with time-bounded write grant.
# Per ADR-020 §"Six scenarios" item 3.
#
# The agent gets read access to the demo's workdir-local repo, an
# exec grant for git, and a time-bounded write_grant on the review
# file (1h window from session start, per F7). Network closed.
#
# Backend: LiteRT-LM / Gemma 4 E4B (per ADR-020 §"Decision" item 8).
#
# The .tape runs from /tmp/aegis-demo-03/, prepared by setup.sh
# (auto-invoked by `make 03-code-review-time-bounded` per the
# demos/Makefile %/demo.gif rule). Everything the .tape touches is
# inside the workdir — no `/repo/`, `/data/` or other root-of-FS
# paths.

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

Type "echo '## STEP 1: prepare the workdir (clean any prior ledger and review)'"
Enter
Sleep 600ms
Type "cd /tmp/aegis-demo-03 && rm -f ledger-*.jsonl data/review.md"
Enter
Sleep 600ms

Type "echo '## STEP 2: show the F7 time-bounded write_grant + the exec_grant for git'"
Enter
Sleep 600ms
Type "grep -A3 'write_grants:' manifest.yaml"
Enter
Sleep 3s
Type "grep -A2 'exec_grants:' manifest.yaml"
Enter
Sleep 2s

Type "echo '## STEP 3: run the agent — Gemma 4 E4B writes the review via filesystem__write (model output below)'"
Enter
Sleep 600ms
Type "aegis run --backend litertlm \"
Enter
Type "    --manifest manifest.yaml \"
Enter
Type "    --model model.litertlm --chat-template-sidecar chat_template.sha256.txt \"
Enter
Type "    --session-id demo-03 \"
Enter
Type "    --prompt 'Write a short positive code review to /tmp/aegis-demo-03/data/review.md using filesystem__write. The change replaces a+b with a.checked_add(b).unwrap_or(0), preventing i32 overflow on inputs near i32::MAX. Note that saturate-on-overflow could be documented.'"
Enter
Sleep 28s

Type "echo '## STEP 4: inspect the ledger — F7 write within window + F4 access trail'"
Enter
Sleep 600ms

Type "echo && echo '--- F7 Time-bounded write (review.md, within 1h window) ---'"
Enter
Sleep 600ms
Type "grep accessType.:.write ledger-demo-03.jsonl | jq -c '{accessType, resourceUri, bytesAccessed}'"
Enter
Sleep 4s

Type "echo && echo '--- F4 Access entries (what the agent did) ---'"
Enter
Sleep 600ms
Type "grep entryType.:.access ledger-demo-03.jsonl | jq -c '{accessType, resourceUri}'"
Enter
Sleep 4s

Type "echo '## STEP 5: verify F9 — the hash-chained ledger is tamper-evident'"
Enter
Sleep 600ms
Type "aegis verify ledger-demo-03.jsonl"
Enter
Sleep 4s
