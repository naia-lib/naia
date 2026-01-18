#!/bin/bash
# Tesaki Enablement Stub
#
# Per TODO.md §5, this script provides scaffolding for Tesaki integration.
# It runs status and review commands and outputs a single-line summary
# of the recommended next action.
#
# Usage: ./tesaki_next.sh
# Output:
#   NEXT_ACTION: <action>
#   Files generated:
#     target/namako_artifacts/tesaki/status.json
#     target/namako_artifacts/tesaki/review.json

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SPECS_DIR="$(dirname "$SCRIPT_DIR")"
NAIA_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
NAMAKO_ROOT="$(cd "$NAIA_ROOT/../namako" && pwd)"
NAIA_NPAP_ROOT="$NAIA_ROOT/test/npap"
ARTIFACTS_DIR="$NAIA_ROOT/target/namako_artifacts/tesaki"

mkdir -p "$ARTIFACTS_DIR"

NAMAKO_CLI="cargo run -p namako-cli --manifest-path $NAMAKO_ROOT/Cargo.toml -q --"
ADAPTER="cargo run --manifest-path $NAIA_NPAP_ROOT/Cargo.toml --"

cd "$SPECS_DIR"

# Run status
$NAMAKO_CLI status -a "$ADAPTER" --json --out "$ARTIFACTS_DIR/status.json" 2>/dev/null

# Run review
$NAMAKO_CLI review -a "$ADAPTER" --out "$ARTIFACTS_DIR/review.json" 2>/dev/null

# Extract recommended_next_action from status
NEXT_ACTION=$(jq -r '.recommended_next_action' "$ARTIFACTS_DIR/status.json" 2>/dev/null || echo "UNKNOWN")

# Extract coverage summary
EXECUTABLE=$(jq -r '.coverage_summary.executable_scenarios_total' "$ARTIFACTS_DIR/review.json" 2>/dev/null || echo "?")
DEFERRED=$(jq -r '.coverage_summary.deferred_items_total' "$ARTIFACTS_DIR/review.json" 2>/dev/null || echo "?")
PROMOTABLE=$(jq -r '.promotion_candidates | length' "$ARTIFACTS_DIR/review.json" 2>/dev/null || echo "?")

# Print machine-readable summary
echo "TESAKI_STATUS_VERSION=1"
echo "NEXT_ACTION=$NEXT_ACTION"
echo "EXECUTABLE_SCENARIOS=$EXECUTABLE"
echo "DEFERRED_ITEMS=$DEFERRED"
echo "PROMOTION_CANDIDATES=$PROMOTABLE"
echo "STATUS_FILE=$ARTIFACTS_DIR/status.json"
echo "REVIEW_FILE=$ARTIFACTS_DIR/review.json"

# Human-readable one-liner
case "$NEXT_ACTION" in
    "DONE")
        echo ""
        echo "✓ All gates green. No action required."
        ;;
    "RUN")
        echo ""
        echo "→ Run: Execute tests with adapter and update certification."
        ;;
    "RUN_LINT")
        echo ""
        echo "→ Lint: Resolution needed before running tests."
        ;;
    "FIX_LINT")
        echo ""
        echo "→ Fix: Lint failed. Add missing bindings or fix spec errors."
        ;;
    "FIX_RUN")
        echo ""
        echo "→ Fix: Tests failed. Debug and fix step implementations."
        ;;
    "RUN_VERIFY")
        echo ""
        echo "→ Verify: Run verify to confirm baseline matches."
        ;;
    "NEEDS_UPDATE_CERT_APPROVAL")
        echo ""
        echo "→ Update: Drift detected. Review changes and run update-cert."
        ;;
    *)
        echo ""
        echo "? Unknown action: $NEXT_ACTION"
        ;;
esac
