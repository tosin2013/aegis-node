# 33. WebUI Visual MCP Server Management

**Status:** Proposed
**Date:** 2026-05-05
**Domain:** UI / MCP (extends [ADR-018](018-adopt-mcp-protocol-for-agent-tool-boundary.md), [ADR-024](024-mcp-args-prevalidation.md), supports [ADR-031](031-community-webui-for-local-collaboration.md))
**Targets:** v0.9.5 Phase 1d

## Context

[ADR-018](018-adopt-mcp-protocol-for-agent-tool-boundary.md) made
MCP the agent-to-tool boundary. The manifest's `tools.mcp[]` array
declares which servers and tools the agent can reach; everything is
closed-by-default.

In CLI workflows that's manageable for one or two MCP servers — the
Examples 02 + 06 manifests have 1–2 servers each. As the surface area
grows (operator wires up filesystem, sqlite, web, github, gmail, …)
hand-editing `tools.mcp[]` becomes a source of two distinct bugs:

1. **Stale allowlists.** Operator adds a new MCP server but copies
   the wrong tool names into `allowed_tools`. The server exposes
   `read_text_file` but the manifest grants `read_file`. Every call
   denies; the agent's tool catalog has phantom entries.
2. **Over-grant by copy-paste.** Operator copies an example
   `allowed_tools` block from another manifest and forgets to trim.
   The new server is granted tools that don't exist there but exist
   elsewhere — silently shifting blast radius if a new MCP server is
   later added with that tool name.

[ADR-024](024-mcp-args-prevalidation.md)'s `pre_validate` mappings
amplify both bugs because they reference per-tool argument shapes;
typing them by hand against an MCP server's actual JSON schema is
error-prone.

The Community UI ([ADR-031](031-community-webui-for-local-collaboration.md))
already builds and validates manifests. ADR-033 promotes MCP from
"another manifest field operators hand-edit" to "first-class
discoverable management surface."

## Decision

**Add an "Integrations" / "Tools" tab to the Community UI that
catalogs known MCP servers, queries them for their tool list and
JSON schemas, and lets operators visually toggle `tools.mcp[]`
allowlist entries with checkboxes. The tab also renders the
[ADR-024](024-mcp-args-prevalidation.md) `pre_validate` mappings
inferred from each tool's schema, with operator override.**

### Catalog data shape

Each MCP server entry in the catalog carries:

