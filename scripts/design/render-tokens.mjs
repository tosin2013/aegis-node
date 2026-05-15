#!/usr/bin/env node
// Generate ui/src/index.css's @theme block from the YAML front matter in
// docs/DESIGN.md. Per issue #163 (parent #161). docs/DESIGN.md is the
// single source of truth — this script writes the derived CSS so the
// two artefacts can't drift.
//
// Scope (matches #163):
//   - colors.*       → --color-{name}        (dark-mode values only;
//                                              light theme lands in #165)
//   - font.*         → --font-{name}
//   - typography.*   → --text-{name}         (font-size only;
//                                              richer typography lands in #164
//                                              when component primitives consume it)
//   - rounded.*      → --radius-{name}       (Tailwind v4 convention)
//   - spacing.*      → --spacing-{name}
//
// Out of scope per #163: components.*, elevation.*, colors.*.light. Those
// belong to sub-issues #164 and #165 respectively.
//
// Idempotency: same input → same output, byte-for-byte. CI can run this
// and then `git diff --exit-code ui/src/index.css` to catch drift.
//
// Run from any CWD; paths resolve relative to the script's location.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import yaml from "js-yaml";

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(__dirname, "..", "..");
const DESIGN_MD = resolve(REPO_ROOT, "docs", "DESIGN.md");
const INDEX_CSS = resolve(REPO_ROOT, "ui", "src", "index.css");

const HAND_WRITTEN_SENTINEL =
  "/* DESIGN.md tokens above — hand-written CSS below. */";

const GENERATED_HEADER = `/* GENERATED FROM docs/DESIGN.md by scripts/design/render-tokens.mjs.
 * Do not hand-edit this @theme block — run \`pnpm design:tokens\` after
 * updating the spec. Light-theme variant lands in issue #165;
 * component-level tokens land in #164. */`;

function parseFrontMatter(markdown) {
  const m = markdown.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n/);
  if (!m) {
    throw new Error(
      `docs/DESIGN.md: no YAML front matter found (expected delimited by ---)`,
    );
  }
  return yaml.load(m[1]);
}

function pickDark(value) {
  // Color tokens are objects { dark, light }; fall back to the value
  // itself if it's already a scalar (defensive — not currently used).
  if (typeof value === "string") return value;
  if (value && typeof value === "object" && "dark" in value) return value.dark;
  throw new Error(`Color token has no \`dark\` key: ${JSON.stringify(value)}`);
}

function normaliseFontStack(value) {
  // YAML folded scalars collapse newlines to spaces; collapse runs of
  // whitespace to single spaces so the output is one line.
  return String(value).replace(/\s+/g, " ").trim();
}

function renderThemeBlock(spec) {
  const lines = [];
  lines.push("@theme {");

  // ----- Colors (dark mode only at this stage) -----
  lines.push("  /* Colors — dark mode. Light mode lands in #165. */");
  for (const [name, value] of Object.entries(spec.colors ?? {})) {
    lines.push(`  --color-${name}: ${pickDark(value)};`);
  }
  lines.push("");

  // ----- Font stacks -----
  lines.push("  /* Font stacks (system-only — no webfont downloads, per ADR-031). */");
  for (const [name, value] of Object.entries(spec.font ?? {})) {
    lines.push(`  --font-${name}: ${normaliseFontStack(value)};`);
  }
  lines.push("");

  // ----- Typography font sizes -----
  //
  // The full typography spec (weight, line-height, letter-spacing) is
  // expressed per-component in the YAML; component primitives in #164
  // will read it directly. At this stage we only emit font-size as
  // --text-{name} so the existing utility-class surface keeps working.
  lines.push("  /* Typography (font-size only at this stage; richer typography");
  lines.push("   * is per-component and lands with primitives in #164). */");
  for (const [name, value] of Object.entries(spec.typography ?? {})) {
    if (value && typeof value === "object" && value.fontSize) {
      lines.push(`  --text-${name}: ${value.fontSize};`);
    }
  }
  lines.push("");

  // ----- Rounded -----
  lines.push("  /* Corner radii. */");
  for (const [name, value] of Object.entries(spec.rounded ?? {})) {
    lines.push(`  --radius-${name}: ${value};`);
  }
  lines.push("");

  // ----- Spacing -----
  lines.push("  /* Spacing scale. */");
  for (const [name, value] of Object.entries(spec.spacing ?? {})) {
    lines.push(`  --spacing-${name}: ${value};`);
  }

  lines.push("}");
  return lines.join("\n");
}

function buildIndexCss(themeBlock, existingCss) {
  // Pull the hand-written portion (everything after the sentinel) out
  // of the current file. If the sentinel is missing this is the first
  // generation pass — keep whatever was already below the @theme block
  // as the hand-written portion. The CSS that ships today has exactly
  // the structure we want to preserve, so the no-sentinel path is the
  // safe default for the first run.
  let handWritten;
  const sentinelIdx = existingCss.indexOf(HAND_WRITTEN_SENTINEL);
  if (sentinelIdx !== -1) {
    handWritten = existingCss
      .slice(sentinelIdx + HAND_WRITTEN_SENTINEL.length)
      .replace(/^\s*\n/, "");
  } else {
    // Pull everything after the first @theme block's closing brace.
    const themeMatch = existingCss.match(/@theme\s*{[\s\S]*?\n}\s*\n/);
    handWritten = themeMatch
      ? existingCss.slice(themeMatch.index + themeMatch[0].length)
      : "";
  }

  return [
    `@import "tailwindcss";`,
    "",
    GENERATED_HEADER,
    themeBlock,
    "",
    HAND_WRITTEN_SENTINEL,
    "",
    handWritten.trimStart(),
  ].join("\n");
}

function main() {
  const markdown = readFileSync(DESIGN_MD, "utf8");
  const spec = parseFrontMatter(markdown);
  const themeBlock = renderThemeBlock(spec);

  const existingCss = readFileSync(INDEX_CSS, "utf8");
  const next = buildIndexCss(themeBlock, existingCss);

  if (next === existingCss) {
    console.log("ui/src/index.css already in sync with docs/DESIGN.md.");
    return;
  }

  writeFileSync(INDEX_CSS, next);
  console.log("ui/src/index.css regenerated from docs/DESIGN.md.");
}

main();
