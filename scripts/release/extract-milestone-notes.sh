#!/bin/bash
# Extract a single milestone card from RELEASE_PLAN.md by version.
#
# Usage:
#   scripts/release/extract-milestone-notes.sh <version> [release-plan-path]
#
# Examples:
#   scripts/release/extract-milestone-notes.sh v0.9.0
#   scripts/release/extract-milestone-notes.sh v0.9.0 RELEASE_PLAN.md
#
# Writes the milestone's H3 section to stdout. Exits 1 with an actionable
# error if no matching card is found.
#
# Card format expected (per RELEASE_PLAN.md "Local Milestones"):
#   ### vX.Y.Z — <Milestone Title>
#   <!-- milestone-id: ... -->
#   - **Status:** ...
#   - **Due:** ...
#
#   <freeform paragraph>
#
# A card runs from its `### vX.Y.Z` header to the next `### vX.Y.Z` header
# (or the closing `<!-- /LOCAL MILESTONES -->` marker).

set -euo pipefail

VERSION="${1:-}"
RELEASE_PLAN="${2:-RELEASE_PLAN.md}"

if [ -z "$VERSION" ]; then
    echo "usage: $0 <version> [release-plan-path]" >&2
    echo "example: $0 v0.9.0" >&2
    exit 2
fi

if [ ! -f "$RELEASE_PLAN" ]; then
    echo "ERROR: $RELEASE_PLAN not found" >&2
    exit 2
fi

extract_card() {
    local target="$1"
    awk -v ver="$target" '
        BEGIN { in_card = 0 }
        /^### v[0-9]+\.[0-9]+\.[0-9]+/ {
            if ($0 ~ "^### " ver " ") {
                in_card = 1
                print
                next
            } else if (in_card) {
                in_card = 0
                exit
            }
        }
        /^<!-- \/LOCAL MILESTONES -->/ {
            if (in_card) {
                in_card = 0
                exit
            }
        }
        {
            if (in_card) print
        }
    ' "$RELEASE_PLAN"
}

CARD=$(extract_card "$VERSION")

# Pre-release tags (vX.Y.Z-rc.1, -beta.2, etc.) reuse their base version's
# milestone card. e.g. v0.9.0-rc.1 → look up v0.9.0.
if [ -z "$CARD" ] && [[ "$VERSION" =~ ^(v[0-9]+\.[0-9]+\.[0-9]+)- ]]; then
    BASE="${BASH_REMATCH[1]}"
    echo "INFO: no card for $VERSION; falling back to base version $BASE" >&2
    CARD=$(extract_card "$BASE")
fi

if [ -z "$CARD" ]; then
    {
        echo "ERROR: no milestone card for $VERSION found in $RELEASE_PLAN"
        echo
        echo "The release workflow expects an H3 header like:"
        echo "  ### $VERSION — <Milestone Title>"
        echo "inside the <!-- LOCAL MILESTONES --> ... <!-- /LOCAL MILESTONES --> block."
        echo
        echo "Available milestone cards:"
        grep -E '^### v[0-9]+\.[0-9]+\.[0-9]+' "$RELEASE_PLAN" || echo "  (none found)"
    } >&2
    exit 1
fi

printf '%s\n' "$CARD"
