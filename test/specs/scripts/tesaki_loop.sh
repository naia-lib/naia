#!/bin/bash
# ============================================================================
# Tesaki v0 Loop Script — NEXT_TASK.md Generator
# ============================================================================
#
# Per TODO.md §2, this script produces:
#   - target/namako_artifacts/tesaki/status.json
#   - target/namako_artifacts/tesaki/review.json
#   - target/namako_artifacts/tesaki/NEXT_TASK.md
#
# It never modifies repo files outside target/namako_artifacts/.
#
# Usage: ./tesaki_loop.sh
# ============================================================================

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

echo "=== Tesaki v0 Loop ==="
echo "Artifacts: $ARTIFACTS_DIR"
echo ""

# Step 1: Run status
echo "[1/3] Running namako status..."
$NAMAKO_CLI status -a "$ADAPTER" --json --out "$ARTIFACTS_DIR/status.json" 2>/dev/null

# Step 2: Run review
echo "[2/3] Running namako review..."
$NAMAKO_CLI review -a "$ADAPTER" --out "$ARTIFACTS_DIR/review.json" 2>/dev/null

# Step 3: Parse and generate NEXT_TASK.md
echo "[3/3] Generating NEXT_TASK.md..."

# Extract recommended_next_action
NEXT_ACTION=$(jq -r '.recommended_next_action // "UNKNOWN"' "$ARTIFACTS_DIR/status.json" 2>/dev/null)

# Extract coverage info
EXECUTABLE=$(jq -r '.coverage_summary.executable_scenarios_total // 0' "$ARTIFACTS_DIR/review.json" 2>/dev/null)
DEFERRED=$(jq -r '.coverage_summary.deferred_items_total // 0' "$ARTIFACTS_DIR/review.json" 2>/dev/null)
PROMOTABLE=$(jq -r '.promotion_candidates | length' "$ARTIFACTS_DIR/review.json" 2>/dev/null || echo "0")

