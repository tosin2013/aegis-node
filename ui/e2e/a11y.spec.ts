import { test, expect, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

/**
 * Accessibility audit for the Community UI (issue #165). Runs axe-core
 * against the four user-facing surfaces in both themes; fails on any
 * `serious` or `critical` violation, plus any colour-contrast finding.
 *
 * "Theme" here means the manual `data-theme` attribute set by the
 * TopNav toggle — we don't try to programmatically toggle the OS
 * `prefers-color-scheme` because Playwright's `colorScheme` emulation
 * doesn't trigger the manual-override CSS path (`:root[data-theme=…]`)
 * that ships to operators.
 */

const SURFACES = [
  { path: "/", label: "Home" },
  { path: "/chat", label: "Chat" },
  { path: "/manifest", label: "Manifest Builder" },
  { path: "/models", label: "Model Library" },
] as const;

const THEMES = ["dark", "light"] as const;
type Theme = (typeof THEMES)[number];

// Tags map to the WCAG 2.1 A + AA rule sets that axe-core ships.
// `best-practice` is intentionally omitted — those are advisory, not
// conformance gates, and would flag things like "this <section> has
// no <h2>" that the project doesn't commit to.
const AXE_TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"];

async function setTheme(page: Page, theme: Theme) {
  // Set the same localStorage key the production toggle writes. The
  // inline bootstrap script in `ui/index.html` reads this on first
  // paint so we never see the wrong theme even momentarily.
  await page.addInitScript((t: Theme) => {
    window.localStorage.setItem("aegis-theme", t);
  }, theme);
}

for (const theme of THEMES) {
  for (const { path, label } of SURFACES) {
    test(`${label} (${theme}): no serious/critical axe violations`, async ({
      page,
    }) => {
      await setTheme(page, theme);
      await page.goto(path);
      // Settle: the SPA needs one tick to mount and (for /chat, /models)
      // queue its first API call. We don't need the API to succeed —
      // we're auditing chrome — but waiting `networkidle` makes the
      // surface stable.
      await page.waitForLoadState("networkidle");

      // Sanity: confirm the theme attribute actually landed on <html>.
      // If this fails the audit ran against the wrong palette.
      await expect(page.locator("html")).toHaveAttribute(
        "data-theme",
        theme,
      );

      const results = await new AxeBuilder({ page })
        .withTags(AXE_TAGS)
        // Monaco editor (Manifest Builder) ships its own syntax-
        // highlighting theme with hard-coded colours that don't quite
        // hit WCAG AA at 13px — and the theme is part of Monaco's
        // upstream, not ours to fix. The editor's own a11y story is
        // covered by Monaco's accessibility-help dialog; this scan
        // gates the chrome we author, not the IDE's surface.
        .exclude(".monaco-editor")
        .analyze();

      const blocking = results.violations.filter(
        (v) =>
          v.impact === "serious" ||
          v.impact === "critical" ||
          v.id === "color-contrast",
      );

      // Surface the failing rule IDs in the assertion message so the
      // CI annotation tells a reviewer what to fix without spelunking
      // through the artifact bundle.
      const summary = blocking
        .map((v) => `${v.id} (impact=${v.impact}, n=${v.nodes.length})`)
        .join("; ");
      expect(blocking, summary || undefined).toEqual([]);
    });
  }
}
