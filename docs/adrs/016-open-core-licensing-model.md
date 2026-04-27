# 16. Open-Core Licensing: Apache 2.0 Community + Commercial Tiers

**Status:** Accepted
**Date:** 2026-04-27
**Domain:** Business Model / Licensing

## Context

A security-focused infrastructure product faces a tension. Permissive open-source licensing maximizes auditability (security teams can read every line) and adoption (developers prefer Apache/MIT to copyleft or proprietary), both of which are decisive for trust in a security product. But Aegis-Node also needs durable enterprise revenue to fund the sovereign-grade features (TEE attestation, automated compliance reporting, CAC/PIV) that only matter to a small set of high-value buyers.

A pure proprietary license blocks community adoption and makes "we have nothing to hide" claims harder to verify. A pure open-source license starves enterprise feature development. Open-core (permissive open-source for the runtime, commercial license for enterprise add-ons) is the established middle path.

## Decision

Adopt an open-core licensing model with three tiers:

1. **Community (Apache 2.0).** F1–F10 core features, CLI, local replay viewer, manifest validator, OCI-based model distribution. The runtime that must be auditable is the runtime that is open source.
2. **Enterprise (commercial).** Community features + Management UI, SIEM integration packs, RBAC integrations, support SLA. Targeted at mid-market enterprises and hospital systems.
3. **Sovereign (commercial).** Enterprise features + TEE attestation (SGX/SEV), automated CMMC/FedRAMP reporting, CAC/PIV authentication. Targeted at defense contractors and government agencies.

Properties:
- The community runtime must be sufficient to pass a security review on its own. Enterprise features add convenience and integration; they do not unlock previously-blocked compliance.
- All ledger and manifest formats are open. Enterprise tiers consume them; they never extend them in proprietary directions.
- Apache 2.0 is chosen over copyleft for compatibility with enterprise legal review and for the explicit patent grant.

## Consequences

**Positive:**
- Auditability of the security-critical runtime; security reviewers can read everything that touches policy enforcement.
- Apache 2.0 minimizes legal friction for enterprise adoption.
- Community/Enterprise/Sovereign tiers map cleanly to distinct buyer profiles and budgets.
- The defense beachhead (CMMC 2.0) can buy Sovereign without a complex licensing negotiation.

**Negative:**
- Drawing the line between "open" and "commercial" features is a recurring architectural decision; getting it wrong erodes either community trust or commercial revenue.
- Open-core invites forks; we must remain the most credible source for the runtime by being responsive to community contributions.
- Apache 2.0 does not protect against a hyperscaler offering Aegis-Node as a managed service. Acceptable risk for now; revisit if it materializes.

## Domain Considerations

The split mirrors successful open-core security infrastructure (HashiCorp Vault → Vault Enterprise; GitLab CE → EE; Elastic prior to license change). The Sovereign tier is a deliberate echo of Red Hat's defense / federal product packaging.

## Implementation Plan

1. Adopt Apache 2.0 for the community repo from day one. No "license change later" plan.
2. Keep enterprise/sovereign code in separate repos with clearly distinct licensing.
3. Establish a Contributor License Agreement (CLA) appropriate for the future commercial tier; pick early to avoid relicensing pain later.
4. Document the "what's open vs commercial" boundary publicly so prospects know what they're getting in each tier.
5. Build the security-review evidence package (manifest + ledger + replay viewer + policy summary) entirely from community-tier outputs — no commercial dependency for compliance.

## Related PRD Sections

- §9.1 Open-Core Business Model
- §1 Product Vision (auditability is the differentiator)

## Domain References

- HashiCorp open-core history
- Apache License 2.0 (patent grant rationale)
- OSI-approved licensing best practices
