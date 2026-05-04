#!/bin/sh
# Example 06 setup — stage /tmp/aegis-example-06/ for the finance/ops
# expense-audit example. Builds a fresh SQLite DB from seed.sql.
# Idempotent.

set -eu

WORKDIR=/tmp/aegis-example-06
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

require_bin aegis 'cargo install --path crates/cli --features llama'
require_bin oras 'https://github.com/oras-project/oras/releases'
require_bin cosign 'https://github.com/sigstore/cosign/releases'
require_bin sqlite3 'apt-get install sqlite3 (for seed loading)'
require_bin uvx 'pipx install uv (provides uvx for the SQLite MCP server)'

echo '[06] aegis pull Qwen 2.5 1.5B Q4_K_M (cosign-verified)...'
aegis pull "$QWEN_REF" \
    --keyless-identity "$KEYLESS_IDENTITY" \
    --keyless-oidc-issuer "$KEYLESS_OIDC_ISSUER" >/dev/null

CACHED="$HOME/.cache/aegis/models/$QWEN_MANIFEST_SHA"
if [ ! -f "$CACHED/blob.bin" ]; then
    echo "ERROR: $CACHED/blob.bin missing after aegis pull" >&2
    exit 2
fi

echo "[06] staging $WORKDIR/..."
mkdir -p "$WORKDIR/output"
ln -sf "$CACHED/blob.bin"                 "$WORKDIR/model.gguf"
ln -sf "$CACHED/chat_template.sha256.txt" "$WORKDIR/chat_template.sha256.txt"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ln -sf "$SCRIPT_DIR/manifest.yaml"  "$WORKDIR/manifest.yaml"
ln -sf "$SCRIPT_DIR/prompt.txt"     "$WORKDIR/prompt.txt"
ln -sf "$SCRIPT_DIR/approval.json"  "$WORKDIR/approval.json"

# Build the expense DB from seed.sql.
echo '[06] building expenses.db from seed.sql...'
rm -f "$WORKDIR/expenses.db"
sqlite3 "$WORKDIR/expenses.db" < "$SCRIPT_DIR/seed.sql"

# Pre-warm the MCP SQLite server so the first agent run doesn't pay
# the uvx download latency mid-session.
echo '[06] pre-warming uvx mcp-server-sqlite cache...'
uvx mcp-server-sqlite --help >/dev/null 2>&1 || true

rm -f "$WORKDIR"/ledger-*.jsonl "$WORKDIR/output/q2-expense-anomalies.md"

cat <<EOF
[06] ready. Run the agent (note the AEGIS_APPROVAL_FILE env var):

  cd $WORKDIR
  AEGIS_APPROVAL_FILE=$WORKDIR/approval.json \\
  aegis run --manifest manifest.yaml \\
      --model model.gguf \\
      --workload finance-auditor --instance inst-001 \\
      --prompt "\$(cat prompt.txt)"

Then inspect the artifact + audit trail:

  cat output/q2-expense-anomalies.md
  grep mcp__sqlite ledger-*.jsonl | jq -c '{accessType, resourceUri}'
  grep approval ledger-*.jsonl | jq .
  aegis verify ledger-*.jsonl
EOF
