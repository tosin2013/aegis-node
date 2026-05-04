# Aegis-Node examples

Six hand-runnable, fork-friendly examples that walk from "can I run an
agent at all?" to "production-shape finance audit with SQL + human
approval." Each example produces a tangible artifact — a refund letter,
a code review, a research summary, an attestation report — so you have
both the *audit trail* (the ledger) and the *work product* (the file
the agent wrote) when you're done.

## Why these examples

Three things 2026 is asking enterprise AI agents to answer, and what
Aegis-Node actually does about each:

| 2026 ask | Source | Aegis answer | Example |
|---|---|---|---|
| *"What did this agent do at 3pm last Tuesday?"* | *VentureBeat: 88% of enterprises had agent security incidents in past 12 months; 21% have runtime visibility* (Apr 2026) | F4 + F5 + F6 + F9 — every tool call in a hash-chained ledger; signed end-of-session network attestation | All; emphasized in 05 |
| *"Can the agent be poisoned through its tool layer?"* | *OWASP Top 10 for Agentic Applications 2026* — agentic supply chain (incl. ClawHub MCP-skill registry incident, Q1 2026) | `tools.mcp[].allowed_tools` + ADR-024 `pre_validate` — protocol allowlist plus per-arg side-effect mapping | 02, 06 |
| *"How do we prove a human approved this?"* | *EU AI Act* high-risk requirements (Aug 2 2026); *Colorado AI Act* (Jun 30 2026); *CMMC 2.0* (Nov 2 2026) | F3 approval gate — file / TTY / web / mTLS+SPIFFE channels; chain-of-evidence ledger entry naming approver + reason | 03, 06 |

## Prerequisites

### Toolchain (one-time per machine)

Two ways to get a working toolchain. The Docker path is fastest if
you just want to run the examples; the native path is what you want
if you're going to develop on the codebase.

#### Docker — fastest, no host toolchain install

```bash
git clone https://github.com/tosin2013/aegis-node.git && cd aegis-node
docker run --rm -it \
    -v "$PWD:/workspaces/aegis-node" \
    -w /workspaces/aegis-node \
    ghcr.io/tosin2013/aegis-node-devbox:latest \
    bash

# inside the container — Rust, Go, oras, cosign, jq, node all pre-installed:
cargo install --locked --path crates/cli --features llama
aegis identity init --trust-domain aegis-node.local
cd examples/01-hello-world && bash setup.sh
# follow the example's README from here
```

The image is `.devcontainer/Dockerfile` published from main, signed
with cosign keyless. Same image VS Code uses for "Reopen in Container."

#### Native — `mise` toolchain manager

The canonical native path uses [`mise`](https://mise.jdx.dev/) (per
[ADR-017](../docs/adrs/017-local-development-environment-devcontainer-mise.md))
to pin Rust / Go / cosign / Node to the versions the project tests
against:

```bash
# 0. Install mise itself (skip if already installed)
curl https://mise.run | sh

# 1. Activate mise in your current shell (mise install puts tools in
#    ~/.local/share/mise but they aren't on PATH until activation).
#    Persist by adding this line to ~/.bashrc (or ~/.zshrc) too.
eval "$(~/.local/bin/mise activate bash)"                 # use 'zsh' if on zsh

# 2. Install the pinned toolchain (Rust 1.85, Go 1.23, cosign 2.4.1, node 20)
cd /path/to/aegis-node
mise install

# 3. Build + install aegis to ~/.cargo/bin (puts it on PATH; enables --prompt)
cargo install --locked --path crates/cli --features llama

# 4. Bootstrap the local CA (one-time)
aegis identity init --trust-domain aegis-node.local
```

If you skip step 1 you'll see *"cargo: command not found"* even after
`mise install` succeeds — mise installed the tools, but your shell
doesn't know where they are until activated.

`make build` alone is not enough — it builds to `target/debug/aegis`
without the `llama` feature, so `aegis run --prompt …` won't work.
The `cargo install` step above is what the demos and examples expect.

