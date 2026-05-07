# Manifest Builder Templates

Curated starter manifests for the Phase 1d Community UI Manifest
Builder ([ADR-031](../../../docs/adrs/031-community-webui-for-local-collaboration.md)).
Each template is anchored in a documented operator pain point — public
CVE, postmortem, or recurring forum complaint — and demonstrates how
Aegis-Node's manifest schema closes that specific gap.

## Why this directory exists

Operators authoring a permission manifest from a blank YAML file is a
recipe for misconfiguration. The community templates ship as concrete
starting points: copy one, edit the paths and identity blocks, save.
The Web UI's Manifest Builder reads this directory at build time and
exposes the templates as a categorized dropdown.

User-added templates live separately at
`~/.config/aegis/manifests/templates/*.yaml` and are read at request
time by `GET /api/v1/templates`. Same metadata convention, no special
handling — drop a YAML file, the dropdown picks it up. To share a
local template upstream, PR it into this directory.

## Template metadata convention

Each template starts with a structured comment block that the SPA
parses into the dropdown card:

```yaml
# === Aegis-Node Manifest Template ===
# id: <kebab-case-stable-id>
# title: <human-readable name>
# category: <Filesystem | Network/egress | Approval & destructive ops |
#            MCP integrity | Database & SQL | Cost / runaway |
#            Compliance | Multi-tenancy | Browser / RAG |
#            Supply chain | Workflow>
# difficulty: <starter | intermediate | advanced>
# pain: <one-line problem statement + URL to source>
# adrs: [<ADR numbers>]
# ====================================
#
# <freeform docs about when an operator reaches for this template>

schemaVersion: "1"
# ...rest of the manifest...
```

The `=== Aegis-Node Manifest Template ===` delimiter is the
parser's signal. Everything between it and the closing `===` line
is structured metadata; below the closing delimiter is freeform
documentation that the dropdown shows in a tooltip.

The metadata fields are YAML comments only — they don't appear in
the parsed manifest, so `aegis validate` ignores them. Operators
who copy the YAML out of the editor get the full file (delimiters
included); the comment block survives as inline documentation in
their working copy.

## Adding a template

1. Copy an existing template that's closest to your scenario.
2. Update the metadata block: new `id`, `title`, `category`,
   `difficulty`, `pain` (with URL), and the relevant `adrs` list.
3. Edit the manifest body — paths, MCP servers, write grants, etc.
4. Run `aegis validate <your-template>.yaml` to confirm the
   manifest parses and lints clean.
5. PR the file to this directory.

The pain-point citation is required, not optional. A template
without a documented pain isn't a template — it's an example. The
distinction matters because the dropdown's value is "here's a
shape that defends against a real attack you've heard of." If
the pain point is weak, ship it under [`examples/`](../../) as a
full demo program instead.

## Categories

| Category | What it covers |
|---|---|
| Filesystem | `tools.filesystem.read` / `write` patterns; pre-validation against path traversal |
| Network/egress | `tools.network.outbound` allowlists; egress quotas; SSRF defenses |
| Approval & destructive ops | F3 approval gate + F7 write grants + exec wrappers for risky verbs |
| MCP integrity | Defenses against MCP rug-pulls, tool poisoning, cross-tenant leaks |
| Database & SQL | Read-only DB access via SQLite/Postgres MCP with verb-level allowlists |
| Cost / runaway | Token/wallclock circuit breakers, quota schemas |
| Compliance | CMMC / NIST 800-171 / FedRAMP-aligned manifest shapes |
| Multi-tenancy | Tenant-scoped retrieval; per-tenant identity binding |
| Browser / RAG | Web-automation agents; vector-store-aware authz |
| Supply chain | Defenses against package hallucination, MCP supply-chain attacks |
| Workflow | Session-forking, multi-model patterns |

## Difficulty tiers

- **starter** — a sane default for the most common shape; minimal
  surface, deny-by-default, no advanced features. Ideal for an
  operator's first manifest.
- **intermediate** — composes 2–3 manifest features (e.g. MCP +
  `pre_validate` + targeted network allowlist). Assumes familiarity
  with the [F1–F10 control set](../../../docs/adrs/004-declarative-yaml-permission-manifest.md).
- **advanced** — uses features at the boundary of v1.0.0 scope
  (ADR-027 quotas, ADR-029 task-scoped grants, multi-turn caps).
  May reference manifest fields that haven't shipped yet — mark
  them clearly in the YAML and operators will see "not yet
  implemented" warnings from `aegis validate`.

## Phase 1d.1d UI integration

The dropdown UI ships in sub-phase 1d.1d alongside the live
`aegis validate` integration ([Phase 1d implementation
plan](../../../docs/plans/v0.9.5-ui-implementation.md)). Until
then, this directory is a reviewable artifact for operators —
copy a template into your editor of choice and pair it with
`aegis validate` from the CLI.

## Related

- [examples/](../../) — full demo programs (manifest + setup.sh +
  README) for end-to-end scenarios. A template gets promoted to a
  full example when it includes a runnable program.
- [ADR-031](../../../docs/adrs/031-community-webui-for-local-collaboration.md)
  — Community UI design including the templates dropdown
- [ADR-024](../../../docs/adrs/024-mcp-args-prevalidation.md) —
  `pre_validate` schema referenced by several templates
