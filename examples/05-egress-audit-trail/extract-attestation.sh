#!/bin/sh
# Promote the F6 NetworkAttestation entry from the ledger into a
# standalone, human-readable artifact. Run from the workdir after the
# agent finishes: the artifact lands at output/network-attestation.json
# and a session report at output/session-report.md.

set -eu

LEDGER=$(ls ledger-*.jsonl 2>/dev/null | head -1)
if [ -z "${LEDGER:-}" ]; then
    echo 'ERROR: no ledger-*.jsonl in cwd' >&2
    exit 1
fi

mkdir -p output

# F6 NetworkAttestation entry (signed, end-of-session summary)
grep network_attestation "$LEDGER" | head -1 | jq . > output/network-attestation.json

# Human-readable session report — answers "what did the agent do?"
{
    echo '# Session report'
    echo
    echo "Ledger: $LEDGER"
    echo "Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo
    echo '## Tool calls attempted'
    echo
    grep accessType "$LEDGER" \
        | jq -r 'select(.entryType == "access" or .entryType == "violation") | "- \(.entryType | ascii_upcase): \(.accessType) → \(.resourceUri // "(no uri)")"' \
        || true
    echo
    echo '## F6 Network attestation (signed)'
    echo
    echo '```json'
    cat output/network-attestation.json
    echo '```'
    echo
    echo '## Chain integrity'
    echo
    echo '```'
    aegis verify "$LEDGER" 2>&1 || true
    echo '```'
} > output/session-report.md

echo "[05] artifacts ready:"
echo "  output/network-attestation.json"
echo "  output/session-report.md"
