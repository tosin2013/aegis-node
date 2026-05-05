# 31. Community WebUI for Local Collaboration

**Status:** Proposed
**Date:** 2026-05-05
**Domain:** UI / runtime (extends [ADR-005](005-human-approval-gate-for-sensitive-actions.md), [ADR-007](007-pre-execution-reasoning-trajectory.md), [ADR-010](010-deterministic-trajectory-replay-offline-viewer.md), [ADR-016](016-open-core-licensing-model.md), [ADR-025](025-multi-turn-agent-loop-with-triple-bound-circuit-breaker.md))
**Targets:** v0.9.5 Phase 1d (release date 2026-10-20)

## Context

Through v0.9.0 the operator surface is `aegis run --prompt "..."` plus
the [F8 offline replay viewer](010-deterministic-trajectory-replay-offline-viewer.md)
(after-the-fact) and the localhost web channel for [F3 approvals](005-human-approval-gate-for-sensitive-actions.md)
(narrow scope). That's enough to prove the runtime works; it isn't
enough to make the runtime *usable* day-to-day.

Three friction points dominate first-run feedback:

1. **No live transparency.** F5 reasoning entries land in the F9 ledger
   in real time, but watching them requires `tail -f` on a JSONL file.
   Operators want to see *what the agent is doing right now* while it
   does it — not after.
2. **Manifest authoring is YAML.** Operators draft `manifest.yaml`
   in their editor of choice, run `aegis validate`, see linter output,
   edit, repeat. F10 ships in v0.9.0 but is CLI-only. A 50-line
   manifest with three nested `tools.mcp[]` entries is hard to keep
   correct under hand-edit pressure.
3. **F3 approvals are out-of-band.** TTY approvals interrupt the
   terminal; file approvals require a side-channel; the localhost
   web UI for approvals isn't tied to the agent's chat context.
   Operators don't see *why* the agent wants the action (the F5
   reasoning) at the moment of decision.

[ADR-016](016-open-core-licensing-model.md) sets the open-core
licensing model: the community runtime stays Apache-2.0; commercial
tiers add convenience and integration, not previously-blocked
compliance. The Web UI splits along this line — a free, locally-
scoped Community UI proves the runtime is usable; the
[Enterprise UI (ADR-034)](034-enterprise-management-dashboard-and-rbac.md)
adds fleet management and is commercial.

The arrival of bounded multi-turn ([ADR-025](025-multi-turn-agent-loop-with-triple-bound-circuit-breaker.md))
makes this UI work especially load-bearing — agents executing 5–10
turns of read-reason-act with inline approvals are no longer
inspectable from a single CLI invocation.

## Decision

**Ship a Community WebUI in v0.9.5 (Phase 1d) under Apache-2.0. The
UI runs strictly on localhost, manages exactly one local `aegis`
process, and exposes four core surfaces: chat, live trajectory,
inline F3 approvals, and a visual manifest builder. It is a static
SPA served by the existing `aegis` binary on a configurable
localhost port; no separate UI server, no external dependencies, no
network egress.**

### Surfaces

1. **Context-aware chat.** Multi-turn conversation against a single
   agent. Each user message becomes a fresh `Session::run_turn`
   prompt; the chat history is the prompt context. Bounded by the
   manifest's `inference.*` and `--max-turns` ([ADR-025](025-multi-turn-agent-loop-with-triple-bound-circuit-breaker.md))
   exactly as the CLI is.

2. **Live trajectory streaming.** As the agent runs, F5
   reasoning entries and F4 access entries stream into the chat
   alongside the assistant's text. Implementation: WebSocket (or SSE
   fallback) feed of new ledger entries, rendered inline in the
   conversation. Each entry is the same JSON-LD that lands in the
   ledger — UI is a real-time view of the same source-of-truth.

