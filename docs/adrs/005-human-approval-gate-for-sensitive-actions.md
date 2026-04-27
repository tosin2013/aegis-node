# 5. Human Approval Gate for Sensitive Actions

**Status:** Accepted
**Date:** 2026-04-27
**Domain:** Security / Human-in-the-Loop (F3)

## Context

Even an agent with a tightly scoped manifest can take destructive actions within its scope (e.g., a write grant on a database is a license to corrupt that database). For high-risk action classes — writes, deletes, external API calls, privileged tool use — the security review demands a human-authorized approval per action with a logged approver identity. The agent's own claim that "I would have asked the user" is not auditable; only an approval event signed by an identified human is.

## Decision

Every action class that the operator marks as sensitive in the Permission Manifest (F2) triggers a synchronous Human Approval Gate. Properties:

1. **Structured, human-readable summary.** The agent presents a plain-language explanation of what it intends to do, not raw JSON. Approvers must be able to evaluate without reading the agent's internal state.
2. **Authenticated approval channels only.** CLI prompt with local OS user attribution, a local web UI bound to localhost with a session token, or a signed API call with an authenticated identity. Never an unauthenticated channel.
3. **Configurable timeout with safe default.** If no approval is received within `N` seconds (default: rejected), the action is automatically rejected. No silent escalation.
4. **Immutable approval record.** Approval and rejection events are written to the Trajectory Ledger (F9) with: approver identity, timestamp, the exact action approved, and the reasoning step (F5) that triggered the request.
5. **No batch approval bypass in v1.** Each action request is a discrete approval event. Bulk-approve modes are deferred to ensure auditability is unambiguous in MVP.

## Consequences

**Positive:**
- Provides the audit trail's "who approved this" answer that compliance frameworks require.
- Prevents silent runaway actions from prompt-injected or hallucinating agents.
- The structured summary creates a forcing function for explainable agent reasoning.

**Negative:**
- Synchronous approval introduces latency and operator burden; if applied too broadly, agents become unusable.
- Requires a thoughtful classification of "sensitive" actions in the manifest; manifest authors carry the responsibility.
- Approval UX must be excellent or operators will rubber-stamp everything, defeating the control.

## Domain Considerations

The model echoes change-management approvals in regulated environments (Sarbanes-Oxley change tickets, FDA Part 11 e-signatures, ITIL change advisory boards). Approvers from those backgrounds will recognize the pattern, and the audit artifacts will integrate with their existing GRC tooling.

## Implementation Plan

1. Define the action-summary template format (concise English + structured action metadata).
2. Implement three approval channels: CLI (TTY-attached prompt), local web UI (localhost-only, session-token gated), signed-API (mTLS + identity).
3. Define default timeout (suggest 120 seconds) and refusal-on-timeout semantics.
4. Wire into the runtime: any sensitive action blocks the agent's tool-call loop until approval/rejection/timeout resolves.
5. Ledger schema extension: ApprovalRequested, ApprovalGranted, ApprovalRejected, ApprovalTimedOut events.

## Related PRD Sections

- §4 F3 — Human Approval Gate
- §4 F2 — Permission Manifest (`approval_required_for` field)
- §4 F9 — Hash-Chained Ledger

## Domain References

- FDA 21 CFR Part 11 (electronic signatures)
- ITIL Change Advisory Board patterns
- NIST 800-53 AC-21 (Information Sharing)
