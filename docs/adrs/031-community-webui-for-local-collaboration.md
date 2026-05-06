# 31. Community WebUI for Local Collaboration

**Status:** Proposed
**Date:** 2026-05-05 (amended 2026-05-06 — locked UI stack + wow-factor surface inventory)
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

The four core surfaces below ship in v0.9.5. Each one carries a
"wow-factor" inventory of capabilities that distinguish Aegis-Node's
UI from generic LLM-chat front-ends — these are properties only this
runtime can deliver because they're anchored to the F1–F10 controls.

1. **Context-aware chat.** Multi-turn conversation against a single
   agent. Each user message becomes a fresh `Session::run_turn`
   prompt; the chat history is the prompt context. Bounded by the
   manifest's `inference.*` and `--max-turns` ([ADR-025](025-multi-turn-agent-loop-with-triple-bound-circuit-breaker.md))
   exactly as the CLI is.

   *Wow-factor capabilities:*
   - **Verifiable badge per assistant turn** — small shield icon
     beside each message; click opens the [F9](011-hash-chained-tamper-evident-ledger.md)
     ledger entry with hash-chain visualization. No other open chat
     UI cryptographically anchors messages.
   - **Inline tool-call cards** rendering the gate decision per call:
     manifest allowlist hit, [ADR-024](024-mcp-args-prevalidation.md)
     `pre_validate` outcome, [F7](009-read-only-default-with-explicit-write-grants.md)
     write-grant scope, [F6](008-network-deny-by-default-at-runtime-level.md)
     network policy. Expand to see args + result; the result hash is
     visible.
   - **Live circuit breaker bar** — three thin gauges (turns / tokens
     / wallclock) that drain in real time per [ADR-025](025-multi-turn-agent-loop-with-triple-bound-circuit-breaker.md).
     A gauge hitting its cap pulses red and surfaces the partial-
     ledger preservation message.
   - **Indirect-prompt-injection banner** — when [ADR-028](028-adversarial-pre-filter-gate.md)'s
     pre-filter sanitizes a tool result, a yellow `<aegis-system-warning>`
     banner appears on that tool card. Click to inspect what was
     filtered. This is something no commercial chat UI exposes.
   - **Per-turn collapsible blocks** — multi-turn loops fold into
     "Turn N of M" sections following [ADR-026](026-hierarchical-per-turn-ledger-protocol.md)'s
     hierarchical schema. Hover any turn to highlight its node in
     the trajectory side-panel.
   - **Streaming reasoning trace** — [F5](007-pre-execution-reasoning-trajectory.md)
     `toolsConsidered → toolSelected` renders as a thin "thinking"
     strip *above* the tool call as it streams. Auditable provenance,
     animated.
   - **Citation chips on assistant text** — every claim links back to
     the file path / MCP source it came from, click-to-verify
     against the [F4](006-structured-access-log-jsonld-siem-format.md)
     access entry.
   - **Slash-command palette** — `/pause` (manually trip the circuit
     breaker), `/approve <id>`, `/escalate`, `/replay`,
     `/manifest`. Discoverable via `/`, keyboard-first.
   - **Real stop button.** Pressing it triggers the circuit breaker
     cleanly; the partial F9 ledger is preserved per [ADR-025](025-multi-turn-agent-loop-with-triple-bound-circuit-breaker.md).
   - **Side-by-side trajectory mode.** Toggle splits the view: chat
     left, animated DAG right, scroll-locked. README screenshot.

