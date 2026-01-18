#!/bin/bash
# Determinism Enforcement Script for Namako v2 Commands
#
# Per TODO.md §4, this script verifies that:
# - namako status --json is deterministic (except timestamp)
# - namako review is byte-identical across runs
# - namako explain is byte-identical across runs

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SPECS_DIR="$(dirname "$SCRIPT_DIR")"
NAIA_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
NAMAKO_ROOT="$(cd "$NAIA_ROOT/../namako" && pwd)"
NAIA_NPAP_ROOT="$NAIA_ROOT/test/npap"
ARTIFACTS_DIR="$NAIA_ROOT/target/namako_artifacts/determinism_test"

# Clean up artifacts
rm -rf "$ARTIFACTS_DIR"
mkdir -p "$ARTIFACTS_DIR"

NAMAKO_CLI="cargo run -p namako-cli --manifest-path $NAMAKO_ROOT/Cargo.toml --"
ADAPTER="cargo run --manifest-path $NAIA_NPAP_ROOT/Cargo.toml --"

cd "$SPECS_DIR"

echo "=== Namako v2 Determinism Verification ==="
echo "Artifacts: $ARTIFACTS_DIR"
echo ""

# Test 1: status --json determinism (ignoring timestamp)
echo "[1/3] Testing status --json determinism..."
$NAMAKO_CLI status -a "$ADAPTER" --json --out "$ARTIFACTS_DIR/status1.json" 2>/dev/null
$NAMAKO_CLI status -a "$ADAPTER" --json --out "$ARTIFACTS_DIR/status2.json" 2>/dev/null

# Strip timestamp_utc field for comparison
jq 'del(.timestamp_utc)' "$ARTIFACTS_DIR/status1.json" > "$ARTIFACTS_DIR/status1_normalized.json"
jq 'del(.timestamp_utc)' "$ARTIFACTS_DIR/status2.json" > "$ARTIFACTS_DIR/status2_normalized.json"

if diff -q "$ARTIFACTS_DIR/status1_normalized.json" "$ARTIFACTS_DIR/status2_normalized.json" > /dev/null; then
    echo "  ✓ status --json is deterministic (ignoring timestamp)"
else
    echo "  ✗ status --json differs between runs!"
    diff "$ARTIFACTS_DIR/status1_normalized.json" "$ARTIFACTS_DIR/status2_normalized.json"
    exit 1
fi

# Test 2: review determinism
echo "[2/3] Testing review determinism..."
$NAMAKO_CLI review -a "$ADAPTER" --out "$ARTIFACTS_DIR/review1.json" 2>/dev/null
$NAMAKO_CLI review -a "$ADAPTER" --out "$ARTIFACTS_DIR/review2.json" 2>/dev/null

if diff -q "$ARTIFACTS_DIR/review1.json" "$ARTIFACTS_DIR/review2.json" > /dev/null; then
    echo "  ✓ review is byte-identical across runs"
else
    echo "  ✗ review differs between runs!"
    diff "$ARTIFACTS_DIR/review1.json" "$ARTIFACTS_DIR/review2.json" | head -20
    exit 1
fi

# Test 3: explain determinism
echo "[3/3] Testing explain determinism..."
# Pick a known scenario key from smoke.feature
SCENARIO_KEY="features/smoke.feature:L8"
$NAMAKO_CLI explain -a "$ADAPTER" --scenario-key "$SCENARIO_KEY" --out "$ARTIFACTS_DIR/explain1.json" 2>/dev/null
$NAMAKO_CLI explain -a "$ADAPTER" --scenario-key "$SCENARIO_KEY" --out "$ARTIFACTS_DIR/explain2.json" 2>/dev/null

if diff -q "$ARTIFACTS_DIR/explain1.json" "$ARTIFACTS_DIR/explain2.json" > /dev/null; then
    echo "  ✓ explain is byte-identical across runs"
else
    echo "  ✗ explain differs between runs!"
    diff "$ARTIFACTS_DIR/explain1.json" "$ARTIFACTS_DIR/explain2.json" | head -20
    exit 1
fi

echo ""
echo "=== All determinism checks passed ==="
echo ""
echo "Allowed variations:"
echo "  - status: timestamp_utc field"
echo "  - review: none"
echo "  - explain: none"
