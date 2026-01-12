#!/bin/bash
# spec_tool_test.sh - Self-test suite for spec_tool.sh
#
# Usage: ./specs/spec_tool_test.sh
#
# This script validates spec_tool.sh invariants using tiny fixtures in a temp directory.
# Runs in < 1s, never calls cargo, and is fully deterministic.
#
# Tests:
# - Contract ID extraction (multi-ID lines, alphanumeric suffixes)
# - Coverage counting (covered vs uncovered contracts)
# - Traceability marking (COVERED/UNCOVERED status)
# - Portability fallback (forced perl mode)

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Test counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# Helper functions
fail() {
    echo -e "${RED}✗ FAIL:${NC} $*" >&2
    ((TESTS_FAILED++)) || true
    return 1
}

pass() {
    echo -e "${GREEN}✓ PASS:${NC} $*"
    ((TESTS_PASSED++)) || true
}

assert_eq() {
    local actual="$1"
    local expected="$2"
    local test_name="$3"

    ((TESTS_RUN++)) || true

    if [[ "$actual" == "$expected" ]]; then
        pass "$test_name"
    else
        fail "$test_name: expected '$expected', got '$actual'"
    fi
}

assert_contains() {
    local haystack="$1"
    local needle="$2"
    local test_name="$3"

    ((TESTS_RUN++)) || true

    if echo "$haystack" | grep -qF "$needle"; then
        pass "$test_name"
    else
        fail "$test_name: expected output to contain '$needle'"
    fi
}

strip_ansi() {
    # Remove ANSI escape codes
    sed 's/\x1b\[[0-9;]*m//g'
}

# Create temp directory and setup cleanup
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "spec_tool.sh self-tests"
echo "======================="
echo "Temp directory: $TMP"
echo ""

# Create fixture directories
mkdir -p "$TMP/contracts"
mkdir -p "$TMP/generated"
mkdir -p "$TMP/test/tests"

# Create fixture contract registry (4 contracts)
cat > "$TMP/generated/CONTRACT_REGISTRY.md" << 'EOF'
# Contract ID Registry

**Generated:** 2026-01-12 00:00 UTC
**Total Contracts:** 4

---

## Full Contract Index

### Connection Lifecycle (1_connection_lifecycle.md)

- `connection-01`
- `connection-02`

### Entity Scopes (6_entity_scopes.md)

- `entity-scopes-03a`

### Uncovered (99_uncovered.md)

- `uncovered-99`

EOF

# Create fixture test files
cat > "$TMP/test/tests/a.rs" << 'EOF'
/// Contract: [connection-01], [connection-02]
#[test]
fn test_multi_contract() {
    // Test for connection-01 and connection-02
}
EOF

cat > "$TMP/test/tests/b.rs" << 'EOF'
/// Contract: [entity-scopes-03a]
#[test]
fn test_single_contract() {
    // Test for entity-scopes-03a
}
EOF

# Path to spec_tool.sh
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SPEC_TOOL="$SCRIPT_DIR/spec_tool.sh"

if [[ ! -x "$SPEC_TOOL" ]]; then
    echo -e "${RED}ERROR:${NC} spec_tool.sh not found or not executable at: $SPEC_TOOL"
    exit 1
fi

# Export environment overrides
export SPEC_TOOL_CONTRACTS_DIR="$TMP/contracts"
export SPEC_TOOL_GENERATED_DIR="$TMP/generated"
export SPEC_TOOL_TEST_DIR="$TMP/test/tests"

echo "Running test cases..."
echo ""

# ============================================================================
# Test Case A: Coverage counts multi-ID lines + suffix IDs (normal mode)
# ============================================================================

echo "Test Case A: Coverage counting (normal grep -P mode)"
echo "------------------------------------------------------"

OUTPUT_A=$("$SPEC_TOOL" coverage 2>&1 | strip_ansi)

# Extract metrics
TOTAL_A=$(echo "$OUTPUT_A" | grep "Total contracts in registry:" | grep -oE '[0-9]+' | head -1)
COVERED_A=$(echo "$OUTPUT_A" | grep "Contracts with test annotations:" | grep -oE '[0-9]+' | head -1)
COVERAGE_PCT_A=$(echo "$OUTPUT_A" | grep "Coverage:" | grep -oE '[0-9]+%' | sed 's/%//')

assert_eq "$TOTAL_A" "4" "Total contracts = 4"
assert_eq "$COVERED_A" "3" "Covered contracts = 3"
assert_eq "$COVERAGE_PCT_A" "75" "Coverage = 75%"
assert_contains "$OUTPUT_A" "uncovered-99" "Uncovered list contains uncovered-99"

