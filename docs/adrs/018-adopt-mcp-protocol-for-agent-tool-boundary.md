# 18. Adopt the Model Context Protocol (MCP) as the Agent-to-Tool Boundary

**Status:** Accepted
**Date:** 2026-04-29
**Domain:** Tool Integration / Agent Runtime (extends F2 + F5)

## Context

[ADR-007](007-pre-execution-reasoning-trajectory.md) (F5 Pre-Execution Reasoning Trajectory) anticipates a reasoning capturer that "intercepts LLM tool-selection output" — but no protocol has ever been named for what produces that output. The JSON-LD ledger at `schemas/ledger/v1/context.jsonld` has reserved `toolsConsidered` and `toolSelected` terms since v0.1.0 yet they remain semantically empty. Meanwhile the manifest's `tools.apis[]` (`schemas/manifest/v1/manifest.schema.json`) is an HTTP-verb allowlist — useful for network-policy purposes but not a tool catalog. It carries no `description`, no `input_schema`, no machinery for tool discovery.

Operators integrating Aegis-Node need a standard tool-invocation protocol so the runtime can mediate tool calls the same way it already mediates filesystem and network syscalls. Reinventing one would forfeit the broader Anthropic Model Context Protocol (MCP) ecosystem and burden every tool author with a bespoke Aegis-Node integration.

The decision belongs in the v0.9.0 — Tooling and Replay milestone (due 2026-10-04), where it lands alongside the `llama.cpp` FFI binding (ADR-014) and the `aegis validate` CLI (ADR-012). A real model loader is also where MCP `tool_use` output naturally surfaces, so the protocol decision and its first concrete consumer ship together.

## Decision

Aegis-Node adopts the Model Context Protocol (MCP) as the canonical agent ↔ tool wire format.

1. **MCP is the protocol.** From v0.9.0 onward, the agent ↔ tool boundary is MCP. F5 reasoning entries' `toolsConsidered` / `toolSelected` JSON-LD terms (already reserved in the v1 `@context`) carry MCP tool names.

2. **Phase 1 ships an MCP _client_ only.** The runtime consumes external MCP servers' tool catalogs and invokes their tools. Aegis-Node does NOT expose its own surface (ledger, identity, policy) as an MCP server in Phase 1. A separate ADR may pursue an MCP-server surface later if the audit story demands it.

3. **MCP slots _above_ the syscall mediator.** `crates/inference-engine/src/mediator.rs` already provides five `mediate_*` methods (filesystem_read/write/delete + network_connect + exec). MCP tool invocations that have filesystem, network, or exec side effects flow through these existing methods, so F2 enforcement and F4 access logging are inherited unchanged. The new entry point is `Session::mediate_mcp_tool_call(server, tool, args)`.

