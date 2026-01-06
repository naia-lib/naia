#!/bin/bash
# Run all E2E tests and calculate pass rate
#
# Usage:
#   ./scripts/run_e2e_tests.sh
#   ./scripts/run_e2e_tests.sh --quiet  # Only show final pass rate

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

QUIET=false
if [[ "$1" == "--quiet" ]]; then
    QUIET=true
fi

cd "$REPO_ROOT"

if [[ "$QUIET" == "true" ]]; then
    cargo test -p naia-test 2>&1 | python3 "$SCRIPT_DIR/calculate_pass_rate.py" 2>&1 | grep -E "PASS RATE:|PASSED:|FAILED:|TOTAL:"
else
    cargo test -p naia-test 2>&1 | python3 "$SCRIPT_DIR/calculate_pass_rate.py"
fi