2. **Live trajectory streaming.** As the agent runs, F5
   reasoning entries and F4 access entries stream into the chat
   alongside the assistant's text. Implementation: WebSocket (or SSE
   fallback) feed of new ledger entries, rendered inline in the
   conversation. Each entry is the same JSON-LD that lands in the
   ledger — UI is a real-time view of the same source-of-truth.

   *Wow-factor capabilities:*
   - **Animated trajectory DAG** — turns light up left-to-right as
     they execute; tool nodes pulse green on success, red on
     [F2](004-declarative-yaml-permission-manifest.md) violation,
     yellow on [ADR-027](027-aggregate-quota-schema.md) quota
     warning. cmd+click any node to open the F9 ledger entry.
   - **Replay scrub** — the same DAG drives the offline replay viewer
     ([ADR-010](010-deterministic-trajectory-replay-offline-viewer.md)),
     so a finished session can be replayed turn-by-turn with a
     timeline scrubber.
   - **Hierarchical fold/unfold** per [ADR-026](026-hierarchical-per-turn-ledger-protocol.md)
     turn brackets — collapse a turn to a single node, expand to
     see its tool_call/tool_result children.
   - **Tampering visualization** — if [F9](011-hash-chained-tamper-evident-ledger.md)
     verify detects a chain break, the affected nodes go grey with
     a broken-link icon and the verify CLI's diagnostic text.
   - **Quota meters** — sidebar shows live `tools.<class>.quota`
     consumption per [ADR-027](027-aggregate-quota-schema.md).
     Approaching a cap triggers a soft warning before the hard stop.

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

   *Wow-factor capabilities:*
   - **Inline cards, not modal dialogs** — slides into the
     conversation thread without breaking flow.
   - **Args diff vs. baseline** — when a tool was previously
     approved with similar args, the card shows a side-by-side
     diff so the operator sees exactly what changed.
   - **Tier-aware UX** — advisory cards are dismissible warnings;
     blocking cards halt the turn until decided; escalating cards
     surface the [ADR-029](029-task-scoped-ephemeral-approval-grants.md)
     async-approver flow with pause/resume.
   - **TTL countdown** — task-scoped approval grants display their
     remaining lifetime; the `sha256(canonical_args)` binding is
     visible on hover.
   - **Keyboard-first** — `a` approve, `d` deny, `e` escalate.
     Cards are screen-reader accessible (WCAG 2.1 AA).

4. **Visual manifest builder.** Form-driven editor for the F2
   manifest. As the operator edits, the in-process F10
   `aegis validate` ([ADR-012](012-policy-as-code-validation.md))
   engine returns warnings live: overly broad paths, redundant
   `pre_validate` ([ADR-024](024-mcp-args-prevalidation.md)) clauses,
   missing `quota` ([ADR-027](027-aggregate-quota-schema.md)) on
   network outbound, etc. Output is the same YAML that lands on disk;
   "Save" writes the file the CLI would consume. Hand-edit and
   builder coexist — operators can switch back to text at any time.

   *Wow-factor capabilities:*
   - **Monaco editor with red/green inline diagnostics** — type a
     wildcard glob too broad and the line lights up red with the
     same wording `aegis validate` would emit.
   - **Schema-aware autocomplete** — manifest field names, allowed
     values for enum-like fields (channels, tier names), valid
     ISO 8601 durations for [F7](009-read-only-default-with-explicit-write-grants.md)
     write_grants.
   - **Form ↔ YAML round-trip** — toggle between the form view and
     raw YAML preserves comments and key order; no lossy
     reformatting.
   - **Dry-run runner** — "Run validate" button executes
     [ADR-012](012-policy-as-code-validation.md) on the current
     buffer, displays the report inline, and links specific
     warnings back to the offending line.

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

### UI stack and bundle pipeline

The SPA is locked to a specific stack so contributors don't relitigate
choices PR-by-PR:

| Concern | Choice | Why |
|---|---|---|
| Framework | **React 19 + Vite 6 + TypeScript** | Largest mindshare in 2026 dev-tool front-ends; Vite gives instant HMR for dev and a small static-bundle output for embed. |
| Styling | **Tailwind CSS + shadcn/ui** (Radix primitives) | shadcn is the de facto modern dev-tool aesthetic (Vercel, Resend, Trigger.dev, OpenStatus). Components are *copied* into the repo, no NPM dep on a UI lib, no surprise upgrades. |
| Chat foundation | **assistant-ui** (MIT, shadcn-based, Vercel AI SDK underneath) | Open-source streaming chat primitive; gives us threads, tool-call rendering, stop/regenerate without writing a chat engine. We extend its primitives with the verifiable-badge / approval-card / circuit-breaker layers. |
| Trajectory DAG | **xyflow/react** (formerly React Flow, MIT) | Best-in-class node graph in React; powers Inngest and Trigger.dev trace UIs. Lets us animate per-turn nodes lighting up as the agent executes. |
| Manifest editor | **Monaco** (the VS Code editor, MIT) | YAML editing with inline diagnostics surfaced from `aegis validate`. Operators get an IDE-grade editor for the manifest. |
| Command palette | **cmdk** (Radix-related, MIT) | Raycast/Linear-style cmd+k surface for slash commands and navigation. |
| Toasts | **Sonner** (MIT) | Minimal, animated, shadcn-aligned. |
| Icons | **lucide-react** (ISC) | Consistent set; matches shadcn out of the box. |
| Time formatting | **date-fns** (MIT) | Tree-shakeable; small bundle. |
| Data fetching | **TanStack Query** (MIT) | Standard for SSE/WebSocket-backed React. |

