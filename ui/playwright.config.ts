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
    // `pnpm exec vite preview` (not `pnpm preview`) invokes the vite
    // binary directly without going through the `preview` npm script.
    // The script wrapper made vite's stdout invisible to Playwright's
    // webServer probe on GitHub Actions, leading to a 90s timeout
    // with no diagnostic output.
    //
    // `--host 127.0.0.1` pins the bind address. By default Vite's
    // preview server binds to localhost on whichever address family
    // resolves first — that can land on `::1` while Playwright probes
    // `127.0.0.1`. Explicit pinning avoids the resolution race.
    //
    // `--strictPort` makes vite exit (visibly) if 4173 is busy rather
    // than silently falling through to the next port that Playwright
    // can't reach.
    command:
      "pnpm exec vite preview --port 4173 --strictPort --host 127.0.0.1",
    // `port` makes Playwright probe both IPv4 and IPv6 loopback on the
    // chosen port — more forgiving than a pinned `url`.
    port: 4173,
    // 120s headroom for cold-start CI runners. Local boots in ~1s.
    timeout: 120_000,
    reuseExistingServer: !process.env.CI,
    // Surface vite preview's stdout/stderr in the test run output so
    // the next CI failure (if any) shows what the server actually did.
    stdout: "pipe",
    stderr: "pipe",
  },
});
