#!/bin/bash
# Namako v1 CI Pipeline for Naia Specs
# Usage: ./scripts/namako_ci.sh
#
# Runs the full lint → run → verify pipeline.
# Exit codes:
#   0 = All green
#   1 = Lint failed
#   2 = Run failed (step execution failure)
#   3 = Verify failed (baseline mismatch)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SPECS_DIR="$(dirname "$SCRIPT_DIR")"
ADAPTER="cargo run --manifest-path $SPECS_DIR/../namako/Cargo.toml --"

cd "$SPECS_DIR"

echo "=== Namako v1 CI Pipeline ==="
echo "Working directory: $SPECS_DIR"
echo ""

# Step 1: Lint
echo "[1/3] Running lint..."
if ! cargo run -p namako-cli --manifest-path ../../namako/Cargo.toml -- \
    lint -s . -a "$ADAPTER" -o resolved_plan.json 2>/dev/null; then
    echo "❌ Lint failed"
    exit 1
fi
echo "✓ Lint passed"
echo ""

# Step 2: Run
echo "[2/3] Running adapter execution..."
if ! $ADAPTER run -p resolved_plan.json -o run_report.json 2>/dev/null; then
    echo "❌ Run failed (adapter execution error)"
    exit 2
fi

# Check for failed scenarios in run_report
if grep -q '"status": "failed"' run_report.json; then
    echo "❌ Run completed with failed scenarios"
    exit 2
fi
echo "✓ Run passed"
echo ""

# Step 3: Verify
echo "[3/3] Running verify..."
if ! cargo run -p namako-cli --manifest-path ../../namako/Cargo.toml -- \
    verify -s . -a "$ADAPTER" 2>/dev/null; then
    echo "❌ Verify failed (baseline mismatch)"
    exit 3
fi
echo "✓ Verify passed"
echo ""

echo "=== All checks passed ==="