| Field | Source |
|---|---|
| `server_name` | Operator-provided (matches the manifest's `tools.mcp[].server_name`) |
| `server_uri` | Operator-provided (`stdio:npx -y …`, `https://…`, etc.) |
| `discovered_tools` | Live `tools/list` MCP RPC against the server |
| `tool_schemas` | Per-tool JSON schema returned by the server |
| `pre_validate_inferred` | Mapping derived from each tool's schema (e.g., a tool with `path: string` arg → `kind: filesystem_read, arg: path`) |
| `allowlist_state` | Set of tool names operator has checked |
| `last_discovered_at` | Timestamp; refresh button re-runs discovery |

The catalog is **session-scoped state in the UI**, not a manifest
artifact. Saving to manifest serializes `allowlist_state` +
`pre_validate` overrides into the YAML.

### Discovery flow

1. **Add server.** Operator clicks "Add MCP Server", fills `name` +
   `server_uri`, clicks Save.
2. **Discovery.** UI server runs the existing `crates/mcp-client/`
   stdio transport against the server, sends `tools/list`. Returns
   the tool list + schemas. (Re-uses the FastMCP-aware notification
   handling already in v0.9.0.)
3. **Render.** UI shows a table per server:
   ```
   fs-mcp                                        [ Refresh ] [ Remove ]
   ┌──────────────────────────────────────────────────────────────┐
   │ ☑ read_text_file       (path: string)                       │
   │   pre_validate: filesystem_read, arg=path  [override...]    │
   │ ☑ read_multiple_files  (paths: array<string>)               │
   │   pre_validate: filesystem_read, arg_array=paths            │
   │ ☐ list_directory       (path: string)                       │
   │ ☐ list_allowed_directories                                  │
   └──────────────────────────────────────────────────────────────┘
   ```
4. **Toggle.** Operator checks/unchecks tools. UI updates
   `allowlist_state`.
5. **Save to manifest.** Operator clicks "Apply to manifest". UI
   serializes the catalog state into `tools.mcp[]` entries on the
   active manifest, runs `aegis validate`, surfaces any warnings.

### `pre_validate` inference

For each tool the server returns, the UI inspects the JSON schema
and proposes a [ADR-024](024-mcp-args-prevalidation.md) mapping:

| Schema shape | Inferred clause |
|---|---|
| `{ path: string }` | `kind: filesystem_read, arg: path` (or `_write` if tool name matches `^(write_*|delete_*|truncate_*)$`) |
| `{ paths: string[] }` | `kind: filesystem_read, arg_array: paths` |
| `{ url: string }` | `kind: network_outbound, arg: url` |
| `{ command: string, args?: string[] }` | `kind: exec, arg: command` |

The inference is conservative — when in doubt, default to the
read-shaped clause and surface a warning in the UI ("review this:
read or write?"). Operators override with a free-form editor.
[ADR-024](024-mcp-args-prevalidation.md)'s schema is the contract;
the UI is a generator for it.

### Closed-by-default reinforced

The default state of every checkbox is **unchecked**. Adding a
server doesn't auto-grant any tool. Operators check the boxes they
want, manifest serializes only those. This mirrors the runtime's
posture: discovery does not equal authorization.

The catalog also distinguishes "tools the server exposes" from
"tools the agent is granted." Operators see the full surface
(closing the "what tools exist?" knowledge gap that drives copy-
paste bugs) without granting anything they don't intend.

## Why not the alternatives

- **Auto-grant all tools when a server is added.** Inverts the
  posture from closed-by-default to open-by-default. Rejected.
- **Make `pre_validate` discovery the operator's responsibility.**
  Status quo. Hand-typing schema-derived mappings is a major source
  of misconfiguration. Inference + override is materially better.
- **Skip discovery; trust the operator's manual `allowed_tools`
  list.** What the CLI does today. Works for small numbers of
  servers; doesn't scale, and the v0.9.0 firecrawl experience
  showed how easy it is to mis-name tools (`search` vs.
  `firecrawl_search`).
- **Centralized MCP server registry.** A future enhancement (the
  "Integrations marketplace" pattern). Out of scope for v0.9.5;
  v1.x decision.
- **Run discovery on every manifest edit.** Wasteful; servers may
  spawn child processes. Cache discovery per server, refresh on
  user click.

## Implementation tracking

- UI: `ui/src/pages/Integrations.tsx`, `ui/src/components/McpToolCatalog.tsx`.
- Backend: `crates/ui-server/src/handlers/mcp.rs` exposes
  `POST /api/v1/mcp/discover` (runs `tools/list` against a given
  `server_uri`), `POST /api/v1/manifests/:id/mcp` (serialize catalog
  state into the manifest).
- Reuses: `crates/mcp-client/src/lib.rs` (the existing FastMCP-
  aware client). Schema inference logic lives in
  `crates/policy/src/mcp_schema_infer.rs` (new, small, pure rust)
  so the same logic is testable and shared with `aegis validate`
  warnings.
- Tracking issue: see v0.9.5 milestone tracker.

## Open questions for follow-up

- **Server health monitoring.** Should the catalog show whether each
  server is currently reachable / responsive? Useful for
  troubleshooting, but adds polling load. Lean: opt-in indicator,
  refresh-on-demand by default.
- **Schema drift detection.** When a server's tool schema changes
  between two discovery runs, surface the diff. The previous
  `pre_validate` mapping may no longer be correct. Lean: warn on
  refresh; never silently overwrite operator-confirmed mappings.
- **Sandboxed MCP wrappers.** [ADR-024](024-mcp-args-prevalidation.md)'s
  rejected alternative (Linux user-namespace + seccomp jail per MCP
  server) becomes more attractive once operators run dozens of
  servers. The UI tab would surface "this server runs sandboxed"
  vs. "this server has full host access." Not v0.9.5; revisit when
  malicious-MCP threat models become a customer concern.

## References

- [ADR-018](018-adopt-mcp-protocol-for-agent-tool-boundary.md) MCP boundary
- [ADR-024](024-mcp-args-prevalidation.md) `pre_validate` mappings
- [ADR-028](028-adversarial-pre-filter-gate.md) post-tool result filtering
- [ADR-031](031-community-webui-for-local-collaboration.md) Community UI
- `crates/mcp-client/src/lib.rs` FastMCP-aware client (v0.9.0)
