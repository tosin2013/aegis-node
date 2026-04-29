# Replay viewer DOM snapshot test

Per **F8-C / ADR-010 / issue [#64](https://github.com/tosin2013/aegis-node/issues/64)**.

Headless DOM regression check that pins the viewer's rendering of
`fixtures/sample-session.jsonl` against a committed snapshot
(`expected.json`). If a viewer or fixture change drifts the audit
experience, this test catches it at PR time.

## Why jsdom (and not Playwright)

The viewer is small, vanilla JS, uses no browser-specific APIs we care
about beyond standard DOM + Web Crypto. Playwright would download
~300 MB of Chromium per CI run for negligible coverage gain. jsdom is
~9 MB, runs the test in ~1 s, and pins exactly the runtime we need.

The trade-off: jsdom isn't a real browser, so this test won't catch
Chrome/Firefox/Safari rendering quirks. **F8-A's air-gap CI guard**
(`tools/replay-viewer/check-airgap.sh`) is what enforces the "works in
any browser, no network" property; this test enforces "the rendered
structure matches what we committed."

## Run locally

```bash
cd tools/replay-viewer/snapshot
npm ci
npm test                  # check mode (CI uses this)
npm run regenerate        # rewrite expected.json after intentional changes
```

`npm test` exits 0 if the rendered DOM summary matches `expected.json`,
non-zero with an inline diff hint otherwise.

## What's pinned

The snapshot captures structural invariants only — not whitespace, CSS,
or pixel rendering. That keeps it reviewable + diff-friendly:

- **integrity banner** — state (`verified` / `indeterminate` / `broken`)
  and human-readable label
- **summary chips** — file name, session id, entry count, root hash
- **per-entry** — sequence number, type, type-pill text, timestamp,
  every key/value row in the body

A future viewer change that adds a new field to a card flips the test
red until you re-run `npm run regenerate` to acknowledge the drift.
That's exactly the gate F8-C is meant to provide.

## When this fails on a PR

1. Read the inline diff the runner prints.
2. Decide: intentional drift (new field, fixed wording) vs regression.
3. If intentional, run `npm run regenerate` and commit the updated
   `expected.json` in the same PR.
4. If a regression, fix the viewer and rerun.
