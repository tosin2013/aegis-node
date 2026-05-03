# Demo 01 — MCP, sandboxed twice

Per [ADR-020](../../docs/adrs/020-recorded-demo-program.md) §"Six
scenarios" item 1 + [ADR-024](../../docs/adrs/024-mcp-args-prevalidation.md)
per-tool pre-validation. The ~25-second clip shows **two independent
enforcement layers** both gating every MCP tool call: the protocol-
level allowlist (`tools.mcp[].allowed_tools`) and the syscall gate
reached via ADR-024's `pre_validate` clauses
(`tools.filesystem.*`).

## What this demonstrates

The agent (Qwen 1.5B) emits three MCP tool calls in one turn against
the upstream Anthropic filesystem MCP server. Each call exercises a
different layer:

| # | Model emits | MCP allowlist | Pre-validate (ADR-024) | Result | Resource URI |
|---|---|---|---|---|---|
| 1 | `fs-mcp__read_text_file({path: "/data/research-notes.txt"})` | ✅ allow | ✅ allow (`/data` in `tools.filesystem.read`) | **Access** | `mcp://fs-mcp/read_text_file` |
| 2 | `fs-mcp__read_text_file({path: "/etc/passwd"})` | ✅ allow | ❌ **DENY** (`/etc` not in `tools.filesystem.read`) | **Violation** | `mcp-prevalidate://fs-mcp/read_text_file?path=/etc/passwd` |
| 3 | `fs-mcp__write_file({path: "/tmp/out.txt", contents: "..."})` | ❌ **DENY** (`write_file` not in `allowed_tools`) | n/a | **Violation** | `mcp://fs-mcp/write_file` |

The story: **two layers, both Aegis-controlled**. The MCP allowlist
gates protocol-level intent; the pre-validate clause gates the
underlying side effect against the same `tools.filesystem.*` policy
that gates direct mediator calls. The two Violations carry
distinguishable `resource_uri` schemes — `mcp://` for an allowlist
denial, `mcp-prevalidate://` for a pre-validate denial — so an
auditor can tell which layer refused at a glance.

## What you see in the GIF

| t | Frame | Note |
|---|---|---|
| 0–4s | `grep -A1 'allowed_tools:'` showing the manifest's pre_validate clauses | Two-layer setup: MCP allowlist + per-tool pre_validate |
| 4–22s | `aegis run --prompt "..."` runs the model | Qwen emits 3 tool calls; CLI prints `# tool[0]/[1]/[2]` lines |
| 22–25s | `grep entryType=access` shows the F2 Access entry | Call 1 — both layers permitted |
| 25–29s | `grep mcp_pre_validate` shows the pre-validate Violation | Call 2 — `mcp-prevalidate://fs-mcp/read_text_file?path=/etc/passwd` |
| 29–34s | `grep 'mcp://fs-mcp/write_file'` shows the allowlist Violation | Call 3 — `mcp://fs-mcp/write_file` (no `prevalidate` segment) |

## Why two layers (and why both)

A single layer is brittle:

- **MCP-allowlist only** would refuse calls outside the catalog
  (`write_file`) but couldn't catch `read_text_file({path:
  "/etc/passwd"})` — the protocol intent is allowed; only the path
  is forbidden.
- **Filesystem-gate only** would catch the `/etc/passwd` read at
  the syscall level (assuming the MCP server flows through an
  Aegis-mediated fs gate, which it doesn't — MCP servers run in
  their own process and reach the OS directly). Without the
  pre-validate hook, the syscall gate doesn't see the call at all.

ADR-024's pre_validate clauses connect the two: the manifest
declares "this MCP tool's `path` arg is the side-effect," and the
mediator runs the same `policy.check_filesystem_read` it would run
for a direct `mediate_filesystem_read` call. The MCP server's
process-level enforcement remains the first line of defense; the
pre_validate clause makes the syscall-shaped check enforceable from
the Aegis side without trusting the upstream server's promises.

## F-feature mapping

| Feature | Where it lights up in this demo |
|---|---|
| **F1 Workload Identity** | SessionStart entry binds the SVID to the (manifest, model, chat-template) digest triple. |
| **F2 Permission Manifest** (MCP allowlist) | `tools.mcp[].allowed_tools` refuses Call 3 with `mcp://fs-mcp/write_file`. |
| **F2 Permission Manifest** (filesystem gate via pre_validate) | `tools.mcp[0].allowed_tools[*].pre_validate` refuses Call 2 with `mcp-prevalidate://fs-mcp/read_text_file?path=/etc/passwd`. |
| **F4 Access Log** | Call 1 lands as an Access entry; the auditor sees what the agent read. |
| **F9 Hash-chained ledger** | All three entries chain into the session's tamper-evident ledger; `aegis verify` confirms the root hash. |

## Run locally

```bash
make -C demos 01-mcp-sandboxed-twice
```

That single command runs `setup.sh` (one-time-per-machine setup, then
no-op) and renders the demo. Prerequisites: `aegis` CLI built with
`--features llama`, plus `oras`, `cosign`, and the upstream
`@modelcontextprotocol/server-filesystem` server reachable as
`/usr/bin/mcp-server-filesystem` (a symlink the demos/Dockerfile
provides for CI; install hint: `npm install -g
@modelcontextprotocol/server-filesystem` then `ln -sf
/usr/local/bin/mcp-server-filesystem /usr/bin/...`).

### What `setup.sh` does

1. `aegis pull` the cosign-verified Qwen 2.5 1.5B Q4_K_M GGUF (cached
   at `~/.cache/aegis/models/c7404a910e65596a185e788ede19e09bc017dc3101cd106ba7d65fe1dd7dec37/`).
2. Symlink the model + chat-template sidecar into
   `/tmp/aegis-demo-01/`.
3. Symlink `manifest.yaml` into the workdir so `demo.tape` can use a
   workdir-local path (no checkout-prefix dependency).
4. Write the sample `research-notes.txt` into
   `/tmp/aegis-demo-01/data/`.

## Reproducibility

Per ADR-020 hard requirements, `manifest.yaml` pins
`inference.determinism` (seed 42 + temperature 0). With the same
prompt + the same Qwen blob, every render produces byte-identical
text output. The CI snapshot test (Phase 2b, separate PR) will gate
on the GIF's SHA-256.

## Why Qwen 1.5B (not Gemma 4)

Per the per-demo model selection table in ADR-020 §"Decision" item 8,
Demo 1 stays on **Qwen 1.5B** because pre-validation is mechanical
— the model just needs to emit three named MCP tool calls in
sequence, and seed 42 + temperature 0 makes the choice deterministic.
Larger models would inflate the GIF without improving the security
story.

## Related

- [ADR-018](../../docs/adrs/018-adopt-mcp-protocol-for-agent-tool-boundary.md) — MCP as the agent-tool boundary.
- [ADR-024](../../docs/adrs/024-mcp-args-prevalidation.md) — per-tool pre-validation: the architectural decision this demo exists to demonstrate.
- [`schemas/manifest/v1/examples/agent-with-mcp.manifest.yaml`](../../schemas/manifest/v1/examples/agent-with-mcp.manifest.yaml) — the schema-fixture variant of the same shape (broader Anthropic-fs-MCP catalog; this demo's manifest trims it for narrative clarity).
