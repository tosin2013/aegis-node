# Research Brief: Multi-Turn Agent Loops with Per-Turn Enforcement

**Status:** Research input — fed into [ADR-025](../adrs/025-multi-turn-agent-loop-with-triple-bound-circuit-breaker.md) through [ADR-030](../adrs/030-per-turn-spiffe-mtls-attestation.md). The open questions listed under each Group below have been translated into ADR decisions; this brief is preserved as the threat-model record and citation source.
**Date:** 2026-05-05
**Owner:** Project maintainers
**Related ADRs:** [025](../adrs/025-multi-turn-agent-loop-with-triple-bound-circuit-breaker.md), [026](../adrs/026-hierarchical-per-turn-ledger-protocol.md), [027](../adrs/027-aggregate-quota-schema.md), [028](../adrs/028-adversarial-pre-filter-gate.md), [029](../adrs/029-task-scoped-ephemeral-approval-grants.md), [030](../adrs/030-per-turn-spiffe-mtls-attestation.md), [018](../adrs/018-adopt-mcp-protocol-for-agent-tool-boundary.md), [019](../adrs/019-explicit-write-grant-takes-precedence.md), [023](../adrs/023-litertlm-as-second-inference-backend.md), [024](../adrs/024-mcp-args-prevalidation.md). [Compatibility Charter](../COMPATIBILITY_CHARTER.md), [Compliance Matrix](../COMPLIANCE_MATRIX.md).

## Background

**Project**: Aegis-Node is an AI agent runtime designed to pass a zero-trust
infrastructure review. Every tool call routes through: identity rebind →
manifest decision → gate dispatch → access entry / violation entry. Logs are
append-only and hash-chained (F9). The project is positioned for U.S. CMMC 2.0
(deadline 2026-11-02), with v1.0.0 as the GA milestone.

**Today's runtime is single-pass.** `Session::run_turn(prompt)` is called
exactly once: the model receives the prompt + tool definitions, emits one
batch of tool calls, the dispatcher fires them through the enforcement gates,
the session ends. There is no second turn where the model sees tool results
and decides what to do next.

**Why this matters**: this surfaced empirically while building the v0.9.0
quickstart examples. A "research assistant" example asked Gemma 4 to read two
files via MCP, then write a one-paragraph summary. Gemma 4 emitted the two
reads and stopped — it can't author the summary without seeing what was
read. The same example with Qwen 1.5B "worked" by emitting all three calls
speculatively, where the write content was a hardcoded guess generated
alongside the reads — producing output the project's owner correctly
identified as "generic / templated." The single-pass model forces examples
to be either trivial (one tool call) or fake (model speculates the answer).

For an "agent runtime" the gap is structural: an agent that can't do
*read → reason → act* is not an agent. The redesign must add bounded
multi-turn while preserving every property security review depends on.

**Definition for this brief**: a *turn* is one (model_response →
tool_dispatch → tool_results) cycle. A multi-turn session runs N turns where
the model sees the prior turn's tool results in the prompt for turn N+1,
capped at `--max-turns`. Every turn passes the same enforcement pipeline as
today's single turn.

## Existing project primitives the design must respect

- **F2 Permission Manifest** (closed-by-default, `tools.mcp[].allowed_tools`
  allowlist, ADR-024 `pre_validate` per tool)
- **F3 Approval Gate** (TTY / file / web / mTLS channels)
- **F5 Reasoning Trajectory** — today emits exactly one `reasoning_step`
  ledger entry per session, with `toolSelected` + `toolsConsidered`
- **F6 Network Policy** + end-of-session signed network attestation (v0.8.0)
- **F7 Write-Grants** — time-bounded (ISO 8601 duration), explicit-takes-
  precedence (ADR-019)
- **F9 Hash-Chained Ledger** — append-only, every entry has `prevHash` +
  `sequenceNumber`
- **OWASP Top 10 for Agentic Applications 2026** mapping (T1 prompt injection,
  T2 supply chain, T6 cascading failures, T7 memory poisoning, T9 trust
  exploitation, T10 over-privilege)
