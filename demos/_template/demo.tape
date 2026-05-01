# Aegis-Node demo template — VHS recording script.
# Per ADR-020. Run via `make <demo-name>` from the demos directory.

Output demo.gif

Set FontSize 14
Set Width 1100
Set Height 700
Set Theme "Dracula"
Set TypingSpeed 40ms

# Title card.
Type "# Aegis-Node demo: <one-line description>"
Enter
Sleep 800ms

# Show the manifest's tool catalog so the viewer sees what's allowed
# before the agent acts.
Type "cat manifest.yaml"
Enter
Sleep 1500ms

# Run the agent with the recorded prompt. The CLI must be built with
# --features llama; the manifest pins the determinism knobs so the
# rendered GIF is reproducible.
Type "aegis run \\"
Enter
Type "  --manifest manifest.yaml \\"
Enter
Type "  --model ~/.cache/aegis/models/c7404a91/blob.bin \\"
Enter
Type "  --prompt \"$(cat prompt.txt)\""
Enter

# The agent runs. Allow time for the model to emit reasoning + tool
# calls, plus the mediator's policy decisions.
Sleep 8s

# Show the resulting ledger so the viewer sees the F4/F5/F9 evidence.
Type "aegis verify ledger-*.jsonl"
Enter
Sleep 2s
