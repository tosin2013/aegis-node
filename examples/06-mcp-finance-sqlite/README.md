# Example 06 — finance/ops expense audit via SQLite MCP

The agent queries a synthetic Q2 expense database via the SQLite MCP
server, identifies anomalies, and writes a report to disk under F3
human approval. Two MCP servers across the example set (filesystem in
Example 02; SQLite here) — same enforcement model, different
capability surface.

## Why this matters in 2026

Finance / operations is the **#3 enterprise AI use case** in 2026 —
*30–50% close acceleration* per *Lyzr State of AI Agents Q1 2026*. It
also lands hardest on the OWASP **"tool misuse"** Top-10 risk, because
giving an agent SQL access to your books is the textbook overpermissioned-
agent scenario. Most agent runtimes that bolt on MCP have no way to
prevent the agent from calling `write_query` or `create_table` if the
upstream server advertises them.

Aegis-Node's `tools.mcp[].allowed_tools` allowlist is the protocol-
layer counter: this manifest only permits `read_query`, `list_tables`,
and `describe_table`. Any DDL or DML the model attempts (DROP, DELETE,
UPDATE, CREATE) gets refused at the MCP-allowlist layer *regardless of
what mcp-server-sqlite advertises in its capabilities response*. The F3
approval gate on the report write is the second layer — even read-only
agents need a human signature before their work product lands.

Source: *Lyzr State of AI Agents Q1 2026*; *OWASP Top 10 for Agentic
Applications 2026*; *Sema4.ai 10 AI Agent Use Cases Transforming
Enterprises in 2026* (finance #3 by 30-50% close acceleration metric).

## What you'll see

- The agent emits MCP calls: `sqlite__list_tables`,
  `sqlite__describe_table`, `sqlite__read_query`
- The mediator allows each (all three are in `allowed_tools`)
- The agent emits `filesystem__write` for the report
- Mediator upgrades to `RequireApproval`; F3 file channel grants
- Report at `output/q2-expense-anomalies.md` lists flagged expenses
- Ledger shows F4 entries per SQL query + the F3 approval + F2/F7 write
- `aegis verify` confirms the chain

## Run it

```bash
bash setup.sh
cd /tmp/aegis-example-06
AEGIS_APPROVAL_FILE=/tmp/aegis-example-06/approval.json \
aegis run --manifest manifest.yaml --model model.gguf \
    --workload finance-auditor --instance inst-001 \
    --prompt "$(cat prompt.txt)"
cat output/q2-expense-anomalies.md
grep mcp__sqlite ledger-*.jsonl | jq -c '{accessType, resourceUri}'
aegis verify ledger-*.jsonl
```

## What just happened

| F-feature | What enforced it | ADR |
|---|---|---|
| **F2-MCP allowlist** | Only `read_query`, `list_tables`, `describe_table` accepted; `write_query` etc. denied at the protocol layer | [ADR-018](../../docs/adrs/018-adopt-mcp-protocol-for-agent-tool-boundary.md) |
| **F2 Permission Manifest** | Filesystem writes gated by `tools.filesystem.write`; reads of the DB path gated by `tools.filesystem.read` | [ADR-009](../../docs/adrs/009-permission-manifest-format.md) |
| **F3 Human Approval Gate** | Report write upgraded to `RequireApproval`; F3 file channel read approval.json | [ADR-005](../../docs/adrs/005-f3-human-approval-gate.md) |
| **F4 Access Log** | Every SQL query lands as `mcp__sqlite__read_query` F4 entry naming the query string | [ADR-010](../../docs/adrs/010-trajectory-ledger-format.md) |
| **F9 Hash-Chained Ledger** | The chain proves the agent's queries, the approval, and the report write are all linked end-to-end | [ADR-010](../../docs/adrs/010-trajectory-ledger-format.md) |

## Inspect the artifacts

```bash
# The work product — anomaly report
cat output/q2-expense-anomalies.md

# Every SQL query the agent ran (the audit trail)
grep mcp__sqlite ledger-*.jsonl | jq -c '{accessType, resourceUri, args}'

# F3 approval evidence
grep approval ledger-*.jsonl | jq .

# Chain integrity
aegis verify ledger-*.jsonl
```

The anomaly report (work product) plus every SQL query (audit trail)
plus the approval signature plus the chain integrity is the *complete*
governance answer for a finance audit — every question a CFO or
auditor could reasonably ask is in this one ledger + the artifact.

## Make it yours

- **Bring your own data** — replace `seed.sql` with your own schema
  + data. Adjust the prompt to ask different questions.
- **Add categories of analysis** — extend the prompt to also flag
  duplicate-vendor patterns, weekend submissions, etc. The manifest
  doesn't change; only the prompt does.
- **Tighten to a single query shape** — the SQLite MCP server
  separates `read_query` from `write_query`; you can further restrict
  which read queries are allowed by adapting the server itself (or
  fronting it with a query-allowlist proxy MCP server).
- **Swap to Postgres** — the same MCP pattern works with
  `mcp-server-postgres` for a production-shape database. Manifest
  changes are minimal: server_uri + db connection string.
- **Add a $-threshold approval** — split `write_grants` into two:
  one for the report itself (single approval), and a separate
  `mcp__sqlite__read_query` pre_validate clause that flags queries
  returning anomalies >$10k for re-approval. Out of scope for this
  example; track in your own implementation.

## What you should see when you tighten the manifest

Edit `manifest.yaml` to add `"write_query"` to `allowed_tools`. Re-run
with a prompt that asks the agent to `INSERT INTO expenses` a fake
row. The MCP allowlist now permits `write_query`; the SQLite MCP
server actually executes the INSERT; the row lands in the database;
the F4 ledger entry shows what was written. *That's why you don't add
it.* Restore the read-only allowlist; re-run; the same prompt now
lands as an F2 violation at the MCP-allowlist layer with `resourceUri:
mcp://sqlite/write_query`. The lesson: closed-by-default at the
protocol allowlist means a model emitting a destructive call still
runs through Aegis's policy gate, *before* anything reaches the
upstream MCP server.

For a deeper SQL-injection-style demo: prompt the agent to call
`sqlite__read_query` with `"SELECT * FROM expenses; DROP TABLE
expenses; --"`. Most SQLite MCP servers' `read_query` rejects
multi-statement queries, but the F4 entry captures the full attempted
SQL — so even when the upstream layer catches the attack, the audit
trail names the attempt. Aegis catches what the model *tried*, not
just what succeeded.
