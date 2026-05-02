# Demo 02 — Read-only research assistant.
# Per ADR-020 §"Six scenarios" item 2.
#
# The agent gets read access to the demo's workdir-local data
# directory plus deny-by-default network. No writes, no exec, no
# MCP. Every other I/O attempt would land as an F2 Violation.
#
# Backend: LiteRT-LM with Gemma 4 E4B (per ADR-020 §"Decision" item 8).
#
# The .tape runs from /tmp/aegis-demo-02/, prepared by setup.sh
# (auto-invoked by `make 02-read-only-research` per the
# demos/Makefile %/demo.gif rule). Everything the .tape touches is
# inside the workdir — no `/data/` or other root-of-FS paths.

Output demo.gif

Set FontSize 14
Set Width 1240
Set Height 760
Set Theme "Dracula"
Set TypingSpeed 25ms
Set Padding 12
Set Shell bash

Type "# Aegis-Node demo: Read-only research assistant"
Enter
Sleep 1.2s

Type "echo '## STEP 1: prepare the workdir (clean any prior ledger)'"
Enter
Sleep 600ms
Type "cd /tmp/aegis-demo-02 && rm -f ledger-*.jsonl"
Enter
Sleep 600ms

Type "echo '## STEP 2: show the manifests read-only posture (no writes, no exec, no network)'"
Enter
Sleep 600ms
Type "grep -A4 'tools:' manifest.yaml | head -8"
Enter
Sleep 3s

Type "echo '## STEP 3: run the agent — Gemma 4 E4B reads the file and summarizes (model output below)'"
Enter
Sleep 600ms
Type "aegis run --backend litertlm \"
Enter
Type "    --manifest manifest.yaml \"
Enter
Type "    --model model.litertlm --chat-template-sidecar chat_template.sha256.txt \"
Enter
Type "    --session-id demo-02 \"
Enter
Type "    --prompt 'Read /tmp/aegis-demo-02/data/research-notes.txt and write a one-paragraph summary of the key findings. Quote one specific number from the source. Do not access any other files.'"
Enter
Sleep 25s

Type "echo '## STEP 4: inspect the ledger — F4 Access + F6 NetworkAttestation'"
Enter
Sleep 600ms

Type "echo && echo '--- F4 Access entries (what the agent read) ---'"
Enter
Sleep 600ms
Type "grep entryType.:.access ledger-demo-02.jsonl | jq -c '{accessType, resourceUri}'"
Enter
Sleep 3s

Type "echo && echo '--- F6 NetworkAttestation (zero connections — agent never tried to phone home) ---'"
Enter
Sleep 600ms
Type "grep network_attestation ledger-demo-02.jsonl | jq -c '{accessType, totalConnections, allowedCount, deniedCount}'"
Enter
Sleep 4s

Type "echo '## STEP 5: verify F9 — the hash-chained ledger is tamper-evident'"
Enter
Sleep 600ms
Type "aegis verify ledger-demo-02.jsonl"
Enter
Sleep 4s
