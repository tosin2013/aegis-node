# 4. Declarative YAML Permission Manifest in Source Control

**Status:** Accepted
**Date:** 2026-04-27
**Domain:** Security / Authorization (F2)

## Context

Existing AI agent frameworks declare permissions implicitly — through code that calls tools, through environment variables, through ad-hoc allowlists scattered across config files. None of this is reviewable by a security team before runtime, and none of it is enforceable: if the code can call any tool, then a prompt-injected agent can call any tool.

The security review demands a single, authoritative, human-readable artifact that says exactly what the agent is allowed to do, reviewed before deployment, enforced at runtime, and versioned alongside the agent code itself.

## Decision

The Permission Manifest is a YAML file checked into source control next to the agent code. It is the single source of truth for what an agent may do.

Key properties:
1. **Closed by default.** No "allow all" mode. Anything not listed is forbidden.
2. **Scoped permissions.** `fs:read:/data/reports`, `fs:write:none`, `network:outbound:deny`, `apis: [<allowlist>]`, etc.
3. **Versioned.** Every manifest carries a `schemaVersion` and an `agentVersion`; the runtime refuses to load a manifest with an unsupported schema.
4. **Strict enforcement.** Any tool call not covered by the manifest is rejected with a logged violation in the Trajectory Ledger (F9). Violations are first-class events, not silent failures.
5. **Composable inheritance.** Org-level base policies can be extended by team and agent-level policies; the validator (F10) enforces that child policies cannot exceed parent permissions.

## Consequences

**Positive:**
- A single artifact a security reviewer can read in isolation and approve or reject.
- Diffable in git review — permission changes are visible in PR review.
- Composable inheritance lets central security teams enforce floors without micromanaging every agent.
- Strict enforcement makes manifest review a high-leverage control: approving the manifest is approving the agent's blast radius.

**Negative:**
- YAML is verbose for complex policies; teams may copy-paste rather than compose.
- A overly broad manifest (e.g., `fs:read: /`) silently undermines the model — the validator (F10) must catch this.
- Manifest authoring is friction for developers who don't think in security terms; templates are required to keep adoption easy.

## Domain Considerations

The pattern is intentionally analogous to Kubernetes RBAC manifests, OPA Rego policies, and AWS IAM policy documents. Reviewers from those backgrounds will be productive immediately.

## Implementation Plan

1. Define the manifest JSON Schema; treat it as a versioned API.
2. Ship official templates: `read-only-research`, `single-write-target`, `network-egress-allowlist`, `air-gapped`.
3. Implement strict parser with helpful error messages (line/column, suggested fix).
4. Wire enforcement into the Rust runtime at every syscall boundary (file open, network connect, exec).
5. Cross-validate: the Go-side validator (F10) and the Rust-side enforcer must agree on every manifest semantic; ship a conformance suite.

## Related PRD Sections

- §4 F2 — Permission Manifest
- §4 F10 — Policy-as-Code Validation
- §4 F7 — Read-Only Default + Explicit Write Grants

## Domain References

- Kubernetes RBAC
- Open Policy Agent (Rego)
- AWS IAM policy documents
- NIST 800-53 AC-3 (Access Enforcement)
