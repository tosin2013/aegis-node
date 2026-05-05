# 34. Enterprise Management Dashboard and RBAC

**Status:** Proposed
**Date:** 2026-05-05
**Domain:** Enterprise UI / governance (extends [ADR-016](016-open-core-licensing-model.md), [ADR-031](031-community-webui-for-local-collaboration.md), supports Phase 2.5)
**Targets:** v2.5.0 Phase 2.5 (release date 2027-03-01)

## Context

The [Community UI (ADR-031)](031-community-webui-for-local-collaboration.md)
is purposefully scoped to "the operator and the runtime share a
host." It's the right tool for a developer evaluating Aegis-Node, a
solo engineer running a personal agent, or a small team
experimenting with a single workload.

It's the wrong tool for:

- A Fortune-500 security team running 100+ Aegis-Node agents
  across a Kubernetes cluster.
- A regulated enterprise that needs to demonstrate to auditors
  that only "Security Admins" can mutate manifests, while
  "Operators" can only chat and approve.
- A managed-services provider running Aegis-Node for multiple
  customer tenants, with strict cross-tenant isolation.
- A defense contractor needing automated CMMC 2.0 / FedRAMP
  evidence packages exported continuously rather than per-session.

The [Open-Core Licensing Model (ADR-016)](016-open-core-licensing-model.md)
sets the boundary: the community runtime is sufficient to **pass** a
security review. Commercial tiers add **convenience and
integration** — multi-tenancy, fleet management, automated
compliance reporting — not previously-blocked compliance.

Phase 2.5 (v2.5.0) targets these enterprise workflows. Phase 2
(v2.0.0) ships the [Kubernetes Operator + CRDs](002-split-language-architecture-rust-and-go.md);
Phase 2.5 ships the management surface that operates on top of them.

## Decision

**Ship an Enterprise Management Dashboard in v2.5.0 under a
commercial license. It is a multi-tenant, RBAC-enforced, network-
reachable UI deployed alongside the Kubernetes Operator. It surfaces
fleet-wide agent state, multi-tenant manifest authoring, SSO/SAML
identity, and automated compliance report generation. The runtime
gates that drive security (F1–F10 + ADR-025–030) are the same as
the community tier — the Enterprise UI does not weaken or replace
them; it operates them at scale.**

### Surfaces

1. **Multi-tenant fleet dashboard.** A single view showing every
   Aegis-Node agent across the Kubernetes cluster (or fleet). Per-
   agent cards show: workload identity (SPIFFE ID), current session
   state (idle / running / pending-approval), aggregate quota usage,
   recent F9 ledger violations, last-attestation timestamp. Filters
   by tenant, namespace, agent class, manifest digest.

2. **RBAC + SSO/SAML.** Roles configurable per organization, with
   defaults:
   - **Security Admin** — author + approve manifests, approve all
     F3 escalations, configure tenant policy. Cannot run agents.
   - **Operator** — run agents, approve F3 prompts within scope of
     assigned workloads, view ledger entries. Cannot mutate
     manifests.
   - **Auditor** — read-only access to all sessions, ledgers,
     attestations, compliance exports. Cannot run agents or
     mutate anything.
   - **Custom roles** — declared in commercial-tier configuration,
     enforced at the dashboard's API gateway and at the operator
     pattern in the Kubernetes Operator (Phase 2).
   SSO via SAML 2.0 + OIDC. Identity providers: Okta, Azure AD,
   Google Workspace, Auth0, Keycloak (open-source IdP for
   air-gapped deployments).

3. **Multi-tenant manifest authoring.** The Visual Manifest Builder
   from [ADR-031](031-community-webui-for-local-collaboration.md)
   evolves to support **per-tenant** manifest stores with policy
   inheritance (tenant policy → workload policy → session policy).
   Aggregate quotas ([ADR-027](027-aggregate-quota-schema.md)) can
   be set at the tenant level and inherited; tenants cannot exceed
   their parent caps.