**Aesthetic baseline:** dark-mode-first, monospace accents on
identifiers (workload IDs, hashes, OCI digests). The visual language
matches the runtime's zero-trust positioning rather than fighting it.
WCAG 2.1 AA contrast is the floor for both light and dark themes.

**Bundle pipeline:**

```text
ui/                              ui-server build invokes:
├─ src/                          1. pnpm install (cached by lockfile)
├─ index.html                    2. pnpm build → ui/dist/
├─ vite.config.ts                3. cargo build then embeds ui/dist
├─ package.json                     via rust-embed into the binary
└─ pnpm-lock.yaml                4. axum tower-http ServeEmbed serves
                                    the embedded files at /
crates/ui-server/
├─ build.rs                      Calls (1)+(2) when ui/dist is missing
│                                or stale (compares ui/src mtimes).
├─ src/
│  ├─ lib.rs                     axum::Router with REST + WS routes
│  ├─ embed.rs                   #[derive(rust_embed::Embed)] handle
│  ├─ handlers/{sessions,
│  │  manifests,models,mcp}.rs   feature-scoped handlers
│  └─ ws.rs                      WebSocket upgrade for /api/v1/stream
└─ Cargo.toml                    rust-embed, axum, tower-http
```

The `build.rs` is best-effort: if `ui/dist/` already exists (e.g., a
contributor built the SPA themselves or a release artifact ships
pre-built), the build script does *not* invoke pnpm. CI builds the
SPA explicitly in a UI-build job and caches `ui/dist/` for the cargo
build, mirroring how the existing Rust workflows cache `target/`.

**Why not a build-step-free approach (HTMX, raw JS):** the multi-turn
trajectory DAG, the live-streaming chat, and the Monaco-driven
manifest editor all need real component state. HTMX + a couple
hundred lines of vanilla JS could do the manifest builder, but the
chat + DAG would be substantially harder to make feel polished. The
build-step cost is paid once at release time; operators install one
signed binary.

**Why not Svelte / Solid:** smaller bundles (~40 KB vs. ~150 KB
gzipped) but the React + shadcn ecosystem is what most front-end
contributors recognize on sight. Bundle size is well within "single
static binary" comfort. If bundle pressure becomes real (Phase 2.5
Enterprise UI ships a much larger app), revisit at that point.

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
  `build.rs` invokes `pnpm build` in `ui/` when `ui/dist/` is absent
  or stale; `rust-embed` bakes the dist output into the binary.
- New static SPA at `ui/` per the locked stack above (React 19 +
  Vite 6 + TypeScript + Tailwind + shadcn/ui + assistant-ui +
  xyflow + Monaco + cmdk + Sonner + lucide + TanStack Query).
- Extend `crates/cli/src/run.rs` with `--ui --listen <addr:port>`
  flags. Default listen address `127.0.0.1:7777`.
- Reuse: `crates/ledger-writer/` reader API for the live trajectory
  feed; `crates/approval-gate/src/channels/web.rs` (existing) for the
  in-chat approval channel; `crates/policy/src/validate.rs` for the
  builder's live linting.
- Phase 1d implementation plan: [docs/plans/v0.9.5-ui-implementation.md](../plans/v0.9.5-ui-implementation.md).
- Tracking issue: [#135](https://github.com/tosin2013/aegis-node/issues/135).

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
