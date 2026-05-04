# Example 02 — research assistant via MCP (filesystem + optional Firecrawl)

The agent uses the official Anthropic filesystem MCP server to read a small
docs corpus and writes a one-paragraph research summary to disk. Set
`FIRECRAWL_API_KEY` and the same example extends to live web research via
the Firecrawl MCP server — same agent, same enforcement model, two MCP
servers composed.

## Why this matters in 2026

Two things converge here:

1. **MCP is the protocol layer for AI agents in 2026.** Per the *OWASP Q1
   2026 GenAI Exploit Round-up*, the ClawHub MCP-skill registry incident
   confirmed five of the seven most-downloaded MCP skills as malware. Any
   agent runtime that runs MCP servers without per-tool allowlists +
   per-arg pre-validation is a soft target for the agentic supply-chain
   attack pattern OWASP Top 10 calls "agentic supply chain vulnerabilities."
2. **Knowledge work is the #4 enterprise AI use case** (per *Databricks
   Enterprise AI Agent Trends* and *Lyzr State of AI Agents Q1 2026*) —
   right behind customer support, coding, and finance. RAG-over-internal-
   docs is the canonical first deployment.

Aegis-Node's two-layer MCP enforcement (`tools.mcp[].allowed_tools` for
the protocol allowlist + ADR-024 `pre_validate` for argument-level
side-effect mapping) is the direct counter to *both* OWASP risks above:
tool misuse and agentic supply chain.

## What you'll see

- The model emits MCP tool calls: `fs-mcp__list_directory`,
  `fs-mcp__read_text_file` for each fixture
- The mediator allows each (paths fall under
  `/tmp/aegis-example-02/fixtures/docs`, which the manifest grants)
- The agent emits `filesystem__write` to `output/research-summary.md`
- Mediator allows the write (covered by `tools.filesystem.write`)
- The summary file lands at `output/research-summary.md` with citations
- `ledger-*.jsonl` shows F4 entries for each MCP read + the F2/F7 write
- `aegis verify` reports chain integrity intact

## Run it

### Default mode (offline, no API key needed)

```bash
bash setup.sh
cd /tmp/aegis-example-02
aegis run --manifest manifest.yaml --model model.gguf \
    --workload research-assistant --instance inst-001 \
    --prompt "$(cat prompt.txt)"
cat output/research-summary.md
aegis verify ledger-*.jsonl
```

### Extended mode (live web research via Firecrawl)

```bash
export FIRECRAWL_API_KEY=fc-...        # your Firecrawl key
bash setup.sh                          # detects the env var, wires manifest.firecrawl.yaml
cd /tmp/aegis-example-02
aegis run --manifest manifest.yaml --model model.gguf \
    --workload research-assistant --instance inst-001 \
    --prompt "$(cat prompt.txt)"
```

In extended mode, the ledger gains entries for `firecrawl__search` and
`firecrawl__scrape` calls, plus F6 NetworkAttestation entries showing the
allowed connections to `api.firecrawl.dev:443`.

## What just happened

| F-feature | What enforced it | ADR |
|---|---|---|
| **F2 Permission Manifest** | Mediator gated each filesystem read against `tools.filesystem.read` | [ADR-009](../../docs/adrs/009-permission-manifest-format.md) |
| **F2-MCP allowlist** | `tools.mcp[].allowed_tools` accepts `read_text_file`, `list_directory`, etc.; rejects anything else (write_file, edit_file) at the protocol layer | [ADR-018](../../docs/adrs/018-adopt-mcp-protocol-for-agent-tool-boundary.md) |
| **F2-MCP pre_validate** | `pre_validate: filesystem_read` extracts the `path` arg from each call, runs it through `tools.filesystem.read` *before* dispatch — so even if the MCP server were malicious, paths outside the grant land as F2 violations | [ADR-024](../../docs/adrs/024-mcp-tool-arg-pre-validation.md) |
| **F4 Access Log** | Every MCP tool call lands in the ledger as a typed F4 entry with `mcp__<server>__<tool>` access type | [ADR-010](../../docs/adrs/010-trajectory-ledger-format.md) |
| **F6 Network policy (extended mode)** | `tools.network.outbound.allowed: [api.firecrawl.dev:443]` permits exactly that host:port; everything else stays denied + lands as F2 violation | [ADR-014](../../docs/adrs/014-network-deny-by-default.md) |
| **F9 Hash-Chained Ledger** | Every entry hash-chains to the previous; `aegis verify` confirms integrity end-to-end | [ADR-010](../../docs/adrs/010-trajectory-ledger-format.md) |

## Inspect the artifacts

```bash
# The work product — research summary with source citations
cat output/research-summary.md

# Every MCP call landed as an F4 entry
grep mcp__ ledger-*.jsonl | jq -c '{accessType, resourceUri}'

# The write of the summary itself
grep 'accessType.:.write' ledger-*.jsonl | jq -c '{resourceUri, bytesAccessed}'

# Chain integrity
aegis verify ledger-*.jsonl
```

The summary's content + the F4 entries + the F9 chain together answer
"what did this research agent do?" with both the work product and a
tamper-evident audit trail.

## Make it yours

- **Add your own docs** — drop new `.md` files into `fixtures/docs/` and
  re-run. The agent reads everything in there.
- **Change the prompt** — ask the agent to summarize a different angle
  (security posture, hiring philosophy, anything the docs touch on).
- **Tighten the MCP allowlist** — remove `read_multiple_files` from
  `manifest.default.yaml`'s `allowed_tools`. The agent will fall back to
  individual reads (F4 entry per file). Watch how the ledger structure
  reflects the policy choice.
- **Loosen the path grant** — extend `tools.filesystem.read` to cover a
  directory outside `fixtures/docs/` and prompt the agent to look there.
- **Swap the MCP server** — replace `mcp-server-filesystem` with another
  server from the [modelcontextprotocol catalog](https://modelcontextprotocol.io/servers)
  (GitHub, Slack, Postgres, Memory, Sequentialthinking). The
  `manifest.firecrawl.yaml` shape is the template for any
  API-key-required external MCP server — fork it for your own use case.

## What you should see when you tighten the manifest

Edit `manifest.default.yaml` to remove `list_directory` from `allowed_tools`.
Re-run; the agent's first MCP call (`fs-mcp__list_directory`) lands as an F2
violation in the ledger naming `mcp://fs-mcp/list_directory`, and the agent
falls back to reading files by name (which still works because
`read_text_file` is allowed). Tighten further by removing `read_text_file`
too — now the agent has no way to read files via MCP, the summary is never
produced, and the ledger captures every denied attempt. `aegis verify`
still passes — violations are themselves part of the chain.
