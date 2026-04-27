# 12. Policy-as-Code Validation in CI/CD via `aegis validate`

**Status:** Accepted
**Date:** 2026-04-27
**Domain:** Security / Pre-Deployment Governance (F10)

## Context

A security control reviewed only at deployment time is reviewed too late. By then, the manifest has already been written, the agent has already been built, and pushing back means delay and developer frustration. The security team's leverage is highest at code-review time, when the manifest is still a diff in a pull request and a small set of reviewers can block merge.

Existing AI agent frameworks have no equivalent. Their permission expressions live in code; CI cannot validate them without executing the code, and reviewers cannot evaluate them without reading the agent's full source.

## Decision

Manifests and Behavioral Contracts are validated by a dedicated CLI (`aegis validate`) designed for CI/CD integration. Properties:

1. **Schema validation.** The manifest is checked against a versioned JSON Schema; all type, format, and required-field errors are reported with file/line context.
2. **Policy linting.** A set of rules detects common misconfigurations: overly broad file paths (`/`, `/home`), wildcard tools, missing approval gates on write actions, network grants without justification, etc.
3. **Composition + inheritance.** Org-level base policies can be extended by team-level and agent-level policies. The validator enforces that child policies cannot exceed parent permissions; violations fail the build.
4. **Structured JSON output.** Validation output is structured for CI consumption (GitHub Actions annotations, GitLab CI reports, Jenkins JUnit XML).
5. **Human-readable security summary.** The validator emits a "policy summary" report — a plain-language account of what the agent is permitted to do — suitable for inclusion in a security review package without modification.

## Consequences

**Positive:**
- Moves the security review left into CI and PR review, where the cost of changes is lowest.
- Enables a self-service developer experience: developers see violations at PR time, not at deployment.
- Org-level base policies become a real control: central security can enforce floors without inspecting every agent.
- Policy summaries become the artifact security reviewers ask for, eliminating manual translation steps.

**Negative:**
- Validator linting rules must be conservative (false positives erode trust, false negatives let unsafe manifests through). Tuning is an ongoing concern.
- Inheritance complicates mental model; the validator must explain *why* a child policy is rejected (which parent rule it exceeds).
- The validator must agree exactly with the runtime enforcer (Rust side) on every semantic; conformance test suite is required.

## Domain Considerations

The pattern is established: kube-score, conftest, OPA Rego in CI, Terraform compliance, Snyk policy gates. Reviewers recognize the model immediately and have existing tooling to integrate it with.

## Implementation Plan

1. Define the manifest JSON Schema (versioned, extensible).
2. Implement the linter rule set (start with ~10 high-value rules; grow conservatively).
3. Implement composition: base + team + agent layered manifests, with explicit `extends:` links and parent-permission enforcement.
4. Output formats: GitHub Actions annotations, JUnit XML, plain text, JSON.
5. Generate the "policy summary" plain-language report from the resolved manifest.
6. Conformance test pair: any manifest accepted by the validator must be enforced consistently by the Rust runtime, and vice versa.

## Related PRD Sections

- §4 F10 — Policy-as-Code Validation
- §4 F2 — Permission Manifest

## Domain References

- Open Policy Agent / Conftest
- kube-score
- Terraform Sentinel / Compliance
- NIST 800-53 CM-3 (Configuration Change Control)
