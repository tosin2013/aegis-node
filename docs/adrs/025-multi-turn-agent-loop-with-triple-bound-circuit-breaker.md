# 25. Multi-Turn Agent Loop with Triple-Bound Circuit Breaker

**Status:** Proposed
**Date:** 2026-05-05
**Domain:** Runtime / inference-engine (extends [ADR-007](007-pre-execution-reasoning-trajectory.md), supersedes single-pass `Session::run_turn`)
**Related research:** [docs/research/multi-turn-agent-loop.md](../research/multi-turn-agent-loop.md)

## Context

Today's `Session::run_turn(prompt)` is single-pass: the model receives
the prompt + tool definitions, emits one batch of tool calls, the
dispatcher fires them through the enforcement gates, and the session
ends. There is no second turn where the model sees tool results and
decides what to do next.

This was discovered empirically while building Example 02
(`mcp-research-assistant`) for v0.9.0. The example asked Gemma 4 to
read two fixture files via MCP and write a one-paragraph summary.
Gemma 4 emitted both reads and stopped — it cannot author the summary
without seeing what was read. Qwen 1.5B in the earlier prototype
"worked" only by emitting all three calls speculatively, where the
write content was a hardcoded guess produced alongside the reads. That
output was correctly identified as templated and unacceptable.

The gap is structural: an agent runtime that cannot do *read → reason →
act* is not really an agent runtime. The fix is bounded multi-turn —
the model gets up to N turns where it sees the prior turn's tool
results in the prompt for turn N+1.

But multi-turn is also where AI agent runtimes get into the most
trouble. Two postmortems shaped the bounds we ship:

- **47K-USD clarification loop.** Four LangChain-style agents
  communicating via an A2A protocol entered an infinite clarification
  loop that ran 11 days uninterrupted, accumulating ~$47K in LLM API
  costs before the billing anomaly was detected.
- **Invalid-JSON parser loop.** An API format change caused an agent
  to endlessly retry parsing invalid JSON, burning ~40K tokens/min
  before monitoring caught it.

Empirical data from production deployments (a 26-domain study) shows
68% of successful production agents complete in ≤10 steps before
human intervention, termination, or failure. Beyond that, cumulative
probability decay dominates: a 10-step task with 85%-per-step accuracy
completes successfully only ~19% of the time.

A turn cap alone is not enough. Three failure modes need three different
bounds:

| Failure mode | Mitigation |
|---|---|
| Infinite semantic loop (model keeps emitting tool calls) | turn count |
| Massive context payload eats budget without many turns | token cost |
| Hung tool call / network stall (no API calls but compute burns) | wallclock |

## Decision

**Ship a multi-turn agent loop in `Session::run_turn` capped by a
Triple-Bound Circuit Breaker: turn count, token budget, and wallclock.
Cap-hit halts deterministically and returns the partial F9 ledger
trajectory — never a silent truncation, never a graceful summarization
turn that consumes more tokens.**

Concretely:

1. **CLI surface (`crates/cli/src/run.rs`):**

   ```text
   aegis run \
       --manifest ... --model ... --prompt ... \
       --max-turns N        # default 10. Hard cap on model invocations.
       --max-tokens M       # default per-backend (Gemma 4: 32768; Qwen: 8192).
       --max-seconds S      # default 300. Wallclock from session_start.
   ```

   All three are independent stop conditions. Whichever fires first
   halts the loop.

2. **Loop shape (`crates/inference-engine/src/turn.rs`):**

   ```text
   session_start → ledger entry
   for turn in 1..=max_turns:
       turn_start → ledger entry (sequence, prev_hash, model_digest)
       run model with current context
       parse tool_call list
       if tool_call list is empty AND assistant_text contains a "done" signal:
           emit final assistant_text → ledger
           break (clean termination)
       for each tool_call:
           dispatch through existing enforcement (rebind → policy → gate)
           append (tool_call, tool_result) → ledger
           append (tool_call, tool_result) → context window for turn N+1
       turn_end → ledger entry (cumulative tokens, cumulative wallclock)
       if any of [turns >= max_turns, tokens >= max_tokens, wallclock >= max_seconds]:
           emit TurnCapExceeded violation entry → ledger
           break (capped termination)
   session_end → ledger entry
   ```

   The "done" signal is a model-controlled clean termination — when the
   model emits no tool calls and the assistant text answers the prompt,
   the session ends naturally. This is the common case and does not
   trigger the cap path.

3. **`TurnCapExceeded` error semantics.** Capped termination is not an
   exception. The CLI exits with `aegis run` returning a structured
   error object — non-zero exit code, stderr describing which bound
   tripped, stdout containing the partial F9 ledger path. Callers
   parse the partial trajectory; nothing is lost.

   ```text
   Error: TurnCapExceeded
   - bound: turns (10/10)
   - turns_executed: 10
   - tokens_consumed: 14523/32768
   - wallclock_seconds: 87.3/300
   - ledger_path: ledger-session-XXXX.jsonl
   - last_assistant_text: "I need to read another file before I can..."
   ```

