#!/bin/sh
# Demo 01 setup — stage everything `demo.tape` expects into
# /tmp/aegis-demo-01/. Idempotent; safe to re-run. Invoked
# automatically by `make 01-mcp-sandboxed-twice` (per
# demos/Makefile's `%/demo.gif` rule).
#
# Per ADR-020 + ADR-024. Uses the llama backend + Qwen2.5-1.5B-
# Instruct Q4_K_M; staged paths are workdir-local (no /data — all
# under /tmp/aegis-demo-01/).

set -eu

WORKDIR=/tmp/aegis-demo-01
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
require_bin mcp-server-filesystem 'npm install -g @modelcontextprotocol/server-filesystem'

aegis run --help 2>&1 | grep -q -- '--prompt' || {
    echo 'ERROR: aegis on PATH was built without the llama feature.' >&2
    echo '  rebuild: cargo install --path crates/cli --features llama --force' >&2
    exit 2
}

# 1) Pull the cosign-verified Qwen 2.5 GGUF (one-time per machine;
#    oras cache-hits subsequent runs).
echo '[01] aegis pull Qwen 2.5 1.5B Q4_K_M (cosign-verified)...'
aegis pull "$QWEN_REF" \
    --keyless-identity "$KEYLESS_IDENTITY" \
    --keyless-oidc-issuer "$KEYLESS_OIDC_ISSUER" >/dev/null

CACHED="$HOME/.cache/aegis/models/$QWEN_MANIFEST_SHA"
if [ ! -f "$CACHED/blob.bin" ]; then
    echo "ERROR: $CACHED/blob.bin missing after aegis pull" >&2
    exit 2
fi

# 2) Stage the workdir. Symlinks (not copies) so a later re-render
#    doesn't duplicate the GGUF on disk.
echo "[01] staging $WORKDIR/..."
mkdir -p "$WORKDIR/data"
ln -sf "$CACHED/blob.bin"                 "$WORKDIR/model.gguf"
ln -sf "$CACHED/chat_template.sha256.txt" "$WORKDIR/chat_template.sha256.txt"

# 3) The research note the agent reads in Call 1. Fixed content for
#    deterministic re-renders.
cat > "$WORKDIR/data/research-notes.txt" <<'EOF'
research note about quarterly results
EOF

# 4) Symlink the manifest into the workdir so `demo.tape` can use a
#    workdir-local path (no $checkout-prefix needed). Resolve the
#    script's directory absolutely — `$0` may be relative.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ln -sf "$SCRIPT_DIR/manifest.yaml" "$WORKDIR/manifest.yaml"

echo '[01] ready: cd '"$WORKDIR"' && vhs '"$(dirname "$0")"'/demo.tape'