4. **Live + historical fleet ledger view.** The community UI shows
   one agent's live trajectory; the Enterprise UI streams from
   every agent in the fleet, with filtering, search, and saved
   views. Backed by a tenant-scoped persistent ledger store
   (Phase 2's [persistent ledger storage](../../RELEASE_PLAN.md)).
   The cryptographic chain remains the source of truth — the UI is
   a query surface, not a re-write of the audit data.

5. **Automated compliance report exports.** Continuous evidence-pack
   generation from the F9 ledger:
   - **CMMC 2.0 Level 2** — quarterly auditor-ready report mapping
     observed runtime activity to NIST SP 800-171 controls (the
     [docs/COMPLIANCE_MATRIX.md](../COMPLIANCE_MATRIX.md) is the
     mapping; the report is the evidence).
   - **FedRAMP Moderate / High** — same shape, FedRAMP control
     family mapping.
   - **SOC 2 Type II** — control-objective coverage from the
     ledger's access entries + approval decisions.
   - **EU AI Act high-risk system** — assessment artifact bundle
     for high-risk classification AI systems.
   Reports are signed by the runtime's identity and exported as
   PDF + JSON-LD.

### Architecture

```text
┌──────────────────────────────────────────────────────────────┐
│  Enterprise Management Dashboard (multi-tenant)              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │  React SPA + TypeScript + Tailwind                      │ │
│  │  - Fleet dashboard / RBAC / manifests / reports         │ │
│  └─────────┬───────────────────────────────────────────────┘  │
│            │ HTTPS + SAML/OIDC session                        │
└────────────┼──────────────────────────────────────────────────┘
             │
┌────────────▼──────────────────────────────────────────────────┐
│  aegis-mgmt API (commercial-tier)                             │
│  ┌────────────────────────────────────────────────────────┐  │
│  │ - Auth / RBAC enforcement (every request)              │  │
│  │ - Tenant routing / isolation                           │  │
│  │ - Persistent ledger query API (read-only)              │  │
│  │ - Manifest authoring API (writes through Operator)     │  │
│  │ - Report generator (cron + on-demand)                  │  │
│  └────────────────┬───────────────────────────────────────┘  │
└───────────────────┼───────────────────────────────────────────┘
                    │ Kubernetes API
┌───────────────────▼───────────────────────────────────────────┐
│  Aegis-Node Kubernetes Operator (Phase 2 / v2.0.0)            │
│  ┌────────────────┐  ┌────────────────────┐  ┌────────────┐  │
│  │ AegisAgent CRD │  │ PermissionManifest │  │ Ledger CRD │  │
│  │                │  │ CRD                │  │            │  │
│  └────────────────┘  └────────────────────┘  └────────────┘  │
└───────────────────────────────────────────────────────────────┘
```

The dashboard is layered **on top of** the Phase 2 Kubernetes
Operator. The Operator is the source of truth; the dashboard is a
governance + observability surface.

### Non-replacement of community runtime

The runtime gates that pass a security review remain the
community-tier F1–F10 + ADR-025–030 implementations. The Enterprise
UI does not provide a parallel enforcement path. An auditor
inspecting the runtime sees:

- The same `aegis verify` chain validation.
- The same hash-chained F9 ledger.
- The same per-turn SVID rebinding (ADR-030).
- The same aggregate quota enforcement (ADR-027).

What's commercial:
- Reading those primitives at fleet scale (cross-agent dashboard).
- Managing those primitives across tenants (RBAC).
- Continuously publishing those primitives as auditor evidence
  (compliance exports).

A customer who churns from the commercial tier still has every
audit and security property they had on the commercial tier — the
data lives in the F9 ledger, the Operator's CRDs, and the
manifests, all open formats. They lose the ergonomics and the
report generation, not the runtime guarantees.

## Why not the alternatives

- **Single-tenant Enterprise UI.** Doesn't solve the actual
  enterprise problem (multi-team / multi-customer governance).
  Customers running Aegis-Node for one team can stay on the
  Community UI.
- **Open-source Enterprise UI.** Conflicts with the Open-Core
  model ([ADR-016](016-open-core-licensing-model.md)) — the
  commercial tier's economic moat is the Day-2 ops UX. Free fleet
  management would re-invest engineering time without funding it.
- **Bolt RBAC onto the Community UI.** Would muddy the threat model
  ("localhost-only single-user" is load-bearing simplicity) and
  expand the open-source maintenance surface in a way that
  commercial customers won't fund. Two distinct UIs at distinct
  threat models is cleaner.
- **Build on Grafana / Datadog / general-purpose dashboarding.**
  Plausible for a thin observability layer, but compliance report
  generation is Aegis-Node-specific (the ledger schema, the F-feature
  mapping). Generic dashboards can't generate signed
  CMMC/FedRAMP evidence packs without bespoke Aegis-Node logic.
  The Enterprise UI ships that logic.
- **Defer compliance reporting; ship dashboard-only first.** Removes
  the load-bearing commercial differentiator. Reports are the
  highest-margin feature; shipping dashboard without them
  underprices the tier.

## Implementation tracking

- New crate `crates/aegis-mgmt-api/` (commercial license; lives in
  the Aegis-Node Enterprise repository, not the open-source
  monorepo).
- New SPA `enterprise-ui/` (commercial license; same separation).
- Reuses: Kubernetes Operator (Phase 2), the F9 ledger format
  (ADR-026), the SPIFFE identity model (ADR-003 + ADR-030), the
  policy schema (ADR-004 + ADR-027).
- Tracking: v2.5.0 milestone in RELEASE_PLAN.md. Issues live in
  the Enterprise repository (private).

## Open questions for follow-up

- **Air-gapped deployments.** Customers in classified environments
  cannot use Okta / Azure AD. Keycloak as the bundled IdP for
  air-gapped tier. Manifest authoring and reports must work fully
  offline.
- **Pricing / packaging.** Commercial decision, out of scope for
  this ADR. The Open-Core model ([ADR-016](016-open-core-licensing-model.md))
  sets the philosophical boundary; the SKU shape is for the
  business team.
- **Cross-tenant policy floor.** A managed-services provider may
  want to enforce a cross-tenant minimum (e.g., "all tenants must
  have F6 network deny-by-default"). Implementable as a
  dashboard-level admission webhook against the Kubernetes
  Operator. v2.6.x scope.
- **Compliance frameworks beyond CMMC / FedRAMP / SOC 2 / EU AI
  Act.** ISO 27001, HIPAA, PCI-DSS — added per customer demand
  rather than upfront. Each is a mapping pass over the existing
  ledger schema; mechanism is reusable.

## References

- [ADR-016](016-open-core-licensing-model.md) Open-core licensing
- [ADR-031](031-community-webui-for-local-collaboration.md) Community UI
- [docs/COMPLIANCE_MATRIX.md](../COMPLIANCE_MATRIX.md) Compliance mapping
- [RELEASE_PLAN.md](../../RELEASE_PLAN.md) Phase 2 + 2.5 + 3
- HashiCorp Vault Enterprise (open-core reference)
- GitLab Premium / Ultimate (open-core reference)