3. **Inline F3 approvals.** When the agent hits an approval gate (per
   [ADR-005](005-human-approval-gate-for-sensitive-actions.md) and
   [ADR-029](029-task-scoped-ephemeral-approval-grants.md)), the
   pending approval surfaces as an interactive card in the chat
   feed: the F5 reasoning context, the proposed tool call args, the
   manifest tier (advisory / validating / blocking / escalating),
   the cumulative quota state from [ADR-027](027-aggregate-quota-schema.md),
   and Approve / Deny / Escalate buttons. Approval decisions write
   the same `approval_decision` ledger entry the CLI writes — the
   UI is a channel, not a different policy.

4. **Visual manifest builder.** Form-driven editor for the F2
   manifest. As the operator edits, the in-process F10
   `aegis validate` ([ADR-012](012-policy-as-code-validation.md))
   engine returns warnings live: overly broad paths, redundant
   `pre_validate` ([ADR-024](024-mcp-args-prevalidation.md)) clauses,
   missing `quota` ([ADR-027](027-aggregate-quota-schema.md)) on
   network outbound, etc. Output is the same YAML that lands on disk;
   "Save" writes the file the CLI would consume. Hand-edit and
   builder coexist — operators can switch back to text at any time.

### Architecture

```text
┌──────────────────────────────────────────────────────────┐
│  Browser (localhost:7777)                                │
│  ┌──────────────────────────────────────────────────┐   │
│  │  Community UI — React/Vite SPA, served as static │   │
│  │  assets bundled into the `aegis` binary.         │   │
│  │  - Chat / trajectory / approvals / manifest      │   │
│  └────────────────┬─────────────────────────────────┘   │
│                   │ WebSocket /api/v1/stream             │
│                   │ HTTP    /api/v1/{session,manifest…}  │
└───────────────────┼──────────────────────────────────────┘
                    │
┌───────────────────▼──────────────────────────────────────┐
│  aegis run --ui --listen 127.0.0.1:7777                  │
│  ┌────────────────┐  ┌─────────────────────┐             │
│  │ existing       │  │ new: ui_server      │             │
│  │ Session +      │◄─┤ axum/tower handler  │             │
│  │ run_turn loop  │  │ /api/v1/* routes    │             │
│  └────────────────┘  └─────────────────────┘             │
└──────────────────────────────────────────────────────────┘
```

- **Localhost-only.** The listener binds 127.0.0.1; the UI server
  refuses non-loopback connections at the socket level. This is
  enforced not just by configuration but by binding semantics — there
  is no flag to expose it externally. Operators who want network-
  reachable UI deploy the [Enterprise UI](034-enterprise-management-dashboard-and-rbac.md)
  (different threat model, different licensing).
- **Single agent, single user.** No multi-tenancy, no auth. The
  threat model is "the operator and the runtime share a host"; the
  UI inherits the host's process boundary as its trust boundary.
- **Static SPA bundled into the binary.** No separate UI server, no
  Node.js runtime requirement at production. The dev workflow uses
  Vite for HMR; the release pipeline builds the SPA and embeds the
  static assets via `include_bytes!` (or `rust-embed`) into
  `crates/cli/`.
- **WebSocket + HTTP REST.** WS for live trajectory + approval
  prompts; HTTP for session lifecycle (`POST /api/v1/sessions`,
  `GET /api/v1/manifests/:id`, etc.). All endpoints serve JSON.

### Implementation phasing

v0.9.5 ships **all four surfaces** at minimum-viable depth. Polish
ships in v0.9.6+/v1.0.0:

| Surface | v0.9.5 must | v1.0.0 polish |
|---|---|---|
| Chat | Single-turn + multi-turn + history scrollback | Markdown rendering of assistant output, syntax highlighting |
| Trajectory | Render reasoning + access entries inline | Hierarchical fold/unfold per [ADR-026](026-hierarchical-per-turn-ledger-protocol.md) turn brackets |
| Approvals | Card with reasoning, args, allow/deny | Quota visualization, escalation routing per [ADR-029](029-task-scoped-ephemeral-approval-grants.md) |
| Manifest builder | Form for tools.{filesystem,network,mcp,exec} + live `aegis validate` warnings | Quota schema editor ([ADR-027](027-aggregate-quota-schema.md)), tier picker ([ADR-029](029-task-scoped-ephemeral-approval-grants.md)) |

