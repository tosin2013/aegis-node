#!/bin/sh
# Demo 06 setup — stage everything `demo.tape` expects into
# /tmp/aegis-demo-06/. Idempotent; safe to re-run. Invoked
# automatically by `make 06-egress-containment` (per
# demos/Makefile's `%/demo.gif` rule).
#
# Per ADR-020 + ADR-008. Uses the llama backend + Qwen2.5-1.5B-
# Instruct Q4_K_M.

set -eu

WORKDIR=/tmp/aegis-demo-06
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

aegis run --help 2>&1 | grep -q -- '--prompt' || {
    echo 'ERROR: aegis on PATH was built without the llama feature.' >&2
    echo '  rebuild: cargo install --path crates/cli --features llama --force' >&2
    exit 2
}

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
mkdir -p "$WORKDIR"
ln -sf "$CACHED/blob.bin"                 "$WORKDIR/model.gguf"
ln -sf "$CACHED/chat_template.sha256.txt" "$WORKDIR/chat_template.sha256.txt"

# Symlink the manifest into the workdir so `demo.tape` can use a
# workdir-local path (no checkout-prefix dependency).
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ln -sf "$SCRIPT_DIR/manifest.yaml" "$WORKDIR/manifest.yaml"

rm -f "$WORKDIR"/ledger-*.jsonl

echo '[06] ready: cd '"$WORKDIR"' && vhs '"$(dirname "$0")"'/demo.tape'
