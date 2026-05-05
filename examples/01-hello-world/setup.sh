#!/bin/sh
# Example 01 setup — stage /tmp/aegis-example-01/ for the hello-world
# example. Idempotent; safe to re-run.
#
# Uses Gemma 4 E4B via the LiteRT-LM backend (per ADR-023). Gemma 4
# produces coherent prose where Qwen 1.5B would emit templated text;
# this matters from the very first example a contributor runs.
# Inherits the LiteRT-LM upstream CPU sampler blocker tracked at #119
# — see this example's README "Known limitation" section.

set -eu

WORKDIR=/tmp/aegis-example-01
GEMMA_E4B_REF='ghcr.io/tosin2013/aegis-node-models/gemma-4-e4b-it@sha256:de89d03b650a86410d1c9f48ee2239fdf7d5f8895ad00621e20b9c2ed195f931'
# aegis pull keys its cache by the OCI manifest digest, not the blob hash.
GEMMA_E4B_MANIFEST_SHA='de89d03b650a86410d1c9f48ee2239fdf7d5f8895ad00621e20b9c2ed195f931'
KEYLESS_IDENTITY='^https://github\.com/tosin2013/aegis-node/\.github/workflows/models-publish\.yml@.*$'
KEYLESS_OIDC_ISSUER='https://token.actions.githubusercontent.com'

require_bin() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "ERROR: $1 not found on PATH" >&2
        if [ -n "${2:-}" ]; then echo "  install hint: $2" >&2; fi
        exit 2
    }
}

require_bin aegis 'cargo install --locked --path crates/cli --features "llama litertlm"'
require_bin oras 'https://github.com/oras-project/oras/releases'
require_bin cosign 'https://github.com/sigstore/cosign/releases'

aegis run --help 2>&1 | grep -q -- '--backend' || {
    echo 'ERROR: aegis on PATH was built without --backend support.' >&2
    echo '  rebuild: cargo install --locked --path crates/cli --features "llama litertlm" --force' >&2
    exit 2
}

echo '[01] aegis pull Gemma 4 E4B (cosign-verified)...'
aegis pull "$GEMMA_E4B_REF" \
    --keyless-identity "$KEYLESS_IDENTITY" \
    --keyless-oidc-issuer "$KEYLESS_OIDC_ISSUER" >/dev/null

CACHED="$HOME/.cache/aegis/models/$GEMMA_E4B_MANIFEST_SHA"
if [ ! -f "$CACHED/blob.bin" ]; then
    echo "ERROR: $CACHED/blob.bin missing after aegis pull" >&2
    exit 2
fi

echo "[01] staging $WORKDIR/..."
mkdir -p "$WORKDIR/output"
ln -sf "$CACHED/blob.bin"                 "$WORKDIR/model.litertlm"
ln -sf "$CACHED/chat_template.sha256.txt" "$WORKDIR/chat_template.sha256.txt"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ln -sf "$SCRIPT_DIR/manifest.yaml" "$WORKDIR/manifest.yaml"
ln -sf "$SCRIPT_DIR/prompt.txt"    "$WORKDIR/prompt.txt"

rm -f "$WORKDIR"/ledger-*.jsonl "$WORKDIR/output/greeting.txt"

cat <<EOF
[01] ready. Run the agent:

  cd $WORKDIR
  aegis run --backend litertlm \\
      --manifest manifest.yaml \\
      --model model.litertlm \\
      --chat-template-sidecar chat_template.sha256.txt \\
      --workload hello-world --instance inst-001 \\
      --prompt "\$(cat prompt.txt)"

Then inspect the artifact + ledger:

  cat output/greeting.txt
  aegis verify ledger-*.jsonl
EOF