# Extract top 3 promotion candidates (per TODO.md §3.1)
TOP_CANDIDATES=$(jq -r '
  .promotion_candidates[:3] |
  to_entries |
  map("  \(.key + 1). **\(.value.scenario_name)**\n     - Feature: `\(.value.feature_path)`\n     - Rule: \(.value.rule_name)\n     - Reuse score: \(.value.reuse_score), New steps: \(.value.new_step_texts_estimate)") |
  join("\n\n")
' "$ARTIFACTS_DIR/review.json" 2>/dev/null || echo "  (none)")

# Extract missing bindings for top candidates
MISSING_BINDINGS=$(jq -r '
  .missing_bindings_for_top_candidates[:3] |
  map("  - **\(.candidate_name)**: \(.missing_step_texts | join(", ") | if . == "" then "(all steps covered)" else . end)") |
  join("\n")
' "$ARTIFACTS_DIR/review.json" 2>/dev/null || echo "  (none)")

# Extract drift details if any
DRIFT_KIND=$(jq -r '.drift.kind // "NONE"' "$ARTIFACTS_DIR/status.json" 2>/dev/null)
DRIFT_DETAILS=$(jq -r '.drift.details // [] | map("  - \(.field): \(.baseline) → \(.current)") | join("\n")' "$ARTIFACTS_DIR/status.json" 2>/dev/null || echo "  (unable to parse drift details)")

# Generate NEXT_TASK.md based on action
NEXT_TASK_FILE="$ARTIFACTS_DIR/NEXT_TASK.md"

cat > "$NEXT_TASK_FILE" << EOF
# NEXT_TASK.md — Tesaki v0 Generated Task

**Generated:** $(date -u +"%Y-%m-%dT%H:%M:%SZ")
**Action:** \`$NEXT_ACTION\`

---

## Current Status

| Metric | Value |
|--------|-------|
| Executable Scenarios | $EXECUTABLE |
| Deferred Items | $DEFERRED |
| Promotion Candidates | $PROMOTABLE |
| Drift Status | $DRIFT_KIND |

---

EOF

case "$NEXT_ACTION" in
    "DONE")
        cat >> "$NEXT_TASK_FILE" << EOF
## Task: Propose Micro-Milestone Batch

All gates are green. The system is stable.

### Recommended Next Steps

If promotion candidates exist, consider promoting the top 3 scenarios from Deferred → Executable:

$TOP_CANDIDATES

### Missing Bindings to Implement

$MISSING_BINDINGS

### Instructions

1. Uncomment/enable the top candidate scenarios in their \`.feature\` files
2. Implement missing step bindings in \`naia/test/tests/src/steps/\`
3. Run \`bash scripts/namako_ci.sh\` until green
4. Run \`bash scripts/determinism_check.sh\` to verify determinism
5. If baseline drift is detected, request \`update-cert\` approval

---

*If no promotion candidates exist, consider adding new deferred scenarios or expanding to new feature files.*
EOF
        ;;
    "FIX_LINT")
        cat >> "$NEXT_TASK_FILE" << EOF
## Task: Fix Lint Errors

Lint failed. Missing step bindings or spec errors detected.

### Top Candidates with Missing Bindings

$MISSING_BINDINGS

### Instructions

1. Review the lint errors: \`$NAMAKO_CLI lint -s . -a "$ADAPTER"\`
2. Implement missing step bindings in \`naia/test/tests/src/steps/\`
3. Run \`bash scripts/namako_ci.sh\` to verify fix
4. Repeat until lint passes

---
EOF
        ;;
    "FIX_RUN")
        # Extract failure info if available (TODO.md §4)
        LAST_RUN_FAILURES=$(jq -r '.last_run_failures // [] |
          map("  - \(.scenario_key): \(.scenario_name) [\(.failure_kind)]") |
          join("\n")' "$ARTIFACTS_DIR/status.json" 2>/dev/null || echo "  (no failure details available)")

        # Try to get first failing scenario for explain
        FIRST_FAILURE_KEY=$(jq -r '.last_run_failures[0].scenario_key // ""' "$ARTIFACTS_DIR/status.json" 2>/dev/null)

        if [[ -n "$FIRST_FAILURE_KEY" && "$FIRST_FAILURE_KEY" != "null" ]]; then
            echo "  Generating explain for failing scenario: $FIRST_FAILURE_KEY"
            $NAMAKO_CLI explain -a "$ADAPTER" --scenario-key "$FIRST_FAILURE_KEY" \
                --out "$ARTIFACTS_DIR/explain_failure.json" 2>/dev/null || true
        fi

        cat >> "$NEXT_TASK_FILE" << EOF
## Task: Fix Failing Scenarios

Test execution failed. Debug and fix step implementations.

### Failing Scenarios

$LAST_RUN_FAILURES

### Explain Packet

$(if [[ -f "$ARTIFACTS_DIR/explain_failure.json" ]]; then
    echo "See: \`$ARTIFACTS_DIR/explain_failure.json\`"
else
    echo "(No explain packet generated — failure details may not be machine-readable yet)"
fi)

### Fix Categories

- **Binding Bug:** Step implementation is incorrect
- **Harness Gap:** Test harness missing capability
- **SUT Behavior Mismatch:** System under test behaves differently than specified

### Instructions

1. Identify the root cause from the failure output
2. Fix the binding, harness, or investigate SUT behavior
3. Run \`bash scripts/namako_ci.sh\` to verify fix
4. If behavior differs from spec, file a clarification request

---
EOF
        ;;
    "RUN_LINT")
        cat >> "$NEXT_TASK_FILE" << EOF
## Task: Run Lint

Resolution needed before running tests.

### Instructions

1. Run: \`bash scripts/namako_ci.sh\`
2. This will execute lint → run → verify pipeline
3. If lint fails, implement missing bindings

---
EOF
        ;;
    "RUN")
        cat >> "$NEXT_TASK_FILE" << EOF
## Task: Run Tests

Execute tests with adapter and update certification if needed.

### Instructions

1. Run: \`bash scripts/namako_ci.sh\`
2. If run passes, verify will check baseline
3. If drift detected, request \`update-cert\` approval

---
EOF
        ;;
    "RUN_VERIFY")
        cat >> "$NEXT_TASK_FILE" << EOF
## Task: Verify Baseline

Run verify to confirm baseline matches current state.

### Instructions

1. Run: \`bash scripts/namako_ci.sh\`
2. If verify fails, baseline drift is detected
3. Request \`update-cert\` approval if changes are intentional

---
EOF
        ;;
    "NEEDS_UPDATE_CERT_APPROVAL")
        cat >> "$NEXT_TASK_FILE" << EOF
## STOP: Approval Required

Drift detected between current state and baseline certification.

### Drift Details

$DRIFT_DETAILS

### ⚠️ DO NOT PROCEED WITHOUT EXPLICIT APPROVAL

The baseline certification must be updated, but this requires Connor's explicit approval.

### What Changed

Review the drift details above. Common causes:
- Feature file content changed (intentional spec update)
- Step bindings changed (implementation fix)
- Step registry changed (new/modified bindings)

### Instructions

1. Review the drift details carefully
2. **STOP AND WAIT** for Connor's approval
3. Only after approval: \`$NAMAKO_CLI update-cert -s . -a "$ADAPTER"\`

---
EOF
        ;;
    *)
        cat >> "$NEXT_TASK_FILE" << EOF
## Task: Unknown State

Recommended action: \`$NEXT_ACTION\` is not recognized.

### Instructions

1. Check the status.json for more details
2. Manually investigate the state
3. Run: \`bash scripts/namako_ci.sh\` to attempt recovery

---
EOF
        ;;
esac

# Add artifact references
cat >> "$NEXT_TASK_FILE" << EOF

## Artifacts

| Artifact | Path |
|----------|------|
| Status | \`$ARTIFACTS_DIR/status.json\` |
| Review | \`$ARTIFACTS_DIR/review.json\` |
$(if [[ -f "$ARTIFACTS_DIR/explain_failure.json" ]]; then
    echo "| Explain (Failure) | \`$ARTIFACTS_DIR/explain_failure.json\` |"
fi)

---

*Generated by tesaki_loop.sh — Tesaki v0 (no inference)*
EOF

echo ""
echo "✓ Generated: $NEXT_TASK_FILE"
echo ""
echo "=== Summary ==="
echo "Action: $NEXT_ACTION"
echo "Executable: $EXECUTABLE | Deferred: $DEFERRED | Promotable: $PROMOTABLE"
echo ""
cat "$NEXT_TASK_FILE"
