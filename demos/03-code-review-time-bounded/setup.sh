#!/bin/sh
# Demo 03 setup — stage the workdir for the code-review demo.
# Idempotent. Invoked by `make 03-code-review-time-bounded`.
#
# Stages:
#   /tmp/aegis-demo-03/model.litertlm          (Gemma 4 E4B symlink)
#   /tmp/aegis-demo-03/chat_template.sha256.txt
#   /tmp/aegis-demo-03/manifest.yaml           (symlink to source)
#   /tmp/aegis-demo-03/repo/lib.rs             (git repo with 2 commits)
#   /tmp/aegis-demo-03/data/                   (mkdir; review.md lands here)

set -eu

WORKDIR=/tmp/aegis-demo-03
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
require_bin git ''

aegis run --help 2>&1 | grep -q -- '--backend' || {
    echo 'ERROR: aegis on PATH was built without --backend support.' >&2
    echo '  rebuild: cargo install --path crates/cli --features litertlm --force' >&2
    exit 2
}

echo '[03] aegis pull Gemma 4 E4B (cosign-verified)...'
aegis pull "$GEMMA_E4B_REF" \
    --keyless-identity "$KEYLESS_IDENTITY" \
    --keyless-oidc-issuer "$KEYLESS_OIDC_ISSUER" >/dev/null

CACHED="$HOME/.cache/aegis/models/$GEMMA_E4B_MANIFEST_SHA"
if [ ! -f "$CACHED/blob.bin" ]; then
    echo "ERROR: $CACHED/blob.bin missing after aegis pull" >&2
    exit 2
fi

echo "[03] staging $WORKDIR/..."
mkdir -p "$WORKDIR/data"
# Resolve the script's directory absolutely — `$0` may be relative.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ln -sf "$CACHED/blob.bin"                      "$WORKDIR/model.litertlm"
ln -sf "$CACHED/chat_template.sha256.txt"      "$WORKDIR/chat_template.sha256.txt"
ln -sf "$SCRIPT_DIR/manifest.yaml"             "$WORKDIR/manifest.yaml"

# Stage the git repo. Two commits so a `git diff HEAD~1 HEAD` has
# something meaningful to review. Idempotent — the second run sees
# the existing repo and doesn't duplicate commits.
if [ ! -d "$WORKDIR/repo/.git" ]; then
    rm -rf "$WORKDIR/repo"
    mkdir -p "$WORKDIR/repo"
    cd "$WORKDIR/repo"
    git init -q .
    git config user.email 'demo@aegis-node.local'
    git config user.name  'Demo'
    printf 'pub fn add(a: i32, b: i32) -> i32 { a + b }\n' > lib.rs
    git add . && git commit -q -m 'initial'
    printf 'pub fn add(a: i32, b: i32) -> i32 {\n    a.checked_add(b).unwrap_or(0)\n}\n' > lib.rs
    git add . && git commit -q -m 'saturate on overflow'
fi

# Pre-clean ledger + draft review from any prior render so the .tape's
# `rm -f ledger-*.jsonl` is a no-op for the demo viewer.
rm -f "$WORKDIR"/ledger-*.jsonl "$WORKDIR/data/review.md"

echo '[03] ready'