echo ""

# ============================================================================
# Test Case B: Traceability marks covered/uncovered correctly (normal mode)
# ============================================================================

echo "Test Case B: Traceability matrix (normal grep -P mode)"
echo "-------------------------------------------------------"

"$SPEC_TOOL" traceability "$TMP/generated/TRACE.md" >/dev/null 2>&1
TRACE_OUTPUT=$(cat "$TMP/generated/TRACE.md")

# Check each contract
assert_contains "$TRACE_OUTPUT" "| \`connection-01\` |" "connection-01 in matrix"
assert_contains "$TRACE_OUTPUT" "| COVERED |" "connection-01 marked COVERED"
assert_contains "$TRACE_OUTPUT" "| a.rs |" "connection-01 points to a.rs"

assert_contains "$TRACE_OUTPUT" "| \`connection-02\` |" "connection-02 in matrix"
assert_contains "$TRACE_OUTPUT" "| \`entity-scopes-03a\` |" "entity-scopes-03a in matrix"
assert_contains "$TRACE_OUTPUT" "| b.rs |" "entity-scopes-03a points to b.rs"

assert_contains "$TRACE_OUTPUT" "| \`uncovered-99\` |" "uncovered-99 in matrix"
assert_contains "$TRACE_OUTPUT" "| **UNCOVERED** |" "uncovered-99 marked UNCOVERED"

echo ""

# ============================================================================
# Test Case C: Portability fallback (forced perl mode)
# ============================================================================

echo "Test Case C: Coverage counting (forced perl mode)"
echo "--------------------------------------------------"

export SPEC_TOOL_FORCE_PERL=1
OUTPUT_C=$("$SPEC_TOOL" coverage 2>&1 | strip_ansi)

# Extract metrics
TOTAL_C=$(echo "$OUTPUT_C" | grep "Total contracts in registry:" | grep -oE '[0-9]+' | head -1)
COVERED_C=$(echo "$OUTPUT_C" | grep "Contracts with test annotations:" | grep -oE '[0-9]+' | head -1)
COVERAGE_PCT_C=$(echo "$OUTPUT_C" | grep "Coverage:" | grep -oE '[0-9]+%' | sed 's/%//')

assert_eq "$TOTAL_C" "4" "Total contracts = 4 (perl mode)"
assert_eq "$COVERED_C" "3" "Covered contracts = 3 (perl mode)"
assert_eq "$COVERAGE_PCT_C" "75" "Coverage = 75% (perl mode)"
assert_contains "$OUTPUT_C" "uncovered-99" "Uncovered list contains uncovered-99 (perl mode)"

echo ""

echo "Test Case D: Traceability matrix (forced perl mode)"
echo "----------------------------------------------------"

"$SPEC_TOOL" traceability "$TMP/generated/TRACE_PERL.md" >/dev/null 2>&1
TRACE_PERL=$(cat "$TMP/generated/TRACE_PERL.md")

assert_contains "$TRACE_PERL" "| \`connection-01\` |" "connection-01 in matrix (perl mode)"
assert_contains "$TRACE_PERL" "| COVERED |" "connection-01 marked COVERED (perl mode)"
assert_contains "$TRACE_PERL" "| \`uncovered-99\` |" "uncovered-99 in matrix (perl mode)"
assert_contains "$TRACE_PERL" "| **UNCOVERED** |" "uncovered-99 marked UNCOVERED (perl mode)"

unset SPEC_TOOL_FORCE_PERL

echo ""

# ============================================================================
# Summary
# ============================================================================

echo "======================================================================"
echo "Test Summary"
echo "======================================================================"
echo "Tests run:    $TESTS_RUN"
echo "Tests passed: $TESTS_PASSED"
echo "Tests failed: $TESTS_FAILED"
echo ""

if [[ $TESTS_FAILED -eq 0 ]]; then
    echo -e "${GREEN}spec_tool self-tests: PASS${NC}"
    exit 0
else
    echo -e "${RED}spec_tool self-tests: FAIL${NC}"
    echo ""
    echo "What would fail if multi-ID parsing broke:"
    echo "  - Test 'Covered contracts = 3' would fail (would count 2 instead)"
    echo "  - Coverage percentage would be wrong (50% instead of 75%)"
    echo "  - Traceability matrix would miss multi-contract annotations"
    exit 1
fi
