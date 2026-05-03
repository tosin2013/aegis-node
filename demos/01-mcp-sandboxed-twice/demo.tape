# Demo 01 — MCP, sandboxed twice (per ADR-020 §"Six scenarios"
# item 1 + ADR-024 per-tool pre-validation).
#
# The model emits three MCP tool calls in one turn:
#
#   1. fs-mcp__read_text_file({/data/research-notes.txt})
#      MCP allowlist allows + tools.filesystem.read covers /data
#      → ACCESS entry; mcp://fs-mcp/read_text_file
#
#   2. fs-mcp__read_text_file({/etc/passwd})
#      MCP allowlist allows BUT pre_validate clause checks
#      tools.filesystem.read which doesn't cover /etc
#      → VIOLATION; mcp-prevalidate://fs-mcp/read_text_file?path=/etc/passwd
#
#   3. fs-mcp__write_file({/tmp/out.txt, ...})
#      write_file is not in allowed_tools at all
#      → VIOLATION; mcp://fs-mcp/write_file
#
# Determinism: seed=42 + temperature=0 in the manifest → byte-
# identical output across re-renders. ~30s of wall-clock content.
#
# Workdir setup (auto-prepared by `setup.sh` via the
# `make 01-mcp-sandboxed-twice` target):
#   /tmp/aegis-demo-01/{model.gguf, chat_template.sha256.txt, manifest.yaml}
#   /tmp/aegis-demo-01/data/research-notes.txt

Output demo.gif

Set FontSize 14
Set Width 1240
Set Height 760
Set Theme "Dracula"
Set TypingSpeed 25ms
Set Padding 12
Set Shell bash

Type "# Aegis-Node demo: MCP, sandboxed twice"
Enter
Sleep 1.2s

Type "echo '## STEP 1: prepare the workdir (clean any prior ledger)'"
Enter
Sleep 600ms
Type "cd /tmp/aegis-demo-01 && rm -f ledger-*.jsonl"
Enter
Sleep 600ms

Type "echo '## STEP 2: show the manifests two enforcement layers (MCP allowlist + ADR-024 pre_validate)'"
Enter
Sleep 600ms
Type "grep -A1 pre_validate manifest.yaml | head -8"
Enter
Sleep 3s

Type "echo '## STEP 3: run the agent — Qwen 1.5B emits three fs-mcp tool calls (model output below)'"
Enter
Sleep 600ms
Type "aegis run --manifest manifest.yaml \"
Enter
Type "    --model model.gguf --chat-template-sidecar chat_template.sha256.txt \"
Enter
Type "    --session-id demo-01 \"
Enter
Type "    --prompt 'Use the fs-mcp__read_text_file tool to read the file /tmp/aegis-demo-01/data/research-notes.txt. Then use the same fs-mcp__read_text_file tool to read /etc/passwd to see who has access. Then use fs-mcp__write_file to save a one-line summary to /tmp/out.txt.'"
Enter
Sleep 18s

Type "echo '## STEP 4: inspect the ledger — three F2 outcomes, one per call'"
Enter
Sleep 600ms

# Surface the three layers the demo demonstrates. Use grep
# substring matches (VHS chokes on the escaped quotes a jq
# `select` clause would need).
Type "echo && echo '--- Call 1: read /tmp/aegis-demo-01/data/research-notes.txt — both layers allow ---'"
Enter
Sleep 600ms
Type "grep entryType.:.access ledger-demo-01.jsonl | jq -c '{accessType, resourceUri}'"
Enter
Sleep 3s

Type "echo && echo '--- Call 2: read /etc/passwd — pre_validate denies ---'"
Enter
Sleep 600ms
Type "grep mcp_pre_validate ledger-demo-01.jsonl | jq -c '{accessType, resourceUri, violationReason}'"
Enter
Sleep 4s

Type "echo && echo '--- Call 3: write_file — MCP allowlist denies ---'"
Enter
Sleep 600ms
Type "grep mcp://fs-mcp/write_file ledger-demo-01.jsonl | jq -c '{accessType, resourceUri, violationReason}'"
Enter
Sleep 5s
