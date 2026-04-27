# 7. Reasoning + Action Trajectory Recorded Before Execution

**Status:** Accepted
**Date:** 2026-04-27
**Domain:** Auditability / Explainability (F5)

## Context

"What did the agent do?" is answered by the access log (F4). "Why did it do it?" requires a record of the agent's reasoning chain — which inputs triggered which thoughts, which tools were considered, which tool was selected, and which option was rejected. Most LLM agent frameworks treat reasoning as ephemeral: the chain-of-thought is generated, used to pick a tool, and then thrown away. This makes post-hoc explanation impossible and post-incident root-cause analysis a guessing game.

Worse, recording reasoning *after* an action executes means a crash mid-execution leaves no trace of why the agent attempted the action. An auditor cannot tell whether the action was unauthorized intent or an unexpected fault.

## Decision

Every action the agent takes is preceded by a Trajectory Ledger entry recording the reasoning that led to it. Properties:

1. **Pre-execution write.** The trajectory entry is committed to the ledger (F9) *before* the action executes. A crash during action execution still leaves a complete record of intent.
2. **Structured content.** Each entry records: triggering input, the reasoning chain (in structured natural language), tools considered, tool selected, and the action to execute.
3. **Linked to access entries.** Each trajectory entry has a reasoning-step ID; access log entries (F4) reference it via the same ID.
4. **Replay-capable.** The format is machine-parseable enough for the F8 replay viewer to reconstruct a synchronized timeline of reasoning + action without external services.
5. **No reasoning recorded after action completion.** Post-execution outcomes are recorded in separate result entries, not appended to the reasoning entry — the reasoning record is immutable evidence of the agent's pre-action state.

## Consequences

**Positive:**
- Answers "why did it act?" with verifiable, pre-action evidence.
- Crash-resilient: even an interrupted action leaves a complete intent record.
- Foundation for replay (F8) and incident root-cause analysis.

**Negative:**
- Every action incurs a synchronous ledger write before it can proceed; performance-sensitive use cases need batching strategies that don't break the pre-execution invariant.
- LLM reasoning is non-deterministic; structured-language output requires either fine-tuning or a deterministic post-processing layer.
- Storage volume for long-running agents is significant; retention/archival policy must be defined.

## Domain Considerations

The pattern echoes Write-Ahead Logging (WAL) in databases: write the intent before applying the change, so recovery is possible from any failure point. Auditors familiar with WAL semantics will recognize the durability guarantee.

## Implementation Plan

1. Define the trajectory entry JSON-LD schema (input, reasoning steps, tools considered, tool selected, action).
2. Implement a Reasoning Capturer that intercepts LLM tool-selection output and converts it to the structured form.
3. Wire the runtime so the ledger write completes before the action handler is invoked; failures to write the trajectory entry block the action.
4. Add reasoning-step IDs that link to F4 access entries.
5. Build a conformance fixture: golden trajectory for a deterministic test agent, used in regression tests.

## Related PRD Sections

- §4 F5 — Reason + Action Trajectory
- §4 F4 — Access Log
- §4 F8 — Trajectory Replay

## Domain References

- Write-Ahead Logging (PostgreSQL, SQLite)
- ISO 27001 A.12.4.1 (Event Logging)
- IEEE 829 audit trail patterns
