#!/bin/sh
# Example 02 setup — stage /tmp/aegis-example-02/ for the research-
# assistant example.
#
# Modes:
#   default   — filesystem MCP only. Works offline. The agent reads
#               fixtures/docs/ and writes output/research-summary.md.
#   firecrawl — filesystem MCP + Firecrawl MCP for live web research.
#               Triggered automatically by setting FIRECRAWL_API_KEY:
#                   export FIRECRAWL_API_KEY=fc-...
#                   bash setup.sh
#
# Idempotent; safe to re-run.

set -eu

WORKDIR=/tmp/aegis-example-02
QWEN_REF='ghcr.io/tosin2013/aegis-node-models/qwen2.5-1.5b-instruct-q4_k_m@sha256:c7404a910e65596a185e788ede19e09bc017dc3101cd106ba7d65fe1dd7dec37'
QWEN_MANIFEST_SHA='c7404a910e65596a185e788ede19e09bc017dc3101cd106ba7d65fe1dd7dec37'
KEYLESS_IDENTITY='^https://github\.com/tosin2013/aegis-node/\.github/workflows/models-publish\.yml@.*$'
KEYLESS_OIDC_ISSUER='https://token.actions.githubusercontent.com'

require_bin() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "ERROR: $1 not found on PATH" >&2
        if [ -n "${2:-}" ]; then echo "  install hint: $2" >&2; fi
        exit 2
    }
}

require_bin aegis 'cargo install --locked --path crates/cli --features llama'
require_bin oras 'https://github.com/oras-project/oras/releases'
require_bin cosign 'https://github.com/sigstore/cosign/releases'
require_bin mcp-server-filesystem 'npm install -g @modelcontextprotocol/server-filesystem'

echo '[02] aegis pull Qwen 2.5 1.5B Q4_K_M (cosign-verified)...'
aegis pull "$QWEN_REF" \
    --keyless-identity "$KEYLESS_IDENTITY" \
    --keyless-oidc-issuer "$KEYLESS_OIDC_ISSUER" >/dev/null

CACHED="$HOME/.cache/aegis/models/$QWEN_MANIFEST_SHA"
if [ ! -f "$CACHED/blob.bin" ]; then
    echo "ERROR: $CACHED/blob.bin missing after aegis pull" >&2
    exit 2
fi

echo "[02] staging $WORKDIR/..."
mkdir -p "$WORKDIR/fixtures/docs" "$WORKDIR/output"
ln -sf "$CACHED/blob.bin"                 "$WORKDIR/model.gguf"
ln -sf "$CACHED/chat_template.sha256.txt" "$WORKDIR/chat_template.sha256.txt"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ln -sf "$SCRIPT_DIR/prompt.txt" "$WORKDIR/prompt.txt"
# Copy (not symlink) the fixture .md files. mcp-server-filesystem
# refuses to follow symlinks whose targets resolve outside the
# allowed-directory it was launched with — copy keeps the canonical
# path inside the workdir.
for doc in "$SCRIPT_DIR"/fixtures/docs/*.md; do
    cp -f "$doc" "$WORKDIR/fixtures/docs/$(basename "$doc")"
done

# Mode selection: Firecrawl extended if FIRECRAWL_API_KEY is set.
if [ -n "${FIRECRAWL_API_KEY:-}" ]; then
    echo '[02] FIRECRAWL_API_KEY detected — wiring extended mode (filesystem + Firecrawl MCP)'
    require_bin npx 'install Node.js + npm'
    # Note: we intentionally do NOT pre-run firecrawl-mcp here.
    # `npx -y firecrawl-mcp --help` doesn't always exit cleanly (the
    # MCP server may start listening on stdin instead of printing help),
    # which hangs setup.sh. The first `aegis run` will pay the npx
    # install latency once; subsequent runs hit npm's cache.
    ln -sf "$SCRIPT_DIR/manifest.firecrawl.yaml" "$WORKDIR/manifest.yaml"
    MODE='extended (filesystem + Firecrawl)'
else
    echo '[02] default mode — filesystem MCP only (set FIRECRAWL_API_KEY for web research)'
    ln -sf "$SCRIPT_DIR/manifest.default.yaml" "$WORKDIR/manifest.yaml"
    MODE='default (filesystem only)'
fi

rm -f "$WORKDIR"/ledger-*.jsonl "$WORKDIR/output/research-summary.md"

cat <<EOF
[02] ready in mode: $MODE

  cd $WORKDIR
  aegis run --manifest manifest.yaml \\
      --model model.gguf \\
      --workload research-assistant --instance inst-001 \\
      --prompt "\$(cat prompt.txt)"
  cat output/research-summary.md
  aegis verify ledger-*.jsonl
EOF
