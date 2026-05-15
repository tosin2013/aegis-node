#!/usr/bin/env node
// Generate ui/src/index.css's @theme block from the YAML front matter in
// docs/DESIGN.md. docs/DESIGN.md is the single source of truth — this
// script writes the derived CSS so the two artefacts can't drift.
//
// Issues touched: #163 (initial generator), #165 (this — dual-theme
// emission + data-theme overrides). Parent tracker: #161.
//
// Emission layout:
//   1. Default `@theme { ... }` — dark colours + font/text/radius/spacing.
//      Tailwind v4 registers these as CSS custom properties on :root and
//      generates utility classes (bg-bg, text-fg, ...). Dark is the
//      default per ADR-031's identifier-dense aesthetic.
//   2. `@media (prefers-color-scheme: light) :root { ... }` — light colour
//      overrides + `color-scheme: light` so native controls flip too.
//   3. `:root[data-theme="dark"]` / `:root[data-theme="light"]` — manual
//      override via a localStorage-driven attribute on <html>. Wins over
//      the media query because the attribute selector is more specific.
//
// Non-colour tokens (font, text size, radius, spacing) don't vary by
// mode, so they live in `@theme` only.
//
// Idempotency: same input → same output, byte-for-byte. CI can run this
// and then `git diff --exit-code ui/src/index.css` to catch drift.
//
// Lives under `ui/scripts/` (not the repo-root `scripts/`) so Node's
// module resolver walks into `ui/node_modules/` for `js-yaml`.

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

const GENERATED_HEADER = `/* GENERATED FROM docs/DESIGN.md by ui/scripts/render-tokens.mjs.
 * Do not hand-edit anything above the sentinel comment — run
 * \`pnpm design:tokens\` after updating the spec. Both themes
 * (dark + light) emit from a single source. */`;

function parseFrontMatter(markdown) {
  const m = markdown.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n/);
  if (!m) {
    throw new Error(
      `docs/DESIGN.md: no YAML front matter found (expected delimited by ---)`,
    );
  }
  return yaml.load(m[1]);
}

function pickMode(value, mode) {
  if (typeof value === "string") return value;
  if (value && typeof value === "object" && mode in value) return value[mode];
  throw new Error(
    `Color token has no \`${mode}\` key: ${JSON.stringify(value)}`,
  );
}

function normaliseFontStack(value) {
  return String(value).replace(/\s+/g, " ").trim();
}

function renderThemeBlock(spec) {
  const lines = [];
  lines.push("@theme {");

  // Dark is the default — Tailwind v4 registers these as the canonical
  // utility-class sources (bg-bg, text-fg, ...). Light overrides come
  // later via :root selectors so the utility-class names stay stable.
  lines.push("  /* Colors — dark mode (default). Light overrides emit below. */");
  for (const [name, value] of Object.entries(spec.colors ?? {})) {
    lines.push(`  --color-${name}: ${pickMode(value, "dark")};`);
  }
  lines.push("");

  lines.push("  /* Font stacks (system-only — no webfont downloads, per ADR-031). */");
  for (const [name, value] of Object.entries(spec.font ?? {})) {
    lines.push(`  --font-${name}: ${normaliseFontStack(value)};`);
  }
  lines.push("");

  lines.push("  /* Typography (font-size only; richer typography lives in component primitives). */");
  for (const [name, value] of Object.entries(spec.typography ?? {})) {
    if (value && typeof value === "object" && value.fontSize) {
      lines.push(`  --text-${name}: ${value.fontSize};`);
    }
  }
  lines.push("");

  lines.push("  /* Corner radii. */");
  for (const [name, value] of Object.entries(spec.rounded ?? {})) {
    lines.push(`  --radius-${name}: ${value};`);
  }
  lines.push("");

  lines.push("  /* Spacing scale. */");
  for (const [name, value] of Object.entries(spec.spacing ?? {})) {
    lines.push(`  --spacing-${name}: ${value};`);
  }

  lines.push("}");
  return lines.join("\n");
}

function renderModeBlock(spec, mode, selector) {
  // Renders a CSS rule that overrides only the colour custom properties
  // for one theme mode. `selector` is the wrapping selector text (e.g.
  // `:root[data-theme="light"]` or the body of an @media block).
  const lines = [];
  lines.push(`${selector} {`);
  lines.push(`  color-scheme: ${mode};`);
  for (const [name, value] of Object.entries(spec.colors ?? {})) {
    lines.push(`  --color-${name}: ${pickMode(value, mode)};`);
  }
  lines.push("}");
  return lines.join("\n");
}

function renderModeOverrides(spec) {
  // The order matters for the cascade:
  //   1. @theme registered dark tokens on :root.
  //   2. :root sets color-scheme: dark as the default (not in @theme
  //      because color-scheme is a regular CSS property, not a token).
  //   3. @media (prefers-color-scheme: light) overrides when the OS asks.
  //   4. data-theme attribute overrides everything when set by the
  //      manual toggle (higher specificity than plain :root).
  return [
    "/* Default color-scheme for native form controls / scrollbars. */",
    ":root {",
    "  color-scheme: dark;",
    "}",
    "",
    "/* Light mode — applies when the OS prefers light. */",
    "@media (prefers-color-scheme: light) {",
    renderModeBlock(spec, "light", "  :root")
      .split("\n")
      .map((l) => "  " + l)
      .join("\n")
      .replace(/^  /, ""),
    "}",
    "",
    "/* Manual overrides set by the toggle in TopNav. Wins over the media query. */",
    renderModeBlock(spec, "dark", ':root[data-theme="dark"]'),
    "",
    renderModeBlock(spec, "light", ':root[data-theme="light"]'),
  ].join("\n");
}

function buildIndexCss(spec, existingCss) {
  // Pull the hand-written portion (everything after the sentinel) out
  // of the current file. If the sentinel is missing this is the first
  // generation pass — keep whatever was already below the last generated
  // block as the hand-written portion.
  let handWritten;
  const sentinelIdx = existingCss.indexOf(HAND_WRITTEN_SENTINEL);
  if (sentinelIdx !== -1) {
    handWritten = existingCss
      .slice(sentinelIdx + HAND_WRITTEN_SENTINEL.length)
      .replace(/^\s*\n/, "");
  } else {
    // Best-effort fallback: cut at the first @theme block's closing brace.
    const themeMatch = existingCss.match(/@theme\s*{[\s\S]*?\n}\s*\n/);
    handWritten = themeMatch
      ? existingCss.slice(themeMatch.index + themeMatch[0].length)
      : "";
  }

  const themeBlock = renderThemeBlock(spec);
  const modeOverrides = renderModeOverrides(spec);

  return [
    `@import "tailwindcss";`,
    "",
    GENERATED_HEADER,
    themeBlock,
    "",
    modeOverrides,
    "",
    HAND_WRITTEN_SENTINEL,
    "",
    handWritten.trimStart(),
  ].join("\n");
}

function main() {
  const markdown = readFileSync(DESIGN_MD, "utf8");
  const spec = parseFrontMatter(markdown);

  const existingCss = readFileSync(INDEX_CSS, "utf8");
  const next = buildIndexCss(spec, existingCss);

  if (next === existingCss) {
    console.log("ui/src/index.css already in sync with docs/DESIGN.md.");
    return;
  }

  writeFileSync(INDEX_CSS, next);
  console.log("ui/src/index.css regenerated from docs/DESIGN.md.");
}

main();
