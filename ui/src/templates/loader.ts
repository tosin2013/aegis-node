/**
 * Build-time loader for the curated manifest templates under
 * `examples/templates/manifests/*.yaml`.
 *
 * Vite's `import.meta.glob` resolves the path relative to this file
 * and inlines each template's raw YAML as a string. The parser
 * extracts the metadata block delimited by
 * `=== Aegis-Node Manifest Template ===` into a typed
 * [`Template`].
 *
 * Why build-time embed and not runtime fetch:
 *
 *   - Airgap-clean — templates ship inside the SPA bundle, no
 *     API call to `/api/v1/templates` is required for the curated
 *     set
 *   - Bundle-size-cheap — at ~1 KB raw YAML each, 20 templates
 *     is ~20 KB before gzip compression
 *   - Same source-of-truth as `aegis validate` — the YAML files
 *     this loader reads are exactly the files operators run
 *     `aegis validate` against
 *
 * User-added templates live separately at
 * `~/.config/aegis/manifests/templates/*.yaml` and will be
 * fetched at runtime by `GET /api/v1/templates` (sub-phase 1d.1d
 * follow-up). Same `Template` shape, same metadata-block
 * convention.
 */
import type {
  Template,
  TemplateCategory,
  TemplateDifficulty,
} from "./types";

const KNOWN_CATEGORIES: ReadonlySet<TemplateCategory> = new Set([
  "Filesystem",
  "Network/egress",
  "Approval & destructive ops",
  "MCP integrity",
  "Database & SQL",
  "Cost / runaway",
  "Compliance",
  "Multi-tenancy",
  "Browser / RAG",
  "Supply chain",
  "Workflow",
]);

const KNOWN_DIFFICULTIES: ReadonlySet<TemplateDifficulty> = new Set([
  "starter",
  "intermediate",
  "advanced",
]);

const METADATA_BLOCK_REGEX =
  /# === Aegis-Node Manifest Template ===\n([\s\S]*?)\n# =+/;

const MetaScalarKeys = ["id", "title", "category", "difficulty"] as const;

function parseMetadata(raw: string): Omit<Template, "yaml"> | null {
  const match = METADATA_BLOCK_REGEX.exec(raw);
  if (!match) return null;
  const body = match[1];

  const meta: Record<string, string> = {};
  for (const line of body.split("\n")) {
    const m = /^# ([\w-]+):\s*(.*)$/.exec(line);
    if (m) {
      const key = m[1];
      const value = m[2].trim();
      // Pain spans multiple lines; concatenate into the first
      // value rather than overwriting on continuation lines.
      if (key in meta && key === "pain") {
        meta[key] += " " + value;
      } else {
        meta[key] = value;
      }
    } else if (line.startsWith("#       ")) {
      // Continuation of a multi-line value. The metadata block's
      // formatting in the templates puts wrapped pain text on
      // lines that start with `#` + 7 spaces.
      const trimmed = line.replace(/^#\s+/, "").trim();
      if (meta.pain) meta.pain += " " + trimmed;
    }
  }

  for (const key of MetaScalarKeys) {
    if (!meta[key]) return null;
  }

  // adrs: "[007, 011]" → [7, 11]
  const adrs: number[] = [];
  const adrsRaw = meta.adrs ?? "";
  const adrsMatch = /\[([^\]]*)\]/.exec(adrsRaw);
  if (adrsMatch) {
    for (const piece of adrsMatch[1].split(",")) {
      const n = parseInt(piece.trim(), 10);
      if (!Number.isNaN(n)) adrs.push(n);
    }
  }

  const category: TemplateCategory | "Uncategorised" = KNOWN_CATEGORIES.has(
    meta.category as TemplateCategory,
  )
    ? (meta.category as TemplateCategory)
    : "Uncategorised";

  const difficulty: TemplateDifficulty = KNOWN_DIFFICULTIES.has(
    meta.difficulty as TemplateDifficulty,
  )
    ? (meta.difficulty as TemplateDifficulty)
    : "starter";

  return {
    id: meta.id,
    title: meta.title,
    category,
    difficulty,
    pain: meta.pain ?? "",
    adrs,
  };
}

// Vite resolves this path at build time. `eager: true` avoids the
// dynamic-import boundary so the YAML strings are inlined into
// the templates chunk rather than fetched as separate assets at
// runtime — keeps the dropdown population synchronous.
const RAW_YAML = import.meta.glob<string>(
  "../../../examples/templates/manifests/*.yaml",
  { query: "?raw", import: "default", eager: true },
);

const ALL_TEMPLATES: ReadonlyArray<Template> = Object.entries(RAW_YAML)
  .map(([path, yaml]) => {
    const meta = parseMetadata(yaml);
    if (!meta) {
      console.warn(`[templates] skipping ${path} — missing metadata block`);
      return null;
    }
    return { ...meta, yaml } satisfies Template;
  })
  .filter((t): t is Template => t !== null)
  .sort((a, b) => {
    // Group by difficulty (starter → intermediate → advanced),
    // then by category, then by title for stable ordering.
    const order: Record<TemplateDifficulty, number> = {
      starter: 0,
      intermediate: 1,
      advanced: 2,
    };
    if (order[a.difficulty] !== order[b.difficulty])
      return order[a.difficulty] - order[b.difficulty];
    if (a.category !== b.category) return a.category.localeCompare(b.category);
    return a.title.localeCompare(b.title);
  });

export function getAllTemplates(): ReadonlyArray<Template> {
  return ALL_TEMPLATES;
}

export function getTemplateById(id: string): Template | undefined {
  return ALL_TEMPLATES.find((t) => t.id === id);
}

export function groupByDifficulty(): Record<
  TemplateDifficulty,
  ReadonlyArray<Template>
> {
  return {
    starter: ALL_TEMPLATES.filter((t) => t.difficulty === "starter"),
    intermediate: ALL_TEMPLATES.filter((t) => t.difficulty === "intermediate"),
    advanced: ALL_TEMPLATES.filter((t) => t.difficulty === "advanced"),
  };
}
