# 2. Split-Language Architecture: Rust Inference Engine + Go Control Plane

**Status:** Accepted
**Date:** 2026-04-27
**Domain:** Implementation Language / Runtime Architecture

## Context

Aegis-Node must serve two operating environments with very different demands:

- **Phase 1 (local CLI):** Single binary running on a developer laptop with no GPU; must be small, fast-starting, memory-safe, and have no GC pauses during inference.
- **Phases 2–3 (Kubernetes / OpenShift):** Cloud-native control plane with deep K8s ecosystem integration (operator-sdk, client-go, CRDs, admission webhooks, OpenShift SCCs, GitOps tooling).

A single-language choice forces a poor trade-off:
- Rust-only blocks Kubernetes ecosystem velocity (tooling for operators and admission control is overwhelmingly Go).
- Go-only sacrifices memory safety guarantees and GC pause behavior in the inference hot path, and produces larger binaries less suitable for edge.

## Decision

Adopt a deliberate split-language architecture:

1. **Inference Engine — Rust** (binding to `llama.cpp` via FFI). Owns: model loading, tokenization, generation, network-deny enforcement (F6), filesystem sandbox enforcement, identity token verification at the runtime boundary.
2. **Control Plane & Orchestrator — Go.** Owns: CLI (`aegis ...`), API server, policy validator (F10), trajectory ledger writer (F9), Kubernetes Operator, OpenShift SCC integration, manifest parser, replay viewer build pipeline.

The two halves communicate over a stable, versioned IPC contract (gRPC over Unix domain socket on local; gRPC over mTLS in K8s) with the Permission Manifest serving as the shared schema.

## Consequences

**Positive:**
- Memory safety and predictable latency in the inference hot path (Rust).
- Native K8s/OpenShift ecosystem fluency in the control plane (Go).
- Clear blast-radius separation: a bug in the control plane cannot corrupt model execution; a bug in the inference engine cannot bypass policy enforcement (policy is enforced at the IPC boundary).
- Lowers the contributor barrier for the control plane (Go has a much larger pool of contributors familiar with Kubernetes patterns).

**Negative:**
- Two toolchains, two release pipelines, two sets of dependencies to audit.
- IPC boundary must be carefully designed: every Permission Manifest field must round-trip losslessly between Go and Rust, and the validator (Go) must agree with the enforcer (Rust) on semantics.
- Cross-language debugging and tracing is harder than single-language; requires correlation IDs in every log entry.

## Domain Considerations

The split aligns with prior art: container runtimes (containerd Go control plane + runc Rust/C low-level runtime), service mesh (Istio Go control plane + Envoy C++ data plane). Reviewers from cloud-native security backgrounds will recognize the pattern immediately.

## Implementation Plan

1. Define the IPC contract (`aegis.proto`) before either side begins implementation; treat it as a committed API.
2. Build a conformance test suite that exercises both implementations against the same Permission Manifest fixtures.
3. Establish a rule: no policy logic in the Rust side beyond "enforce what the manifest says"; all policy interpretation happens in Go.
4. Cross-publish tagged releases of both binaries from a single monorepo so they version together.

## Related PRD Sections

- §7 Architecture Principles (#4: Split-Language Pragmatism)
- §5 Three-Phase Deployment Roadmap

## Domain References

- containerd / runc split architecture
- Istio + Envoy control-plane / data-plane separation
- Kubernetes Operator pattern (operator-sdk)