Plus two adjacent capabilities ship as their own ADRs:

- [ADR-032 — Visual Model Library and Session Forking](032-webui-model-library-and-session-forking.md)
- [ADR-033 — Visual MCP Server Management](033-webui-visual-mcp-server-management.md)

## Why not the alternatives

- **Browser extension / electron app.** Either path requires shipping
  a separate distribution channel, a separate signing chain, and
  separate security review. The bundled-SPA approach inherits the
  runtime's existing supply chain ([ADR-013](013-oci-artifacts-for-model-distribution.md)
  + [ADR-021](021-huggingface-as-upstream-oci-as-trust-boundary.md)) — one signed binary, one trust boundary.
- **Web UI shipped separately as its own crate / Docker image.**
  Adds operational surface for a feature that's supposed to *reduce*
  friction. The first-run promise is "install one binary, run one
  command" — a separate UI distribution breaks that.
- **Skip the UI; rely on third-party tooling (LangSmith / Helicone /
  custom dashboards).** Those exist for the LangChain ecosystem
  precisely because LangChain doesn't ship one. They also bind the
  user to a third-party SaaS. For Aegis-Node's compliance posture,
  shipping our own localhost UI is the right answer.
- **Defer to Enterprise UI only.** Splits the funnel awkwardly:
  developers evaluating the open-source tier don't see the UX they'd
  get on the commercial tier, so they make adoption decisions
  against the worst-case CLI experience. The Community UI is the
  "free trial" that proves the runtime is usable.

## Implementation tracking

- New crate `crates/ui-server/` (axum routes, WS upgrades, manifest
  builder backend that wraps `crates/policy/` validate functions).
- New static SPA at `ui/` (Vite + React + TypeScript). Build output
  bundled into the CLI binary at compile time.
- Extend `crates/cli/src/run.rs` with `--ui --listen <addr:port>`
  flags. Default listen address `127.0.0.1:7777`.
- Reuse: `crates/ledger-writer/` reader API for the live trajectory
  feed; `crates/approval-gate/src/channels/web.rs` (existing) for the
  in-chat approval channel; `crates/policy/src/validate.rs` for the
  builder's live linting.
- Tracking issue: see v0.9.5 milestone tracker.

## Open questions for follow-up

- **Auth on localhost.** The threat model is "operator owns the
  host," so no auth at all is defensible. But: a malicious local
  process can drive the UI via the loopback API. Should we require
  a per-session token in `Authorization` headers (stored in a
  user-readable file with 0600 perms; same trust as kubectl
  config)? Lean yes — costs nothing, closes a small gap.
- **Cookie storage of session state.** UI may want to persist
  drafts of manifests across browser refreshes. localStorage is
  acceptable; no PII or secrets stored UI-side. Decision: yes,
  manifest drafts only, no session content.
- **i18n.** Out of scope for v0.9.5; English only. Translation
  hooks (`i18next`) are wired in the SPA scaffold so future PRs can
  add locales without rewriting components.
- **Accessibility.** WCAG 2.1 AA target. Keyboard navigation for
  approvals is required (operator may be approving from a
  screen-reader environment). Color is informational — not
  load-bearing.

## References

- [ADR-005](005-human-approval-gate-for-sensitive-actions.md) F3 baseline
- [ADR-007](007-pre-execution-reasoning-trajectory.md) F5 reasoning
- [ADR-010](010-deterministic-trajectory-replay-offline-viewer.md) F8 offline replay
- [ADR-012](012-policy-as-code-validation.md) F10 validate
- [ADR-016](016-open-core-licensing-model.md) Open-core licensing model
- [ADR-025](025-multi-turn-agent-loop-with-triple-bound-circuit-breaker.md) Multi-turn loop
- [ADR-026](026-hierarchical-per-turn-ledger-protocol.md) Per-turn ledger
- [ADR-029](029-task-scoped-ephemeral-approval-grants.md) F3 ephemeral grants
- HashiCorp Vault UI / GitLab CE patterns for open-core front-ends
