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

# Compute absolute paths from SCRIPT_DIR (robust against cwd)
NAIA_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
NAMAKO_ROOT="$(cd "$NAIA_ROOT/../namako" && pwd)"
NAIA_NPA_ROOT="$NAIA_ROOT/test/npa"
ARTIFACTS_DIR="$NAIA_ROOT/target/namako_artifacts"
mkdir -p "$ARTIFACTS_DIR"

# Validate paths exist
if [[ ! -f "$NAMAKO_ROOT/Cargo.toml" ]]; then
    echo "❌ Cannot find namako at: $NAMAKO_ROOT"
    echo "   Expected sibling repo structure: .../specops/{naia,namako}"
    exit 1
fi

if [[ ! -f "$NAIA_NPA_ROOT/Cargo.toml" ]]; then
    echo "❌ Cannot find naia_npa adapter at: $NAIA_NPA_ROOT"
    echo "   Expected location: naia/test/npa/"
    exit 1
fi

NAMAKO_CLI="cargo run -p namako-cli --manifest-path $NAMAKO_ROOT/Cargo.toml --"
ADAPTER="cargo run --manifest-path $NAIA_NPA_ROOT/Cargo.toml --"

cd "$SPECS_DIR"

echo "=== Namako v1 CI Pipeline ==="
echo "Working directory: $SPECS_DIR"
echo ""

# Step 1: Lint
echo "[1/3] Running lint..."
if ! $NAMAKO_CLI lint -s . -a "$ADAPTER" -o "$ARTIFACTS_DIR/resolved_plan.json" 2>/dev/null; then
    echo "❌ Lint failed"
    exit 1
fi
echo "✓ Lint passed"
echo ""

# Step 2: Run
echo "[2/3] Running adapter execution..."
if ! $ADAPTER run -p "$ARTIFACTS_DIR/resolved_plan.json" -o "$ARTIFACTS_DIR/run_report.json" 2>/dev/null; then
    echo "❌ Run failed (adapter execution error)"
    exit 2
fi

# Check for failed scenarios in run_report
if grep -q '"status": "failed"' "$ARTIFACTS_DIR/run_report.json"; then
    echo "❌ Run completed with failed scenarios"
    exit 2
fi
echo "✓ Run passed"
echo ""

# Step 3: Verify
echo "[3/3] Running verify..."
if ! $NAMAKO_CLI verify -s . -a "$ADAPTER" -r "$ARTIFACTS_DIR/run_report.json" 2>/dev/null; then
    echo "❌ Verify failed (baseline mismatch)"
    exit 3
fi
echo "✓ Verify passed"
echo ""

echo "=== All checks passed ==="
