# 24. MCP Tool-Arg Pre-Validation — The Second Layer for `tools.mcp[]`

**Status:** Accepted
**Date:** 2026-05-01
**Domain:** Runtime / mediator (extends [ADR-018](018-adopt-mcp-protocol-for-agent-tool-boundary.md), supports [ADR-020](020-recorded-demo-program.md) Demo 1)

## Context

ADR-018 frames the Model Context Protocol as the agent-to-tool
boundary. The runtime's `mediate_mcp_tool_call` checks
`tools.mcp[].allowed_tools` (the protocol-level allowlist), then
dispatches to the MCP server. The MCP server runs in a separate
process; its underlying side-effects — `read_text_file` issuing a
real `read(2)`, `network_fetch` issuing a real `connect(2)` — reach
the OS directly. The mediator only sees the result.

The fixture comment at
[`schemas/manifest/v1/examples/agent-with-mcp.manifest.yaml`](../../schemas/manifest/v1/examples/agent-with-mcp.manifest.yaml)
claims:

> 2. The MCP server's underlying side effects (e.g. read_text_file
>    issuing a real fs read) flow through Session::mediate_filesystem_*
>    and are authorized by tools.filesystem.read separately.

That claim is **aspirational** today. [ADR-020](020-recorded-demo-program.md)
Demo 1 ("MCP, sandboxed twice") depends on it being true; the demo
was filed as blocked on this gap in
[#91](https://github.com/tosin2013/aegis-node/issues/91) while
shipping Demo 1's prep PR.

Three approaches were enumerated in #91:

1. **Pre-dispatch arg validation** — inspect tool args before
   `client.call_tool` and run the relevant `tools.filesystem.*`
   check. Lightweight; works for filesystem-shaped MCPs; needs
   per-server schema knowledge.
2. **Sandboxed MCP wrapper** (Linux user-namespace + seccomp) —
   launch the MCP server inside a jail that surfaces only
   `tools.filesystem.*`-allowed directories. Heavy but generalizes
   to any MCP server, including ones lying about their side-effects.
3. **Aegis-Node MCP shim** — ship our own filesystem MCP that
   routes every fs op through `mediate_filesystem_read/write`.
   Cleanest but ties Aegis-Node to one MCP server impl.

Approach (1) is the cheapest tactical win. Approach (2) is the
right long-term answer for malicious-MCP threat models. Approach (3)
is conceptually cleanest but loses ADR-018's "any MCP server" promise.

## Decision

**Ship a generalized version of (1) — manifest-declared tool
side-effect mapping — as Phase 1.** Defer (2) sandbox-style
enforcement to a future ADR if/when malicious-MCP threat models
become a customer concern.

Concretely:

1. **Schema extension.** `tools.mcp[].allowed_tools` grows from
   `Vec<String>` to `Vec<AllowedTool>` where each `AllowedTool` may
   be either:
   - the existing string shorthand (`"read_text_file"`) — interpreted
     as "no pre-validation; one-layer enforcement," preserving today's
     behavior bit-for-bit, **or**
   - an object with the side-effect mapping:

     ```yaml
     allowed_tools:
       - name: "read_text_file"
         pre_validate:
           - kind: filesystem_read
             arg: path
       - name: "read_multiple_files"
         pre_validate:
           - kind: filesystem_read
             arg_array: paths
       - name: "search_files"
         pre_validate:
           - kind: filesystem_read
             arg: path
       - name: "write_file"
         pre_validate:
           - kind: filesystem_write
             arg: path
       - name: "fetch"
         pre_validate:
           - kind: network_outbound
             arg: url     # mediator parses host + port from URL
     ```

   The string and object shapes coexist — operators upgrade
   per-tool when they want the second layer.

2. **Runtime check.** `mediate_mcp_tool_call` adds a pre-validation
   pass between the MCP allowlist check and the `client.call_tool`
   dispatch:

   ```text
   rebind                                                  (existing)
   policy.check_mcp_tool(server, tool)                     (existing)
   ── if allowed_tool entry has pre_validate clauses:
        for each clause:
          extract the arg by name from request.arguments
          run policy.check_filesystem_*/check_network_outbound
          on Deny: emit Violation, return Error::Denied   (NEW)
   client.call_tool(server_uri, tool, args)                (existing)
   emit Access entry                                        (existing)
   ```

   The Violation entry's `resource_uri` carries the layer
   (`mcp-prevalidate://<server>/<tool>?path=<path>`) so an auditor
   can distinguish "MCP allowlist denied" from "filesystem gate
   denied via MCP pre-validation."

3. **Default = current behavior.** A manifest that doesn't declare
   `pre_validate` clauses gets exactly today's single-layer
   enforcement — backwards compatible. The fixture
   `agent-with-mcp.manifest.yaml` is updated to declare the mapping
   for the Anthropic filesystem MCP's tool catalog, so Demo 1 can
   record against it.

4. **Two-layer claim is now opt-in.** The fixture's comment
   ("authorized by tools.filesystem.read separately") becomes
   accurate **for manifests that declare the mapping**. Operators
   choose: declare the mapping for tight enforcement, or stay on
   the one-layer path for compatibility with MCP servers whose tool
   schemas they don't want to audit.

## Why not the alternatives

- **Heuristic path extraction.** Scan tool args for any string
  field that looks path-shaped (starts with `/`, contains `/`,
  etc.) and run `check_filesystem_*` on each. Cons: false positives
  (a path-shaped value that isn't actually a path), false negatives
  (relative paths the heuristic skips), and worst — security
  decisions driven by heuristics are fragile. Rejected.
- **Sandboxed MCP wrapper (#91 option 2).** Right answer for
  malicious MCP servers — those that lie about their side-effects.
  But: heavy infrastructure (Linux user-namespaces + seccomp);
  cross-platform issues (macOS/Windows enforcement is different);
  hard to reason about for operators. Filed for a future ADR;
  ADR-024's manifest-declared mapping is composable with it (the
  declaration becomes the *contract* the sandbox enforces).
- **Aegis-Node MCP shim (#91 option 3).** Ties our story to one
  MCP server impl, contradicts ADR-018's "any MCP server" framing.
  Operators who want a higher-assurance filesystem MCP can ship
  their own shim and reference it via `server_uri` — that's an
  operator choice, not a runtime architecture decision.
- **Add a new top-level policy section
  `mcp_tool_side_effects:`.** Considered. Rejected because the
  declaration's *scope* is per-tool inside an `allowed_tools` entry
  — putting it elsewhere creates two cross-references (server name
  AND tool name) the manifest validator has to keep in sync.

## Consequences

### Positive

- **Demo 1 (MCP, sandboxed twice) becomes recordable.** The fixture
  manifest declares the mapping; the runtime denies
  `read_text_file({path: "/etc/passwd"})` at pre-validation; the
  ledger names the second layer in the Violation `resource_uri`.
- **Backwards-compatible.** Existing manifests work unchanged. The
  schema extension is additive (`allowed_tools` accepts the union
  type).
- **Operator-controlled.** The manifest is the source of truth for
  which side-effects are gated. No runtime knowledge of specific
  MCP servers' schemas — operators declare what they care about.
- **Composable with future sandbox enforcement.** When the future
  ADR adds Linux-namespace sandboxing, the same manifest declaration
  becomes the contract the sandbox enforces. The pre-validation
  step in the mediator is a defense-in-depth layer, not a
  replacement.

### Negative

- **Operators must read each MCP server's tool catalog** to declare
  meaningful mappings. Today the Anthropic filesystem MCP's catalog
  is small and well-documented; for less-documented servers, this
  is real work. Mitigation: ship an `aegis mcp introspect` helper
  in a future PR that runs the MCP `tools/list` request and
  produces a starter `pre_validate` block (operators review, edit,
  commit).
- **Schema validator complexity.** `allowed_tools` becoming a union
  type complicates the JSON Schema and the Go/Rust deserializers.
  Mitigation: use serde's `#[serde(untagged)]` (Rust) and a
  `oneOf` (JSON Schema) — both well-supported. Tests in the
  conformance battery cover both shapes.
- **Doesn't address malicious MCP servers.** A server claiming
  `read_text_file({path})` semantics but actually writing files is
  not caught here. That threat model is real for unaudited MCP
  servers — but it's a different problem, addressed by Phase 2
  sandbox enforcement (filed as a future ADR). Today operators are
  responsible for vetting which MCP servers they grant.

## Implementation plan

Three sub-issues, executed in order:

**ADR-024-A: Schema bump.**
- `schemas/manifest/v1/manifest.schema.json` — `allowed_tools`
  becomes `oneOf [string, AllowedTool]`. New `AllowedTool` and
  `PreValidateClause` definitions.
- `pkg/manifest/types.go` — Go union type via custom YAML/JSON
  unmarshaller.
- `crates/policy/src/manifest.rs` — Rust union via
  `#[serde(untagged)]`.
- Embedded schema (`pkg/manifest/schema_v1.json`) re-synced.
- Conformance battery (`tests/conformance/cases.json`) gains a
  fixture that exercises both shapes.
- Schema-valid test corpus (`schemas/manifest/v1/examples/`)
  gains an updated `agent-with-mcp.manifest.yaml` that uses
  `pre_validate`.

**ADR-024-B: Mediator pre-validation pass.**
- `crates/inference-engine/src/mediator.rs::mediate_mcp_tool_call`
  inserts the pre-validate loop between the MCP allowlist check
  and `client.call_tool`.
- Each clause's `kind` maps to the corresponding `policy.check_*`
  method. Denied → emit Violation with the
  `mcp-prevalidate://<server>/<tool>?<arg>=<value>` resource URI,
  return `Error::Denied`.
- New unit tests in `crates/inference-engine/tests/mediator.rs`
  covering: allowed (path in `tools.filesystem.read`),
  denied-by-pre-validate (path NOT in `tools.filesystem.read`),
  no-pre-validate (string-shape allowed_tool keeps current
  behavior).

**ADR-024-C: Demo 1 (MCP, sandboxed twice).**
- `demos/01-mcp-sandboxed-twice/` with the now-functional manifest,
  prompt that coaxes three calls (allowed path / pre-validate-denied
  path / not-in-allowed-tools), VHS recording, README.
- Closes ADR-020 §"Six scenarios" item 1.

## Related

- [ADR-018 Adopt the Model Context Protocol](018-adopt-mcp-protocol-for-agent-tool-boundary.md)
  — frames MCP as the agent-tool boundary; ADR-024 is the
  second-layer enforcement that makes the boundary's "two layers"
  claim true.
- [ADR-020 Recorded Demo Program](020-recorded-demo-program.md)
  — Demo 1 ("MCP, sandboxed twice") depends on this.
- [#91 F2-MCP-E](https://github.com/tosin2013/aegis-node/issues/91)
  — the original gap report; this ADR is the architectural answer.
- Future ADR (filed when needed): **MCP server sandbox enforcement
  via Linux user-namespaces + seccomp** — the malicious-MCP
  threat-model answer; composable with this ADR's manifest
  declaration.