**Without mise** (if you'd rather use rustup directly): install
[`rustup`](https://rustup.rs) + [`go 1.23+`](https://go.dev/dl/) via
your usual channel, then run steps 2–3 above. You skip the version
pinning, but the examples don't depend on exact versions.

### Extra binaries on PATH

Not pinned by `mise.toml`; install via your OS package manager:

```bash
# Linux (Debian/Ubuntu)
apt-get install -y jq sqlite3 git
# oras: https://github.com/oras-project/oras/releases (no apt package; download the tarball)
```

| Binary | Used by | Install hint |
|---|---|---|
| `oras` | All (`aegis pull`) | https://github.com/oras-project/oras/releases |
| `cosign` | All (`aegis pull` keyless verify) | provided by `mise install` |
| `jq` | All (inspect ledger) | `apt-get install jq` |
| `git` | Example 04 | `apt-get install git` |
| `sqlite3` | Example 06 (seed loading) | `apt-get install sqlite3` |
| `mcp-server-filesystem` | Example 02 default mode | `npm install -g @modelcontextprotocol/server-filesystem` |
| `npx` | Example 02 extended mode | comes with Node.js (provided by `mise install`) |
| `uvx` | Example 06 (SQLite MCP server) | `pipx install uv` or `curl -LsSf https://astral.sh/uv/install.sh \| sh` |

Plus environment variables for opt-in modes:
- `FIRECRAWL_API_KEY` — Example 02 extended (live web research)

## The 6-example arc

| # | Example | 2026 use case | OWASP risk countered | F-features | Output artifact |
|---|---|---|---|---|---|
| [01](01-hello-world/) | hello-world | Onboarding | (foundation) | F1, F9 | `output/greeting.txt` |
| [02](02-mcp-research-assistant/) | research assistant | Knowledge work; **MCP supply chain** (ClawHub) | Tool misuse, agentic supply chain | F2, F2-MCP, F4, F6, F9 | `output/research-summary.md` |
| [03](03-customer-support-refund/) | customer support refund | **#1**: customer support | Human-agent trust exploitation | F2, F3, F4, F9 | `output/refund-letter.md` |
| [04](04-coding-review-agent/) | code review agent | **#2**: coding agents | Unexpected code execution, identity abuse | F2, F4, F7, F9 | `output/code-review.md` |
| [05](05-egress-audit-trail/) | egress containment + attestation | **Governance #1**: "what did the agent do at 3pm?" | Goal hijack via exfiltration | F2, F6, F9 | `output/network-attestation.json` + `output/session-report.md` |
| [06](06-mcp-finance-sqlite/) | finance/ops expense audit | **#3**: finance/ops | Tool misuse (SQL-injection), human-agent trust | F2, F2-MCP, F3, F4, F9 | `output/q2-expense-anomalies.md` |

Two examples (02, 06) demonstrate external MCP servers — filesystem
and SQLite. Example 02 also has an opt-in **Firecrawl mode** (set
`FIRECRAWL_API_KEY`) that adds live web research as a second MCP
server — the canonical template for any API-key-required external
MCP server (Slack, GitHub, Postgres, Anthropic API, etc.).

## Quick start

```bash
cd examples/01-hello-world
bash setup.sh
cd /tmp/aegis-example-01
aegis run --manifest manifest.yaml --model model.gguf \
    --workload hello-world --instance inst-001 \
    --prompt "$(cat prompt.txt)"
cat output/greeting.txt           # the agent's work product
aegis verify ledger-*.jsonl       # the audit trail
```

About 5 minutes from `git clone` to a passing verify on a fresh
machine (the model pull dominates first run; subsequent examples
hit the cache).

## Where to go next

- **Watch the polished narratives**: [demos/](../demos/) has 6 VHS-
  recorded GIFs of these scenarios. Examples are runnable; demos are
  watchable. They cross-reference each other.
- **Read the design**: [docs/adrs/](../docs/adrs/) — 24 architectural
  decision records covering F1–F10, supply chain, MCP adoption, and
  ADR-024 MCP arg pre-validation.
- **Look at the manifest schema**: [schemas/manifest/v1/](../schemas/manifest/v1/)
  — the JSON Schema for the Permission Manifest, plus four canonical
  examples (`read-only-research`, `single-write-target`, `agent-with-mcp`,
  `agent-with-exec`).

## Contribute a new example

We want more examples. The bar is low: **if it produces a tangible
work-product artifact and `aegis verify` passes, we want it.** The
minimum recipe:

```bash
cp -r examples/02-mcp-research-assistant examples/0N-your-use-case
cd examples/0N-your-use-case
# Edit manifest.yaml — your tools, your paths
# Edit prompt.txt — your scenario
# Edit setup.sh — your fixtures (and update WORKDIR=/tmp/aegis-example-0N)
# Edit README.md — your "Why this matters in 2026" + which OWASP risk it counters
bash setup.sh && cd /tmp/aegis-example-0N
aegis run --manifest manifest.yaml --model model.gguf --prompt "$(cat prompt.txt)"
ls output/                # confirm your example produces an artifact
aegis verify ledger-*.jsonl   # confirm the chain
```

Open a PR titled `examples: 0N <kebab-case-summary>`. Reviewers will
check: (a) the example produces the artifact you describe; (b) the
manifest is closed-by-default + minimal-permission; (c) the README
maps to a real 2026 use case + at least one OWASP risk; (d) the
example doesn't depend on private credentials (use opt-in env vars
for any API key, like Example 02's `FIRECRAWL_API_KEY` pattern).

We don't auto-merge — examples are documentation and we treat them
that way. Expect a few rounds of feedback.

## Ideas for new examples

Concrete next-up suggestions, ranked by 2026 relevance:

- **GitHub MCP coding agent** — read PRs/issues, post review comments.
  Artifact: `output/review.md`. OWASP: tool misuse, identity abuse.
- **Postgres MCP for production-shape DBs** — drop-in replacement for
  Example 06's SQLite. Artifact: same anomaly report shape.
- **Memory MCP for multi-session agents** — directly addresses OWASP
  "memory poisoning" Top-10 risk. Artifact: `output/memory-summary.md`
  + the F4 access log per memory key.
- **Fetch MCP web-research agent** — `tools.network.outbound:
  allowed:` list of trusted domains. Artifact: `output/findings.md`
  with citations.
- **Multi-agent orchestration** — agent A reviews agent B's plan via
  MCP. Artifact: combined `output/plan-review.md`.
- **Healthcare case-summary** — F4 access log + redaction. Artifact:
  `output/case-summary.md`.
- **DevOps log-summarization** — read-only journalctl + F6 deny-egress.
  Artifact: `output/incident-summary.md`.
- **EU AI Act high-risk classification** — agent self-assesses an
  agentic system against the EU AI Act high-risk criteria. Useful
  before the 2026-08-02 deadline. Artifact:
  `output/ai-act-classification.md`.
- **Voice agent stub** — text-only smoke test of the contact-center
  use case. Full voice lands when streaming inference is in the
  runtime.

## Troubleshooting

- **`aegis pull` fails with cosign verification error**: the
  ghcr.io repo + the publish workflow's keyless OIDC chain are
  required. If you're behind a proxy or running offline, mirror the
  models per [docs/MODEL_MIRRORING.md](../docs/MODEL_MIRRORING.md).
- **`mcp-server-filesystem: command not found`**: install via
  `npm install -g @modelcontextprotocol/server-filesystem`.
- **`uvx: command not found`**: install `uv` (`pipx install uv` or
  `curl -LsSf https://astral.sh/uv/install.sh | sh`); `uvx` is part
  of `uv`.
- **`aegis identity init` error about CA path**: the local CA goes in
  `~/.aegis/identity/`. Ensure the directory is writable.
- **Model first-run takes ages**: the Qwen 2.5 1.5B GGUF is ~1 GB.
  Subsequent examples hit the cache (`~/.cache/aegis/models/`).
- **MCP server won't start**: check that the path in
  `tools.mcp[].server_uri` (the part after `stdio:`) actually exists
  and is executable. The `setup.sh` for each MCP example handles
  install + smoke check.
- **F3 approval not picked up**: ensure the `AEGIS_APPROVAL_FILE`
  env var points to the absolute path of `approval.json`, and the
  JSON has `"decision": "granted"`.
