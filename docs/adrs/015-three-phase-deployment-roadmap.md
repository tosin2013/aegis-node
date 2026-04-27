# 15. Three-Phase Deployment Roadmap: CLI → Kubernetes → OpenShift

**Status:** Accepted
**Date:** 2026-04-27
**Domain:** Product Strategy / Deployment Architecture

## Context

Aegis-Node must serve very different audiences with very different deployment surfaces:
- A solo developer evaluating the runtime on a laptop, who needs a single-binary install.
- A platform team deploying agents as cluster workloads with namespace isolation, NetworkPolicies, and persistent ledger storage.
- A defense / federal organization deploying in an air-gapped OpenShift environment with SCCs, CAC/PIV authentication, and disconnected GitOps.

Trying to ship all three at once dilutes focus and produces a half-good Phase 1. Delivering them sequentially requires that the architecture not need to be re-done at each phase boundary.

## Decision

Adopt a deliberate three-phase deployment roadmap, with the same Permission Manifest and Trajectory Ledger formats spanning all three phases. Properties:

1. **Phase 1 — Local CLI (MVP).** Single-binary install. CPU-first inference. F1–F10 features fully implemented locally. Target: developers, security engineers, initial PoCs.
2. **Phase 2 — Kubernetes Runtime.** Same manifest semantics, same ledger format, now enforced as cluster workloads. Adds: namespace isolation, ServiceAccount → SPIFFE identity binding, NetworkPolicies, persistent ledger storage, GPU model backends (vLLM/TGI/KServe).
3. **Phase 3 — OpenShift Enterprise.** Same again, deeper enterprise integration. Adds: SCCs, OpenShift AI integration, disconnected install, GitOps deployment, RBAC mapping, automated CMMC/FedRAMP report exports.
4. **No format migration between phases.** A manifest written for Phase 1 runs unchanged in Phase 3 (within the limits of permission scope). A ledger from Phase 1 replays in the Phase 2 viewer and vice versa.
5. **Phase boundaries are real.** Phase 2 work does not start until Phase 1 hits its security-review milestone. Resists the temptation to chase enterprise revenue prematurely.

## Consequences

**Positive:**
- Each phase has a clear, distinct buyer profile and clear technical scope.
- The "same manifest, same ledger" invariant is the strongest possible argument that Phase 1 evaluations translate to Phase 2/3 confidence.
- Sequential delivery focuses engineering effort and produces a credible Phase 1 GA before opening Phase 2 fronts.
- The Architecture Principle #1 ("security review passability first") is testable at every phase boundary: did the previous phase pass review before this one began?

**Negative:**
- Enterprise prospects evaluating Aegis-Node during Phase 1 see "we can't deploy on K8s yet" — risk of losing early enterprise conversations to competitors who promise (but do not deliver) cluster-native day one.
- Maintaining manifest/ledger format stability across phases constrains schema evolution.
- Phase 2/3 backend integrations (vLLM, KServe, OpenShift AI) introduce dependencies that did not exist in Phase 1; testing matrix grows.

## Domain Considerations

The phased roadmap mirrors the trajectory of similar infrastructure products (Tekton: local pipelines → cluster pipelines → OpenShift Pipelines; Argo: workflows → CD → enterprise OpenShift GitOps). Reviewers familiar with cloud-native product evolution recognize and trust the pattern.

## Implementation Plan

1. Lock the manifest and ledger schemas before Phase 1 GA. Treat schema changes thereafter as a major-version event.
2. Build the Phase 1 binary with the Phase 2/3 backend abstraction in place but unimplemented.
3. Define exit criteria for each phase: Phase 1 is "passes a real security review with at least one design-partner organization"; Phase 2 is "operates an agent fleet in a real customer's cluster"; Phase 3 is "produces an automated CMMC report from a real OpenShift deployment."
4. Document the cross-phase invariants in a "Compatibility Charter" so future engineers know what cannot change.

## Related PRD Sections

- §5 The Three-Phase Deployment Roadmap
- §9 Go-to-Market Strategy (defense beachhead)

## Domain References

- Tekton Pipelines / OpenShift Pipelines product evolution
- Argo CD / OpenShift GitOps
- Crossing the Chasm (Geoffrey Moore) — phased market entry
