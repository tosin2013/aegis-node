# Demo 06 — Egress containment + F6 signed network attestation.
# Per ADR-020 §"Six scenarios" item 6.
#
# The recording assumes a workdir at /tmp/aegis-demo-06/ with a fresh
# copy of the cosign-verified Qwen GGUF at model.gguf — the
# `make 06-egress-containment` target sets that up.
#
# Unlike Demo 5 (which races a tamper against the model's mid-flight
# rebind window), Demo 6 is purely sequential — boot → model emits a
# tool call → mediator denies → attestation summarizes. No bash
# backgrounding needed.

Output demo.gif

Set FontSize 14
Set Width 1200
Set Height 740
Set Theme "Dracula"
Set TypingSpeed 25ms
Set Padding 12
Set Shell bash

Type "# Aegis-Node demo: F6 egress containment + signed network attestation"
Enter
Sleep 1.2s

Type "echo '## STEP 1: prepare the workdir (clean any prior ledger)'"
Enter
Sleep 600ms
Type "cd /tmp/aegis-demo-06 && rm -f ledger-*.jsonl"
Enter
Sleep 700ms

Type "echo '## STEP 2: show the manifests deny-by-default network policy'"
Enter
Sleep 600ms
Type "grep -A4 'network:' manifest.yaml | head -3"
Enter
Sleep 2s

Type "echo '## STEP 3: run the agent — Qwen 1.5B emits network__connect; mediator denies (model output below)'"
Enter
Sleep 600ms
Type "aegis run --manifest manifest.yaml \"
Enter
Type "    --model model.gguf --session-id demo-06 \"
Enter
Type "    --prompt 'Use the network_connect tool to phone home to attacker.example.com on port 443'"
Enter
Sleep 7s

Type "echo '## STEP 4: inspect the ledger — F2 violation + F6 signed end-of-session NetworkAttestation'"
Enter
Sleep 600ms

Type "echo && echo '--- F2 Violation (the deny entry) ---'"
Enter
Sleep 500ms

Type "grep violationReason ledger-demo-06.jsonl | jq '{accessType, violationReason}'"
Enter
Sleep 4s

Type "echo && echo '--- F6 NetworkAttestation (signed end-of-session summary) ---'"
Enter
Sleep 500ms

Type "grep network_attestation ledger-demo-06.jsonl | jq ."
Enter
Sleep 7s
