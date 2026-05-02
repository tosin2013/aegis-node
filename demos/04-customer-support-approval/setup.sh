#!/bin/sh
# Demo 04 setup — stage the workdir for the customer-support demo.
# Idempotent. Invoked by `make 04-customer-support-approval`.
#
# Stages:
#   /tmp/aegis-demo-04/model.litertlm          (Gemma 4 E2B symlink)
#   /tmp/aegis-demo-04/chat_template.sha256.txt
#   /tmp/aegis-demo-04/manifest.yaml           (symlink to source)
#   /tmp/aegis-demo-04/cases/case-1024.txt     (sample customer complaint)
#   /tmp/aegis-demo-04/drafts/                 (empty; refund-letter.md lands here)
#   /tmp/aegis-demo-04/approval.json           (F3 file-channel pre-approval)

set -eu

WORKDIR=/tmp/aegis-demo-04
GEMMA_E2B_REF='ghcr.io/tosin2013/aegis-node-models/gemma-4-e2b-it@sha256:365c6a8b3b226ec825b74ed16404515ec61521b2d7f24490eac672d74466b2ea'
# aegis pull keys its cache by the OCI manifest digest, not the blob hash.
GEMMA_E2B_MANIFEST_SHA='365c6a8b3b226ec825b74ed16404515ec61521b2d7f24490eac672d74466b2ea'
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

echo '[04] aegis pull Gemma 4 E2B (cosign-verified)...'
aegis pull "$GEMMA_E2B_REF" \
    --keyless-identity "$KEYLESS_IDENTITY" \
    --keyless-oidc-issuer "$KEYLESS_OIDC_ISSUER" >/dev/null

CACHED="$HOME/.cache/aegis/models/$GEMMA_E2B_MANIFEST_SHA"
if [ ! -f "$CACHED/blob.bin" ]; then
    echo "ERROR: $CACHED/blob.bin missing after aegis pull" >&2
    exit 2
fi

echo "[04] staging $WORKDIR/..."
mkdir -p "$WORKDIR/cases" "$WORKDIR/drafts"
# Resolve the script's directory absolutely — `$0` may be relative.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ln -sf "$CACHED/blob.bin"                      "$WORKDIR/model.litertlm"
ln -sf "$CACHED/chat_template.sha256.txt"      "$WORKDIR/chat_template.sha256.txt"
ln -sf "$SCRIPT_DIR/manifest.yaml"             "$WORKDIR/manifest.yaml"

# Sample customer complaint. Pre-staged so the GIF doesn't need to
# heredoc it (and so the prompt is grounded in real text the
# auditor can read in the ledger).
cat > "$WORKDIR/cases/case-1024.txt" <<'EOF'
Customer #1024 reports their package arrived damaged on 2026-04-15.
Order total: $87.43. They request a full refund.
EOF

# F3 file-channel pre-approval. VHS's `Type` syntax doesn't tolerate
# escaped-quote shell heredocs, so we stage this here and the .tape
# just `cat`s it for the viewer.
#
# decision: granted | rejected (per ADR-005 schema). The demo runs
# the granted-path; flipping to rejected here demonstrates the deny
# path in a separate take.
cat > "$WORKDIR/approval.json" <<'EOF'
{
  "decision": "granted",
  "approver": "alice@org",
  "reason": "verified case; refund within policy"
}
EOF

# Pre-clean ledger + draft from any prior render so the .tape's
# `rm -f ledger-*.jsonl` is a no-op for the demo viewer.
rm -f "$WORKDIR"/ledger-*.jsonl "$WORKDIR/drafts/refund-letter.md"

echo '[04] ready'
