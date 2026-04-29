#!/usr/bin/env bash
# Enforce ADR-010's air-gap rules on tools/replay-viewer/index.html.
#
# Per F8 / issue #62: the viewer must work in a network-locked browser.
# Anything that looks like an outbound request — fetch, XHR, src= with
# a remote URL, link rel=stylesheet pointing somewhere — fails the
# build. CI runs this on every PR via .github/workflows/replay-viewer.yml.
#
# Exit codes:
#   0  viewer is air-gap clean
#   1  forbidden pattern found

set -euo pipefail

VIEWER="$(dirname "$0")/index.html"

if [[ ! -f "$VIEWER" ]]; then
  echo "viewer not found at $VIEWER" >&2
  exit 1
fi

# Patterns that would let the viewer reach the network. Each match
# fails the check. Comments / docstrings calling those names out are
# allowed only when wrapped in an HTML comment (we strip those first).
TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT
# strip <!-- ... --> blocks (single-line and multi-line)
perl -0777 -pe 's/<!--.*?-->//gs' "$VIEWER" > "$TMP"

forbidden=(
  # HTML
  '<script[^>]+\bsrc='
  '<link[^>]+\brel="?stylesheet"?[^>]+\bhref='
  '<iframe[^>]+\bsrc='
  # Remote-loaded media
  '<img[^>]+\bsrc=["'\'']https?:'
  '<audio[^>]+\bsrc=["'\'']https?:'
  '<video[^>]+\bsrc=["'\'']https?:'
  # JS network APIs
  '\bfetch\s*\('
  '\bXMLHttpRequest\b'
  '\bnavigator\.sendBeacon\b'
  '\bnew\s+EventSource\b'
  '\bnew\s+WebSocket\b'
  'importScripts\s*\('
  # Dynamic module load with remote URL
  'import\s*\(\s*["'\'']https?:'
)

failed=0
for pat in "${forbidden[@]}"; do
  if grep -nE "$pat" "$TMP" >/dev/null; then
    echo "FAIL: pattern matches in tools/replay-viewer/index.html: $pat"
    grep -nE "$pat" "$TMP" || true
    failed=1
  fi
done

if [[ $failed -eq 0 ]]; then
  echo "OK: tools/replay-viewer/index.html is air-gap clean"
fi
exit $failed
