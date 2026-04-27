# 3. Cryptographic Workload Identity for Agents (SPIFFE/SPIRE Compatible)

**Status:** Accepted
**Date:** 2026-04-27
**Domain:** Security / Identity (F1)

## Context

The first question in any zero-trust review is "what identity is this thing running as?" Existing AI agent runtimes typically have no answer: the agent is an unsigned binary with implicit access to whatever the host user can reach. There is no cryptographic binding between the agent's actions and an identity that the audit trail can attribute them to.

Enterprise infrastructure already has a standard for this problem at the workload level: SPIFFE (Secure Production Identity Framework For Everyone) and its reference implementation SPIRE. Kubernetes Service Accounts and AWS IAM roles solve the same problem with proprietary mechanics. A new identity system would be rejected by enterprise security on first sight.

## Decision

Every Aegis-Node agent instance receives a cryptographically signed workload identity at instantiation. The identity:

1. Is bound to a triple `(model digest, manifest digest, configuration digest)` — changing any of them invalidates the identity and halts execution.
2. Conforms to SPIFFE workload identity standards: SPIFFE ID format, X.509-SVID or JWT-SVID issuance, compatible with SPIRE-issued identities for enterprise deployments.
3. Signs every action recorded in the Trajectory Ledger (F9), creating an unambiguous chain of attribution.
4. Is verifiable offline: the runtime ships with a built-in CA mode for local CLI use; in Kubernetes deployments, identities are issued by a SPIRE server in the cluster.

## Consequences

**Positive:**
- Recognized by enterprise security teams without explanation.
- Native fit with existing Kubernetes/OpenShift identity infrastructure (Phase 2/3).
- Cryptographic binding to the manifest means tampered configurations cannot reuse a valid identity.
- Single mental model from local laptop to production cluster.

**Negative:**
- Local CLI use requires bootstrapping a built-in CA, adding install-time complexity.
- SPIFFE adds a vocabulary developers must learn (SVIDs, trust domains, registration entries).
- Identity rotation policy must be defined; long-lived SVIDs weaken the model.

## Domain Considerations

SPIFFE/SPIRE is the de facto identity standard for cloud-native zero-trust. Building on it inherits a substantial amount of ecosystem trust and enterprise interoperability (Istio, Vault, AWS App Mesh, etc.).

## Implementation Plan

1. Define the SPIFFE ID format: `spiffe://<trust-domain>/agent/<workload-name>/<instance>`.
2. Implement a built-in lightweight CA for the local CLI (file-backed, single-tenant).
3. In Phase 2, replace local CA with SPIRE workload attestation against a cluster SPIRE server.
4. Add `aegis identity verify` CLI command that validates an identity token against the manifest digest.
5. Halt execution when the agent's identity, model digest, or manifest digest no longer match.

## Related PRD Sections

- §4 F1 — Agent Identity + Workload Identity
- §7 Architecture Principles (#2: Zero implicit trust)

## Domain References

- SPIFFE Specification (https://spiffe.io/)
- SPIRE workload attestation
- NIST 800-204A Service Mesh Identity
