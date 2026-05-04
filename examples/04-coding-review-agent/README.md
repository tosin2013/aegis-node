# Example 04 — code-review agent with time-bounded write grant

The agent reviews a Go function diff and writes a short code review to
disk under F7 time-bounded enforcement. The write grant carries
`duration: PT1H` — outside the one-hour window the same write is
denied. The deny-by-default network keeps the source from leaving the
host. For the **exec-grant pattern** (`/usr/bin/git` for `git diff`,
etc.), see [demos/03-code-review-time-bounded](../../demos/03-code-review-time-bounded/)
— same shape, with the polished narrative.

## Why this matters in 2026

Coding agents are the **#2 enterprise AI use case** in 2026. *Nubank
reported 12x efficiency and 20x cost savings* migrating millions of
lines of code with a Devin-style agent (per *Sema4.ai*). But the
*OWASP Top 10 for Agentic Applications 2026* lists **"unexpected code
execution"** as a Top-10 risk: agents that can run arbitrary commands
against arbitrary files are a soft target for prompt-injection
attacks that escalate to RCE.

Aegis-Node's F7 time-bounded write grants give a coding agent
*exactly* the writeable surface it needs and nothing more: the agent
can write the review file for one hour, and only that file. The
explicit-grant precedence rule (per ADR-019) means there's no broad
write coverage to fall back on — closed-by-default keeps every other
path denied. The ledger captures every read + the write — so when
something goes sideways, the audit trail says exactly what the agent
read and where it wrote.

Source: *Sema4.ai 10 AI Agent Use Cases Transforming Enterprises in
2026*; *OWASP Top 10 for Agentic Applications 2026*.

## What you'll see

- The model receives a Go-function diff in the prompt
- The agent emits `filesystem__write` to the review file
- The mediator allows the write (covered by the time-bounded grant)
- The review at `output/code-review.md` covers the diff
- Ledger shows F4 entry for the write + session boundaries

## Run it

```bash
bash setup.sh
cd /tmp/aegis-example-04
aegis run --manifest manifest.yaml --model model.gguf \
    --workload code-reviewer --instance inst-001 \
    --prompt "$(cat prompt.txt)"
cat output/code-review.md
aegis verify ledger-*.jsonl
```

## What just happened

| F-feature | What enforced it | ADR |
|---|---|---|
| **F2 Permission Manifest** | Reads gated by `tools.filesystem.read`; the explicit `write_grants` block is the only path that authorizes a write | [ADR-009](../../docs/adrs/009-permission-manifest-format.md) |
| **F7 Time-Bounded Write Grant** | Write to `code-review.md` allowed for 1 hour after session start; denied outside the window | [ADR-019](../../docs/adrs/019-explicit-write-grant-takes-precedence.md) |
| **F4 Access Log** | Every read and write entry in the ledger names the resource + bytes | [ADR-010](../../docs/adrs/010-trajectory-ledger-format.md) |
| **F1 IdentityRebind** | If `model.gguf` is tampered mid-session, the per-tool-call rebind fires + halts the next dispatch (see Demo 5 for the live race) | [ADR-003](../../docs/adrs/003-cryptographic-workload-identity-spiffe-spire.md) |
| **F9 Hash-Chained Ledger** | Every entry hash-chains; `aegis verify` confirms tamper-evident audit | [ADR-010](../../docs/adrs/010-trajectory-ledger-format.md) |

## Inspect the artifacts

```bash
# The work product
cat output/code-review.md

# The review write itself
grep 'accessType.:.write' ledger-*.jsonl | jq -c '{resourceUri, bytesAccessed}'

# Chain integrity
aegis verify ledger-*.jsonl
```

## Make it yours

- **Bring your own diff** — replace the Go diff in `prompt.txt` with
  any code change you want reviewed. The agent writes a fresh review.
- **Tighten the time window** — change `duration: "PT1H"` to `"PT5M"`
  and re-run after 6 minutes; the same write now denies. Watch the
  ledger capture the time-bound denial.
- **Add an exec_grant for git** — extend `manifest.yaml` with
  `exec_grants: [{program: "/usr/bin/git"}]` and prompt the agent to
  call `exec__run` with `/usr/bin/git` so it can inspect a real
  repository's diff. This is what
  [demos/03-code-review-time-bounded](../../demos/03-code-review-time-bounded/)
  does — the demo's manifest is a useful next-step reference.
- **Switch to a coding-tuned model** — `aegis pull` a code-specialty
  GGUF (e.g. a Qwen Coder variant) and point `--model` at it. The
  manifest stays the same; the review quality changes.
- **Add a write_grant for a patch file** — extend the agent to also
  emit a unified diff to `output/fix.patch`. Bound the grant to
  `PT15M` so post-session writes deny.

## What you should see when you tighten the manifest

Remove the `write_grants` block entirely. Re-run; the agent's
`filesystem__write` call lands as an F2 violation in the ledger
naming `output/code-review.md`; no review is produced; `aegis verify`
still passes (the violation is itself in the chain). Restore
`write_grants` and instead change the `resource:` path to a
different file (e.g. `output/different.md`); the agent's attempt to
write `code-review.md` now fails because the explicit grant doesn't
cover that path. Lesson: explicit grants are precise — they bind to
the exact resource path, not a directory prefix.
