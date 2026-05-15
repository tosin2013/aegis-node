import { defineConfig, devices } from "@playwright/test";

/**
 * Playwright configuration for the a11y audit (issue #165).
 * Single chromium project; no auth, no fixtures. Spawns `vite preview`
 * automatically on a fixed port so the same config works in CI and
 * locally.
 *
 * The UI talks to `crates/ui-server` for chat/model APIs over a
 * WebSocket. Without that backend running these tests see the
 * "connecting…" state — that's fine, we're auditing the SPA chrome,
 * not full data round-trips. A separate e2e suite covers backend
 * integration.
 */
export default defineConfig({
  testDir: "./e2e",
  // a11y runs are short; no need for retries.
  retries: 0,
  // No parallelism so Vite preview only starts once.
  workers: 1,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: "http://127.0.0.1:4173",
    trace: "on-first-retry",
    // Reduce flake on slow renders.
    actionTimeout: 10_000,
    navigationTimeout: 30_000,
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: {
    command: "pnpm preview --port 4173 --strictPort",
    url: "http://127.0.0.1:4173",
    // 90s allows the `prebuild`-triggered token regeneration + Vite
    // build + preview boot inside `pnpm preview` if it has to build
    // first. Once the build is cached the start is ~1s.
    timeout: 90_000,
    reuseExistingServer: !process.env.CI,
  },
});
