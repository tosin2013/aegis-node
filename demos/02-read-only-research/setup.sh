#!/bin/sh
# Demo 02 setup — stage everything `demo.tape` expects into
# /tmp/aegis-demo-02/. Idempotent; safe to re-run. Invoked
# automatically by `make 02-read-only-research` (per
# demos/Makefile's `%/demo.gif` rule).
#
# Per ADR-020 + LiteRT-D (#119). Uses the LiteRT-LM backend +
# Gemma 4 E4B; staged paths are workdir-local (no /data, /repo,
# etc. — all under /tmp/aegis-demo-02/).

set -eu

WORKDIR=/tmp/aegis-demo-02
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

require_bin aegis 'cargo install --path crates/cli --features litertlm'
require_bin oras 'https://github.com/oras-project/oras/releases'
require_bin cosign 'https://github.com/sigstore/cosign/releases'

aegis run --help 2>&1 | grep -q -- '--backend' || {
    echo 'ERROR: aegis on PATH was built without --backend support.' >&2
    echo '  rebuild: cargo install --path crates/cli --features litertlm --force' >&2
    exit 2
}

# 1) Pull the cosign-verified Gemma 4 E4B (one-time per machine; oras
#    cache-hits subsequent runs).
echo '[02] aegis pull Gemma 4 E4B (cosign-verified)...'
aegis pull "$GEMMA_E4B_REF" \
    --keyless-identity "$KEYLESS_IDENTITY" \
    --keyless-oidc-issuer "$KEYLESS_OIDC_ISSUER" >/dev/null

CACHED="$HOME/.cache/aegis/models/$GEMMA_E4B_MANIFEST_SHA"
if [ ! -f "$CACHED/blob.bin" ]; then
    echo "ERROR: $CACHED/blob.bin missing after aegis pull" >&2
    exit 2
fi

# 2) Stage the workdir. Symlinks (not copies) so a later `aegis pull`
#    re-render doesn't copy 3.65 GB twice.
echo "[02] staging $WORKDIR/..."
mkdir -p "$WORKDIR/data"
ln -sf "$CACHED/blob.bin"                      "$WORKDIR/model.litertlm"
ln -sf "$CACHED/chat_template.sha256.txt"      "$WORKDIR/chat_template.sha256.txt"

# 3) The sample data the agent reads. Fixed content for deterministic
#    re-renders.
cat > "$WORKDIR/data/research-notes.txt" <<'EOF'
Q3 2025: revenue $147M, EBITDA $42M, headcount 380. Customer churn 4.2%, NPS 62.
EOF

# 4) Symlink the manifest into the workdir so `demo.tape` can use a
#    workdir-local path (no $checkout-prefix needed). Resolve the
#    script's directory absolutely — `$0` may be relative.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ln -sf "$SCRIPT_DIR/manifest.yaml" "$WORKDIR/manifest.yaml"

echo '[02] ready: cd '"$WORKDIR"' && vhs '"$(dirname "$0")"'/demo.tape'