4. **Manifest gains an optional `tools.mcp[]` array.** Per the [Compatibility Charter's "Allowed evolution" rule](../COMPATIBILITY_CHARTER.md) (adding new optional properties), this is a non-breaking schema extension — no `schemaVersion` bump. Each entry has the shape `{server_name, server_uri, allowed_tools: [string]}`. Closed-by-default: without a manifest entry, an MCP tool call is denied and emits a Violation per F2.

5. **Implementation lands in v0.9.0** as four sub-issues under an umbrella tracking issue (filed alongside this ADR):
   - **F2-MCP-A** — schema bump for `tools.mcp[]` (Go + Rust types + drift-test against the canonical schema).
   - **F2-MCP-B** — mediator extension `mediate_mcp_tool_call` that resolves the server, invokes the tool, routes side-effects through existing `mediate_*` methods, and emits one `EntryType::Access` per call with the MCP tool name + reasoning_step_id.
   - **F2-MCP-C** — cross-language conformance: Go `Manifest.Decide` and Rust `Policy::check_mcp_tool` agree on the allowlist battery; rows added to `tests/conformance/cases.json`.
   - **F2-MCP-D** — at least one example MCP-using manifest under `schemas/manifest/v1/examples/`.

## Consequences

**Positive:**
- F5's promise becomes operational. The `toolsConsidered` / `toolSelected` JSON-LD terms gain semantics tied to a real protocol; v0.9.0+ ledgers can be replayed against viewers that understand MCP tool names.
- Native interop with the broader MCP ecosystem — Aegis-Node-protected agents can connect to any MCP-compliant tool server without bespoke integration code.
- Closed-by-default tool allowlist preserves the zero-trust posture: only manifest-listed servers and tools may be invoked.
- Existing F2 enforcement and F4 access logging apply for free: every MCP tool with filesystem/network/exec side effects flows through the existing mediator, so we get policy checks, access entries, and violation emit without new enforcement code.
- Operators don't have to learn an Aegis-Node-specific tool format; if a tool exposes itself via MCP, Aegis-Node can use it.

**Negative:**
- MCP client implementation burden: JSON-RPC framing, transport (stdio / streamable HTTP), capability negotiation, error mapping. Phase 1 keeps this scoped via a small client crate.
- Aegis-Node tracks upstream MCP wire evolution. A future Compatibility Charter entry must pin the MCP version supported in `schemaVersion: "1"` (likely MCP v0.x at adoption); a major MCP wire break could force a v2 manifest schema.
- Tool authors who haven't adopted MCP need to wrap their tools in an MCP server before Aegis-Node can use them. Phase 1's `tools.apis[]` HTTP-verb allowlist remains for HTTP-only callers; MCP is additive, not a replacement.
- MCP server attestation is unspecified in MCP itself. The manifest's `server_uri` carries the trust assertion (e.g., a SPIFFE ID for a co-located server, or a signed-binary path); enforcement of that assertion is closed-by-default but the attestation primitives must be defined per-tool in Phase 2.

## Domain Considerations

MCP is a young protocol but rapidly becoming the de facto standard for AI agent ↔ tool integration. Adopting it now lets Aegis-Node ride the ecosystem instead of competing with it. The risk surface is upstream wire churn; the mitigation is the Compatibility Charter's manifest-schema versioning, which lets us pin an MCP version per `schemaVersion`.

The decision keeps Aegis-Node's enforcement story orthogonal to MCP's transport. MCP defines how tools are discovered and invoked; Aegis-Node decides whether each invocation is allowed and records what happened. The two protocols compose cleanly because MCP is agnostic to the policy layer above it.

## Implementation Plan

1. **Land this ADR** alongside a tracking issue ("MCP: implement Aegis-Node as an MCP client per ADR-018") under the v0.9.0 milestone (issue tracker, not in this PR's diff).
2. **F2-MCP-A** — extend `schemas/manifest/v1/manifest.schema.json` with an optional `tools.mcp[]` definition, mirror in `pkg/manifest/types.go` and `crates/policy/src/manifest.rs`, refresh `pkg/manifest/schema_v1.json` drift-test.
3. **F2-MCP-B** — add `mediate_mcp_tool_call` to `crates/inference-engine/src/mediator.rs`. Resolve the server from manifest, invoke via the new client, route side effects through existing `mediate_*` methods, emit one access entry per tool call.
4. **F2-MCP-C** — extend `tests/conformance/cases.json` with MCP allowlist rows (allowed server + tool, allowed server + disallowed tool, disallowed server). Both engines must agree.
5. **F2-MCP-D** — add `schemas/manifest/v1/examples/agent-with-mcp.manifest.yaml` showing the canonical MCP-using shape.

After all four sub-issues land, the umbrella closes and v0.9.0 has its first concrete tool-protocol deliverable.

## Related PRD Sections

- §4 F2 — Permission Manifest enforcement (this is what gates MCP tool calls).
- §4 F5 — Reasoning Trajectory (this is what records the MCP tool selection).
- §7 Architecture Principles (#2: Zero implicit trust) — the closed-by-default `tools.mcp[]` is the realization of this principle for tool dispatch.

## Domain References

- Anthropic Model Context Protocol specification — https://modelcontextprotocol.io
- ADR-002 (Split-Language Architecture)
- ADR-004 (Permission Manifest)
- ADR-007 (Pre-Execution Reasoning Trajectory)
- ADR-012 (Policy-as-Code Validation) — F10's `aegis validate` will validate the new `tools.mcp[]` shape once F2-MCP-A lands.
- ADR-014 (CPU-First GGUF Inference via llama.cpp) — the model loader that surfaces MCP `tool_use` output.
