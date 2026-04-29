// Headless DOM snapshot test for the F8 replay viewer.
// Per F8-C / ADR-010 / issue #64.
//
// Approach:
//   1. Load tools/replay-viewer/index.html in jsdom (no real browser
//      needed — the viewer is vanilla JS + DOM, no fetch / no remote
//      anything per the F8-A air-gap contract).
//   2. Provide globalThis.crypto (jsdom doesn't ship Web Crypto in v22-29).
//   3. Drive the viewer's exposed render flow: parseJsonl -> verifyChain
//      -> render. We bypass the file-picker change event because jsdom's
//      FileReader/DataTransfer integration is brittle; the viewer exposes
//      these primitives at the top of its <script> by construction.
//   4. Walk the resulting DOM and produce a structured summary (per-entry
//      type + key text snippets + integrity banner state). JSON is the
//      diff-friendliest form for review.
//   5. `check`     — compare summary against expected.json, exit non-zero
//                    on drift.
//      `regenerate`— overwrite expected.json. Run after intentional
//                    viewer/fixture changes.
//
// Usage:
//   node run.mjs check        # CI mode
//   node run.mjs regenerate   # local: refresh expected.json
//
// Why jsdom over Playwright:
//   The viewer is small, pure JS, and uses no browser-specific APIs we
//   care about beyond the standard DOM + Web Crypto. Playwright would
//   add ~300 MB of Chromium download to CI for negligible coverage gain.
//   jsdom is ~9 MB, runs the test in ~1 s, and pins exactly the runtime
//   we need.

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { webcrypto } from "node:crypto";
import { JSDOM, VirtualConsole } from "jsdom";

const __dirname = dirname(fileURLToPath(import.meta.url));
const VIEWER = join(__dirname, "..", "index.html");
const FIXTURE = join(__dirname, "..", "fixtures", "sample-session.jsonl");
const EXPECTED = join(__dirname, "expected.json");

// --- 1. Load the viewer in a jsdom DOM. Run scripts immediately so the
// inline <script> defines parseJsonl / verifyChain / render on window.
const html = readFileSync(VIEWER, "utf8");
const virtualConsole = new VirtualConsole();
virtualConsole.on("error", (e) => console.error("[jsdom error]", e));

const dom = new JSDOM(html, {
  runScripts: "dangerously",
  pretendToBeVisual: true,
  virtualConsole,
});
const { window } = dom;

// --- 2. Web Crypto polyfill (jsdom doesn't ship it as of 29.x).
if (!window.crypto || !window.crypto.subtle) {
  Object.defineProperty(window, "crypto", { value: webcrypto, configurable: true });
}

// --- 3. Drive parseJsonl -> verifyChain -> render programmatically.
// The viewer's <script> defines these at top-level, so they're available
// on window after construction.
const fixtureText = readFileSync(FIXTURE, "utf8");
const entries = window.parseJsonl(fixtureText);
const integrity = await window.verifyChain(fixtureText);
window.render(entries, "sample-session.jsonl", integrity);

// --- 4. Walk the rendered DOM and produce a structured summary.
const summary = extractSummary(window.document);

// --- 5. Compare or regenerate.
const mode = process.argv[2] ?? "check";
if (mode === "regenerate") {
  writeFileSync(EXPECTED, JSON.stringify(summary, null, 2) + "\n");
  console.log(`wrote ${EXPECTED}`);
  process.exit(0);
}
if (mode !== "check") {
  console.error(`usage: node run.mjs [check|regenerate]`);
  process.exit(2);
}

const expected = JSON.parse(readFileSync(EXPECTED, "utf8"));
const got = summary;
if (JSON.stringify(expected) === JSON.stringify(got)) {
  console.log(`OK: ${entries.length} entries, integrity=${integrity.state}`);
  process.exit(0);
}

// Drift — print a small diff hint and the full got payload so a
// reviewer can decide "intentional, regenerate" vs "regression".
console.error("FAIL: rendered DOM does not match expected snapshot.");
console.error(
  "  rerun `node tools/replay-viewer/snapshot/run.mjs regenerate` if the change is intentional.\n",
);
console.error("=== expected ===");
console.error(JSON.stringify(expected, null, 2));
console.error("=== got ===");
console.error(JSON.stringify(got, null, 2));
process.exit(1);

// ---------------------------------------------------------------------
// Summary extraction. Targets the structural invariants we care about,
// not whitespace / CSS. Keeps the snapshot readable and diff-stable.
// ---------------------------------------------------------------------

function extractSummary(doc) {
  const banner = doc.querySelector(".integrity-banner");
  const summary = doc.getElementById("summary");
  const chips = [];
  if (summary) {
    for (const c of summary.querySelectorAll("span > code")) {
      chips.push({ value: text(c) });
    }
  }
  const entries = [];
  for (const card of doc.querySelectorAll(".entry")) {
    const type = card.getAttribute("data-type") || "unknown";
    const seqEl = card.querySelector(".seq");
    const tsEl = card.querySelector(".ts");
    const pillEl = card.querySelector(".type-pill");
    const rows = [];
    for (const r of card.querySelectorAll(".body .row")) {
      const k = r.querySelector(".k");
      const v = r.querySelector(".v");
      if (k && v) rows.push([text(k), text(v)]);
    }
    entries.push({
      seq: seqEl ? text(seqEl) : null,
      type,
      pill: pillEl ? text(pillEl) : null,
      timestamp: tsEl ? text(tsEl) : null,
      rows,
    });
  }
  return {
    integrity: {
      state: banner ? banner.className.replace("integrity-banner ", "").trim() : "missing",
      label: banner ? text(banner) : "(no banner)",
    },
    chips,
    entryCount: entries.length,
    entries,
  };
}

function text(el) {
  return (el.textContent || "").replace(/\s+/g, " ").trim();
}
