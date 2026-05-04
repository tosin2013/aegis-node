#!/bin/sh
# Example 05 setup — stage /tmp/aegis-example-05/ for the egress-
# audit-trail example. Idempotent.

set -eu

WORKDIR=/tmp/aegis-example-05
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
require_bin jq ''

echo '[05] aegis pull Qwen 2.5 1.5B Q4_K_M (cosign-verified)...'
aegis pull "$QWEN_REF" \
    --keyless-identity "$KEYLESS_IDENTITY" \
    --keyless-oidc-issuer "$KEYLESS_OIDC_ISSUER" >/dev/null

CACHED="$HOME/.cache/aegis/models/$QWEN_MANIFEST_SHA"
if [ ! -f "$CACHED/blob.bin" ]; then
    echo "ERROR: $CACHED/blob.bin missing after aegis pull" >&2
    exit 2
fi

echo "[05] staging $WORKDIR/..."
mkdir -p "$WORKDIR/output"
ln -sf "$CACHED/blob.bin"                 "$WORKDIR/model.gguf"
ln -sf "$CACHED/chat_template.sha256.txt" "$WORKDIR/chat_template.sha256.txt"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ln -sf "$SCRIPT_DIR/manifest.yaml"           "$WORKDIR/manifest.yaml"
ln -sf "$SCRIPT_DIR/prompt.txt"              "$WORKDIR/prompt.txt"
ln -sf "$SCRIPT_DIR/extract-attestation.sh"  "$WORKDIR/extract-attestation.sh"

rm -f "$WORKDIR"/ledger-*.jsonl "$WORKDIR/output/network-attestation.json" "$WORKDIR/output/session-report.md"

cat <<EOF
[05] ready. Run the agent + extract the attestation:

  cd $WORKDIR
  aegis run --manifest manifest.yaml \\
      --model model.gguf \\
      --workload egress-audit --instance inst-001 \\
      --prompt "\$(cat prompt.txt)"
  bash extract-attestation.sh
  cat output/session-report.md
  aegis verify ledger-*.jsonl
EOF
