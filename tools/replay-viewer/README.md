# Aegis-Node Replay Viewer

Offline single-file HTML tool for stepping through a Trajectory Ledger
(`.jsonl`). Per **F8 / ADR-010 / issue [#62](https://github.com/tosin2013/aegis-node/issues/62)**.

## Usage

```bash
# 1. Open the viewer in any modern browser. No build step.
xdg-open tools/replay-viewer/index.html        # linux
open tools/replay-viewer/index.html            # macos

# 2. Click "Ledger file:" and pick a .jsonl produced by `aegis run`.
```

That's it. Everything happens in the browser — no fetch, no CDN, no
backend.

## Hard rules (per ADR-010)

These are enforced by the CI guard at
`.github/workflows/replay-viewer.yml` — a regression that violates them
fails the workflow on every PR:

- No `<script src=...>`, no `<link href=...>` outside this file
- No `fetch()`, no `XMLHttpRequest`
- No remote-loaded `<img>` / `<iframe>` / `<audio>` / `<video>`
- All assets inlined; works in air-gapped Chrome / Firefox / Safari

The viewer is one file (`index.html`) with HTML, CSS, and JS in the
same blob. An auditor in a SCIF can drop the file on a USB stick and
open it on a network-locked workstation.

## What it shows

| Entry type            | Color  | Fields rendered |
|-----------------------|--------|-----------------|
| `session_start`       | blue   | spiffeId, model/manifest/config digest |
| `reasoning_step` (F5) | purple | step id, tool selected, tools considered, input + reasoning text |
| `access` (F4)         | green  | resource URI, type, bytes, linked reasoning step id |
| `violation`           | red    | reason, resource URI, access type |
| `approval_request` (F3) | yellow | summary, resource URI, expiry, reasoning step id |
| `approval_granted`    | green  | approver SPIFFE id, decision time |
| `approval_rejected` / `_timed_out` | red / yellow | reason, decision time |
| `network_attestation` (F6) | cyan | totals, connections digest, HMAC signature |
| `session_end`         | blue   | spiffeId |
| _unknown_             | gray   | raw JSON payload (forward-compatible) |

Hover an `access` entry that names a `reasoningStepId` — its linked
reasoning_step card highlights. Visualizes "why did the agent do that"
without scrolling.

## Test fixture

`fixtures/sample-session.jsonl` is a hand-crafted ledger covering every
entry type. Useful for eyeballing the viewer; **the chain hashes are
not real**. F8-B (#63) will replace this with a fixture written by the
runtime so the chain verification banner can be exercised.

## What's not here yet

- **F8-B (#63) — chain verification.** Today the integrity banner
  always reads "unknown." The browser will recompute SHA-256 over each
  entry's canonical bytes once that ships.
- **F8-C (#64) — CI snapshot test.** Headless-browser regression check
  that pins a fixture's DOM tree to a checked-in expected snapshot.
