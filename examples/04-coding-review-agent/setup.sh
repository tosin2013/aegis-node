#!/bin/sh
# Example 04 setup — stage /tmp/aegis-example-04/ for the code-review
# agent example. Builds a tiny git repo with two commits so the agent
# has a meaningful diff to review.

set -eu

WORKDIR=/tmp/aegis-example-04
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
require_bin git ''

echo '[04] aegis pull Qwen 2.5 1.5B Q4_K_M (cosign-verified)...'
aegis pull "$QWEN_REF" \
    --keyless-identity "$KEYLESS_IDENTITY" \
    --keyless-oidc-issuer "$KEYLESS_OIDC_ISSUER" >/dev/null

CACHED="$HOME/.cache/aegis/models/$QWEN_MANIFEST_SHA"
if [ ! -f "$CACHED/blob.bin" ]; then
    echo "ERROR: $CACHED/blob.bin missing after aegis pull" >&2
    exit 2
fi

echo "[04] staging $WORKDIR/..."
mkdir -p "$WORKDIR/output"
ln -sf "$CACHED/blob.bin"                 "$WORKDIR/model.gguf"
ln -sf "$CACHED/chat_template.sha256.txt" "$WORKDIR/chat_template.sha256.txt"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ln -sf "$SCRIPT_DIR/manifest.yaml" "$WORKDIR/manifest.yaml"
ln -sf "$SCRIPT_DIR/prompt.txt"    "$WORKDIR/prompt.txt"

# Build a tiny repo with two commits — the agent reviews the diff.
if [ ! -d "$WORKDIR/repo/.git" ]; then
    rm -rf "$WORKDIR/repo"
    mkdir -p "$WORKDIR/repo"
    cd "$WORKDIR/repo"
    git init -q .
    git config user.email 'example@aegis-node.local'
    git config user.name  'Example'
    cp "$SCRIPT_DIR/fixtures/buggy.go" main.go
    git add . && git commit -q -m 'initial: naive Add with overflow risk'
    cat > main.go <<'EOF'
package main

import (
	"fmt"
	"math"
)

// Add returns the saturated sum of a and b. Clamps to int32 bounds.
func Add(a, b int32) int32 {
	r := int64(a) + int64(b)
	if r > math.MaxInt32 {
		return math.MaxInt32
	}
	if r < math.MinInt32 {
		return math.MinInt32
	}
	return int32(r)
}

func main() {
	fmt.Println(Add(2147483647, 1))
}
EOF
    git add . && git commit -q -m 'fix: saturate Add on int32 overflow'
fi

cd "$WORKDIR"
rm -f ledger-*.jsonl output/code-review.md

cat <<EOF
[04] ready. Run the agent:

  cd $WORKDIR
  aegis run --manifest manifest.yaml \\
      --model model.gguf \\
      --workload code-reviewer --instance inst-001 \\
      --prompt "\$(cat prompt.txt)"
  cat output/code-review.md
  aegis verify ledger-*.jsonl
EOF
