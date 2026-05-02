# Demo 02 — Read-only research assistant.
# Per ADR-020 §"Six scenarios" item 2.
#
# The agent gets read access to /data plus deny-by-default network.
# No writes, no exec, no MCP. Every other I/O attempt would land
# as an F2 Violation.
#
# Backend: LiteRT-LM with Gemma 4 E4B (per ADR-020 §"Decision" item 8).
#
# Workdir setup (auto-prepared by `make 02-read-only-research`):
#   /tmp/aegis-demo-02/{model.litertlm, chat_template.sha256.txt}
#   /data/research-notes.txt

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

Type "cd /tmp/aegis-demo-02 && rm -f ledger-*.jsonl"
Enter
Sleep 600ms

# Show the manifest's read-only posture at a glance.
Type "grep -A4 'tools:' /root/aegis-node/demos/02-read-only-research/manifest.yaml | head -8"
Enter
Sleep 3s

# Run the agent. Gemma 4 E4B at greedy temperature → deterministic
# summary across re-renders.
Type "aegis run --backend litertlm \"
Enter
Type "    --manifest /root/aegis-node/demos/02-read-only-research/manifest.yaml \"
Enter
Type "    --model model.litertlm --chat-template-sidecar chat_template.sha256.txt \"
Enter
Type "    --session-id demo-02 \"
Enter
Type "    --prompt 'Read /data/research-notes.txt and write a one-paragraph summary of the key findings. Quote one specific number from the source. Do not access any other files.'"
Enter
Sleep 25s

# Surface the F-features the demo exercised:
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

Type "echo && echo '--- F9 Tamper-evident chain ---'"
Enter
Sleep 600ms
Type "aegis verify ledger-demo-02.jsonl"
Enter
Sleep 4s