- **`inference.determinism`** — `seed`, `temperature`, `top_p`, `top_k`,
  `repeat_penalty` pinned per session

The output of this research must be compatible with all of the above without
breaking the [Compatibility Charter](../COMPATIBILITY_CHARTER.md).

## Research Questions

### Group A — Loop bounds & runaway prevention

1. **Empirical loop length.** In production agent deployments (LangGraph,
   AutoGPT, Anthropic Claude tool use, OpenAI Assistants, Bedrock Agents),
   what is the distribution of useful turn counts for distinct task classes
   (research, coding, customer support, data analysis)? Is there a P50/P95
   worth defaulting `--max-turns` to?
2. **Cap-hit semantics.** When N turns is reached without the model emitting
   "done," what should the runtime do — return the in-progress trajectory,
   return an explicit `turn_cap_exceeded` error, or fall through to a
   "summarize what you've done so far" final turn? What do production
   frameworks do, and what failure modes do they surface in postmortems?
3. **Cost-bounded loops vs count-bounded loops.** Should the cap be turns
   (`--max-turns`), inference tokens (`--max-tokens`), wallclock
   (`--max-seconds`), or a triple? Which combination has produced production
   incidents in published agent postmortems (LangChain, OpenAI Assistants
   v1 → v2, etc.)?

### Group B — Indirect prompt injection across turns (OWASP T1)

4. **Tool-result-as-prompt risk.** When a tool's output (file contents, MCP
   server response, web search result) is concatenated into the prompt for
   turn N+1, the tool result becomes attacker-controllable input that
   influences future model behavior — the canonical "indirect prompt
   injection" vector (Greshake et al. 2023). What mitigations have empirical
   track records: structured tool-result wrapping, explicit "this is data,
   not instructions" delimiters, separate trust scopes per turn, or canary
   tokens?
5. **Compromised-MCP-server scenario.** If a downstream MCP server is
   compromised and returns malicious instructions in its `text` field,
   what's the threat model? Does the existing F2-MCP allowlist + ADR-024
   `pre_validate` handle it, or does multi-turn open new vectors (e.g., the
   malicious server tells the model to call a different allowed tool with
   attacker-chosen args)? What's the "tool-output-as-prompt-injection"
   defensive pattern used by Anthropic Claude tool use, and is it
   reproducible in an open-runtime setting?
6. **Cross-turn context poisoning.** If turn 1's tool result poisons the
   model's plan and turn 5 acts on the poisoned plan, can the ledger detect
   this retrospectively, and if so via what signal (entropy of plan tokens,
   tool-call-graph divergence, divergence from a baseline session of the same
   prompt)?

### Group C — Cumulative privilege escalation (OWASP T10)

7. **Aggregate effect across turns.** Each turn passes manifest enforcement
   individually; the cumulative effect may not. Example: 100 individual
   `read_text_file` calls each within `tools.filesystem.read`, but in
   aggregate that's a directory exfiltration. What rate-limit / aggregate-cap
   primitives exist in production agent runtimes (LangSmith, Bedrock
   Guardrails, etc.), and how are they expressed in policy?
8. **Per-tool-class budgets.** Should the manifest grow a per-tool-class
   quota (`tools.filesystem.read.max_calls_per_session`,
   `tools.mcp[].max_calls`, `network.max_bytes`)? What are the trade-offs in
   expressivity vs. operator-overload? Which existing zero-trust frameworks
   (SPIFFE/SPIRE, OPA Rego, Cedar) handle this elegantly?
9. **Detection vs. prevention.** Should aggregate-cap violations halt the
   session (prevention) or emit a violation entry while continuing
   (detection)? Which mode aligns with CMMC 2.0 + NIST 800-171 + NIST AI RMF
   guidance?

### Group D — Per-turn ledger model (F9 evolution)

10. **Ledger granularity.** Today: one `reasoning_step` per session. Options
    for multi-turn: (a) one `reasoning_step` per turn, (b) one `turn_start`
    + per-tool-call entries + `turn_end`, (c) nest `tool_call` entries under
    a `turn` parent. What ledger shapes have replay tooling that's been
    auditor-validated (NIST 800-171 §3.3 audit & accountability)?
