# Demo 03 — Code review with time-bounded write grant

Per [ADR-020](../../docs/adrs/020-recorded-demo-program.md) §"Six
scenarios" item 3. The agent gets just enough surface to do a code
review: read the source, run `git diff` to see what changed,
write a review file. The write grant is **time-bounded** — valid
for one hour after session start, after which the same
`check_filesystem_write` returns Deny.

## What this demonstrates

| Layer | Behavior in this demo |
|---|---|
| **F2 Permission Manifest** | Three explicit grants: read `/tmp/aegis-demo-03/repo`, write narrowed to `/tmp/aegis-demo-03/data/review.md` via an explicit `write_grant`, exec `git`. Nothing else — no network, no MCP, no other binaries. |
| **F2 Exec grant** | `exec_grants[].program: /usr/bin/git` is allowed; the path is pinned absolute — `aegis validate` (AEGIS003) refuses bare basenames since any `/git` on PATH would match (security risk). |
| **F7 Time-bounded write grant** | `write_grants[].duration: "PT1H"` (ISO 8601, per ADR-009 + the F7 extension at PR #38). Within the window, `check_filesystem_write(/tmp/aegis-demo-03/data/review.md)` Allows; outside, the same call Denies. |
| **ADR-019 Explicit-precedence** | No broad `tools.filesystem.write` here — the only authorized write path is the explicit `write_grant` for `/tmp/aegis-demo-03/data/review.md`. Closed-by-default refuses everything else. |
| **F4 Access Log** | The agent's write of review.md surfaces as an Access entry. |
| **F9 Hash-chained ledger** | Tamper-evident; `aegis verify` confirms the chain. |

## What you see in the GIF

| t | Frame | Note |
|---|---|---|
| 0–5s | `grep write_grants` + `grep exec_grants` showing the explicit grants | One-hour write window; pinned absolute exec path for git |
| 5–32s | `aegis run --backend litertlm --prompt "..."` runs Gemma 4 E4B | Single-turn inference + write ~28s on a single CPU |
| 32–36s | `grep accessType.:.write` shows F7 within-window write | `accessType: write`, `resourceUri: file:///tmp/aegis-demo-03/data/review.md` |
| 36–40s | `grep entryType.:.access` shows the full Access sequence | One write entry — the explicit-grant flow |
| 40–44s | `aegis verify ledger-demo-03.jsonl` | F9 chain validates |

## Run locally

```bash
make -C demos 03-code-review-time-bounded
```

That's it. The Makefile invokes `setup.sh` which `aegis pull`s
Gemma 4 E4B, mkdirs `/tmp/aegis-demo-03/`, symlinks the model +
chat-template-sidecar, `git init`s a small repo with two commits
(initial `add(a, b)` + saturate-on-overflow patch), and pre-cleans
old ledgers / drafts. Then VHS renders. Idempotent — re-running
the second time skips the `aegis pull` (cache hit), reuses the
existing repo, and just re-renders.

### What `setup.sh` does (in case you want to do it by hand)

```bash
# 1) Pull Gemma 4 E4B (one-time per machine; same blob as Demo 2)
aegis pull \
  ghcr.io/tosin2013/aegis-node-models/gemma-4-e4b-it@sha256:de89d03b650a86410d1c9f48ee2239fdf7d5f8895ad00621e20b9c2ed195f931 \
  --keyless-identity '^https://github\.com/tosin2013/aegis-node/\.github/workflows/models-publish\.yml@.*$' \
  --keyless-oidc-issuer 'https://token.actions.githubusercontent.com'

# 2) Stage the workdir under /tmp (no root-of-FS paths required)
mkdir -p /tmp/aegis-demo-03/data
ln -sf ~/.cache/aegis/models/de89d03b650a86410d1c9f48ee2239fdf7d5f8895ad00621e20b9c2ed195f931/blob.bin \
       /tmp/aegis-demo-03/model.litertlm
ln -sf ~/.cache/aegis/models/de89d03b650a86410d1c9f48ee2239fdf7d5f8895ad00621e20b9c2ed195f931/chat_template.sha256.txt \
       /tmp/aegis-demo-03/chat_template.sha256.txt
ln -sf "$PWD/demos/03-code-review-time-bounded/manifest.yaml" \
       /tmp/aegis-demo-03/manifest.yaml

# Stage the repo (two commits → meaningful diff)
mkdir -p /tmp/aegis-demo-03/repo && cd /tmp/aegis-demo-03/repo
git init -q .
git config user.email demo@aegis-node.local && git config user.name Demo
printf 'pub fn add(a: i32, b: i32) -> i32 { a + b }\n' > lib.rs
git add . && git commit -q -m initial
printf 'pub fn add(a: i32, b: i32) -> i32 {\n    a.checked_add(b).unwrap_or(0)\n}\n' > lib.rs
git add . && git commit -q -m 'saturate on overflow'

# 3) Render
cd "$OLDPWD" && cd demos/03-code-review-time-bounded && vhs demo.tape
```

## Glibc requirement

Same as Demo 2 — the LiteRT-LM runtime needs **glibc 2.38+**. Render
on ubuntu-24.04+ host or in CI's `litertlm.yml` job. Pre-LiteRT
demos (5, 6) using llama.cpp work on older hosts.

## Why a 1-hour duration (and what about post-expiry)

Per ADR-020 §"Six scenarios" item 3: "*the recording includes a
deliberate clock skip showing the post-expiry Deny entry.*" In
practice that requires a `aegis run --now <iso>` flag we don't
have yet — Phase 1 of this demo demonstrates the **within-window**
flow only. The post-expiry frame is a future enhancement that
needs the clock-override CLI surface. The current GIF makes the
in-manifest time-bounding visible (visible `duration: PT1H`) and
shows the agent's write succeeding inside the window; an auditor
reading the manifest knows the same write at session_start + 70min
would Deny.

## Reproducibility

Manifest pins `inference.determinism` (seed 42 + temperature 0).
With the same Gemma 4 E4B blob + the same git repo state, every
render produces byte-identical text output.

## Why Gemma 4 E4B (not Qwen)

Per ADR-020 §"Decision" item 8: code-shaped reasoning benefits
from the larger model. Qwen 1.5B can produce a plausible
"saturate on overflow" line, but Gemma 4 E4B's review covers
edge cases (`i32::MIN`, signed overflow semantics) that read as
meaningful technical feedback rather than auto-completion.