4. **Manifest interaction (no changes required).** All existing F2
   enforcement (`tools.filesystem.*`, `tools.mcp[]`, `tools.network`,
   `inference.determinism`) applies per-tool-call inside the loop
   exactly as it does today. The Triple-Bound is a session-level
   circuit breaker on top of per-call enforcement, not a replacement.

5. **Replay determinism (interaction with [ADR-026](026-hierarchical-per-turn-ledger-protocol.md)).**
   The wallclock bound is non-deterministic across runs. To preserve
   F8 trajectory replay determinism, the wallclock bound is *recorded
   but not replayed*: replay uses turn count + token budget only, and
   the F9 ledger captures the original wallclock value for forensics.
   See ADR-026 §"Replay determinism."

## Why not the alternatives

- **Soft summarization at cap.** Some frameworks (notably ReAct-style
  variants) intercept the boundary turn, strip tool privileges, and
  inject "summarize what you've done." We reject this because: (a) it
  consumes more tokens and pushes the bound, (b) the summarization
  turn frequently inherits hallucinations from the corrupted context
  that triggered the cap, and (c) it makes audit harder — the "final"
  ledger entry is now a mid-flight rationalization rather than a
  faithful record of where the agent actually stopped.
- **Single bound only.** Picking just turns ignores the 47K-USD-loop
  postmortem (40K tokens/min on a small turn count). Picking just
  tokens ignores the wallclock postmortem (hung MCP server, no token
  flow at all). All three must coexist.
- **Per-turn cost ledger but no hard cap.** Optional in v1.x, not
  v1.0.0. The defense-first posture requires bounded execution by
  default.
- **Default `max_turns=5` (OpenAI Agents SDK style).** Too low for
  realistic research-assistant and coding workloads (Group A of the
  research brief shows P50 around 7–10 turns for synthesis tasks).
  10 is the lowest round number that covers the empirical P50 with
  margin without enabling the runaway profile.

## Defaults rationale

- `--max-turns 10` — matches the 68%-of-successful-production-agents
  threshold from the 26-domain study; below the P95 where cumulative
  probability decay dominates.
- `--max-seconds 300` — five minutes covers tool-heavy turns
  (MCP filesystem reads, MCP web search) on commodity CPU; beneath
  the typical user "abandoned the request" threshold.
- `--max-tokens` per-backend — Gemma 4 E4B has a 32K context window;
  Qwen 1.5B Q4_K_M has 8K. The default is the model's full context
  minus a small reserve for the prompt + final response.

## Implementation tracking

- Crate changes: `crates/inference-engine/src/turn.rs` (loop
  redesign), `crates/inference-engine/src/session.rs` (per-turn ledger
  emission, see [ADR-026](026-hierarchical-per-turn-ledger-protocol.md)),
  `crates/cli/src/run.rs` (new CLI flags + error formatting),
  `crates/cli/src/error.rs` (new `TurnCapExceeded` variant).
- Docs: update `docs/COMPATIBILITY_CHARTER.md` to declare the loop
  contract (caller-observable behavior of clean vs. capped
  termination).
- Tracking issue: see v1.0.0 milestone tracker.

## Open questions for follow-up

- **Local vs. backend tokenization.** Token counts are the load-bearing
  bound for cost protection. Should the runtime estimate token usage
  from a deterministic local tokenizer (matching the bound at the
  start of each turn before invoking the backend) or trust the
  backend's async usage report (which lags by one turn)? Local is
  more deterministic but tokenizers drift. Lean toward local with a
  per-backend tokenizer module behind the existing `Backend` trait.
- **Wallclock + replay tension.** ADR-026 needs to ensure replay can
  reproduce the cap path even though wallclock is non-deterministic.
  Working assumption: replay uses turn-count + token-budget only;
  wallclock is "honored on live runs, recorded for audit, ignored on
  replay." The viewer renders the recorded wallclock for context.
- **`--max-turns 0` semantics.** Should `--max-turns 0` mean "run as
  single-pass like v0.9.0 and earlier" (preserves the legacy code
  path for diff debugging) or be rejected as invalid? Lean toward
  reject; legacy single-pass is documented in the v0.9.0 release.

## References

- Research brief: [docs/research/multi-turn-agent-loop.md](../research/multi-turn-agent-loop.md) §"Group A — Loop bounds & runaway prevention"
- 26-domain production agent study (cited in research brief)
- 47K-USD A2A clarification loop postmortem (cited in research brief)
- 40K-token-per-minute JSON parser loop postmortem (cited in research brief)
- OpenAI Agents SDK `max_turns` default (5)
- Anthropic Claude SDK `max_turns` + `max_budget_usd`
- LangGraph `RecursionError` semantics
