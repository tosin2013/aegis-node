# 9. Read-Only Default with Explicit, Time-Bounded Write Grants

**Status:** Accepted
**Date:** 2026-04-27
**Domain:** Security / Authorization (F7)

## Context

The mutation question — "can this agent change production state?" — is the second-most-blocking question after exfiltration. Even agents that the security team trusts to *read* sensitive data are routinely refused write privileges, because a hallucinated tool call or prompt-injection attack with write privileges is catastrophic.

Most AI agent frameworks treat read and write symmetrically. The result: either the agent is denied all write privileges (and can't do useful work) or it has open-ended write privileges (and can't pass review).

## Decision

Aegis-Node defaults all resource access to read-only. Write access is granted only via explicit, narrow, optionally time-bounded entries in the Permission Manifest (F2). Properties:

1. **Default = read-only** for every resource type (filesystem, database, API, message broker).
2. **Explicit `write_grants` block** in the manifest. Each entry specifies resource path/scope and the action (`write`, `delete`, `update`, etc.).
3. **No implicit grants.** A write attempt outside `write_grants` is a critical violation logged to F9, even if the resource is in `read` scope.
4. **Time-bounded grants supported.** Grants may include a duration or expiration timestamp; expired grants are inert and trigger violations.
5. **Composable with F3.** Write grants can require Human Approval Gate (F3) per-action via the manifest's `approval_required_for` field.

## Consequences

**Positive:**
- Provides a clean, auditable answer to "can it mutate prod?" — exactly the resources in `write_grants`, no more.
- Enables fine-grained delegation: an agent can read broadly while writing only to one specific output path.
- Time-bounded grants enable "session write" patterns that match incident-response and one-shot batch workflows without long-lived risk.

**Negative:**
- Manifest authors must enumerate write targets carefully; ergonomics can suffer for write-heavy use cases (the validator should help).
- Time-bounded grants require a clock the runtime trusts; clock drift attacks must be considered (use monotonic-clock or signed-time inputs).
- "Write" semantics differ across resource types (an API POST is not the same as a file overwrite); the manifest schema must define each clearly.

## Domain Considerations

The split mirrors UNIX file permissions, AWS IAM read-vs-write distinctions, and database GRANT semantics. The novel contribution is making it the *default* posture of the entire runtime, not an opt-in policy.

## Implementation Plan

1. Define `write_grants` schema in the Permission Manifest (resource scope, action, optional duration, optional approval requirement).
2. Implement enforcement at every mutation syscall the runtime mediates: file write/truncate/rename/unlink, network POST/PUT/PATCH/DELETE for allowlisted hosts, plugin-defined mutation tools.
3. Time-bound enforcement uses monotonic clock + validated wall clock at session start; expired grants emit F9 violations.
4. Compose with F3: when a write_grant entry has `approval_required: true`, the runtime invokes the approval gate before executing.
5. Conformance test: read-allowed write-denied agents cannot perform any mutation across all supported resource types.

## Related PRD Sections

- §4 F7 — Read-Only Default + Explicit Write Grants
- §4 F2 — Permission Manifest
- §4 F3 — Human Approval Gate

## Domain References

- POSIX file permission model
- AWS IAM action-level granularity
- SQL GRANT/REVOKE semantics
- Principle of Least Privilege (NIST 800-53 AC-6)