11. **Replay determinism.** Single-pass replay needs (manifest, model digest,
    prompt, seed). Multi-turn replay also needs (each tool result, ordering
    across concurrent tool calls, any approval decisions). What's the
    minimum sufficient set of ledger fields, and how does that intersect
    with the Trajectory Replay viewer (F8) planned for v0.9.0?
12. **Hash-chain extensibility.** Adding new entry types (`turn_start`,
    `turn_end`) is backwards-compatible per Compatibility Charter §"ledger."
    What's the migration path for an existing v1 ledger consumer that
    doesn't know `turn_start`? Should the schema bump to `v2` or extend
    `v1`?

### Group E — Approval gate scope across turns (F3)

13. **Per-turn vs per-session approvals.** If an action requires approval in
    turn 2, does that approval implicitly cover the same action in turn 5
    (same args), or must the user re-approve? What are the human-factors
    trade-offs (approval fatigue vs over-grant)?
14. **Approval expiry within a session.** F7 write-grants are time-bounded;
    should approval grants be? What's the prior art in privileged-access-
    management (CyberArk, Hashicorp Vault leases, AWS STS session
    credentials)?
15. **Approval channel assumptions.** The mTLS channel is designed for
    headless / signed-API approvals. In a multi-turn loop, does the approver
    need to be human-in-the-loop on every approval-required turn, or can
    they pre-authorize a narrow scope? What's the "policy-driven auto-
    approval" pattern's failure modes?

### Group F — Trust-boundary and supply-chain implications (OWASP T2)

16. **Tool-result tampering between turns.** The single-pass model has no
    inter-turn gap; multi-turn does. Can an attacker with code execution on
    the runtime modify tool results between dispatch and the model's next
    turn? What's the integrity-protection model for the tool-result
    transport (in-process today, but ADR-022 trust-boundary format-
    agnostic suggests it could become out-of-process)?
17. **Per-turn identity rebind.** F1 rebinds identity on every tool call;
    should multi-turn rebind on every turn boundary too, even when no tool
    is called (e.g., model emits text-only turn)? What does SPIFFE workload
    attestation say about long-running sessions and credential rotation?

### Group G — Industry / standards alignment

18. **Reference architectures.** Which reference architectures explicitly
    address multi-turn agent enforcement: NIST AI RMF profile, MITRE ATLAS,
    ENISA AI Threat Landscape, EU AI Act high-risk system requirements,
    Anthropic's Responsible Scaling Policy, OpenAI's Preparedness Framework?
    What concrete control families do they prescribe?
19. **Compliance mapping.** For CMMC 2.0 Level 2 + FedRAMP Moderate + ISO
    27001:2022, which controls map to multi-turn agent runtime properties,
    and where is current Aegis-Node coverage versus where multi-turn
    introduces a gap?
20. **Comparable runtimes.** What enforcement primitives do comparable
    runtimes (LangChain LangGraph, Microsoft Semantic Kernel, AWS Bedrock
    Agents, Google Vertex Agent Builder, Anthropic Claude Tools) ship for:
    max-turns, per-turn approval, aggregate-cap, indirect-prompt-injection
    defense, replay, hash-chained audit? Where do they each have a security-
    review-blocking gap?

## Output requested

For each group, the deliverable is a 1-2 page review with:
- Concise summary of prior art (with citations)
- Recommendation for the Aegis-Node design with explicit trade-offs
- Open questions that should become a follow-up ADR or RFC

The end product feeds at least three ADRs:
- **ADR-025**: Multi-turn agent loop architecture + `--max-turns` semantics
- **ADR-026**: Per-turn ledger entries + replay determinism
- **ADR-027**: Cumulative-effect aggregate caps in the manifest schema

Possibly a fourth ADR on cross-turn approval scope (F3 evolution).

## Out of scope for this research

- Implementing the multi-turn loop (engineering, follows the ADRs)
- Re-recording demos (separate)
- Updating examples 02/04/06 to use multi-turn (follow-up PR)
- Streaming model output (single-shot per turn is fine for v1.0.0)
