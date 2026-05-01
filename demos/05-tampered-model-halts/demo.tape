# Demo 05 — Tampered model halts session (F1 IdentityRebind).
# Per ADR-020 §"Six scenarios" item 5.
#
# The recording assumes a workdir at /tmp/aegis-demo-05/ with a fresh
# copy of the cosign-verified Qwen GGUF at model.gguf — the
# `make 05-tampered-model-halts` target sets that up.
#
# Why one combined shell command (instead of separate Type lines for
# launch and tamper): the model emits its first tool call in ~3s on
# CPU. Visible per-character typing eats more time than that — by the
# time the .tape "types" the tamper command, the model has already
# finished and the rebind already passed. Putting launch + sleep +
# tamper + wait in one bash command lets the *shell* control the
# timing, not VHS's TypingSpeed.

Output demo.gif

Set FontSize 14
Set Width 1200
Set Height 740
Set Theme "Dracula"
Set TypingSpeed 25ms
Set Padding 12
Set Shell bash

Type "# Aegis-Node demo: F1 IdentityRebind — tampered model halts the session"
Enter
Sleep 1.2s

Type "cd /tmp/aegis-demo-05 && rm -f ledger-*.jsonl run.log"
Enter
Sleep 700ms

Type "sha256sum model.gguf"
Enter
Sleep 1.5s

# Stage the tamper payload outside the agent's view, then in one
# shell command: launch agent → wait 1s → atomic-rename the model →
# wait for the agent to halt.
Type "echo 'tampered bytes' > /tmp/t.bin"
Enter
Sleep 500ms

Type "(aegis run --manifest /root/aegis-node/demos/05-tampered-model-halts/manifest.yaml \"
Enter
Type "    --model model.gguf --prompt 'List files in /data' \"
Enter
Type "    --session-id demo-05 > run.log 2>&1 &) ; \"
Enter
Type "  sleep 1 ; mv /tmp/t.bin model.gguf ; sha256sum model.gguf ; wait ; echo aegis-exit=$?"
Enter
Sleep 9s

Type "# F1 IdentityRebind violation in the ledger:"
Enter
Sleep 500ms

# Pipe through grep first to avoid VHS's quote-escaping limitations;
# the Violation is the only entry whose type is `violation` in this
# demo session.
Type "grep violation ledger-demo-05.jsonl | jq ."
Enter
Sleep 6s
