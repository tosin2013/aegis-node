/**
 * Manifest-template types consumed by the Builder dropdown.
 *
 * Templates are authored as YAML files in
 * `examples/templates/manifests/*.yaml` (relative to repo root).
 * Each carries a structured comment block at the top — the
 * `=== Aegis-Node Manifest Template ===` delimiter — that the
 * loader parses into the dropdown card metadata. See
 * `examples/templates/manifests/README.md` for the convention.
 */

export type TemplateDifficulty = "starter" | "intermediate" | "advanced";

export type TemplateCategory =
  | "Filesystem"
  | "Network/egress"
  | "Approval & destructive ops"
  | "MCP integrity"
  | "Database & SQL"
  | "Cost / runaway"
  | "Compliance"
  | "Multi-tenancy"
  | "Browser / RAG"
  | "Supply chain"
  | "Workflow";

export interface Template {
  /** Stable kebab-case ID; matches the `id:` field in the metadata block. */
  id: string;
  /** Human-readable title shown on the dropdown card. */
  title: string;
  /** One of the defined categories; templates with unknown categories
   *  are rendered under a fallback "Uncategorised" group. */
  category: TemplateCategory | "Uncategorised";
  /** Tier hint for the dropdown grouping. Unknown values fall back
   *  to "starter" so a malformed template still renders. */
  difficulty: TemplateDifficulty;
  /** One-liner pain statement, with citation URL inline.
   *  Surfaced as a tooltip on the dropdown card. */
  pain: string;
  /** ADR numbers referenced in the metadata block. Rendered as
   *  small badges on the card so reviewers can navigate to the
   *  underlying decision. */
  adrs: number[];
  /** Full YAML body — the metadata-block delimiter and freeform
   *  docs are preserved so operators see the inline comments
   *  when the template loads into the editor. */
  yaml: string;
}
