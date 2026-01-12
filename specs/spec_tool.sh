#!/bin/bash
# spec_tool.sh - Comprehensive CLI for Naia specifications management
#
# Usage: ./spec_tool.sh <command> [options]
#
# Commands:
#   bundle          Generate NAIA_SPECS.md (includes all specs + template)
#   lint            Check all specs for consistency issues
#   validate        Run all validation checks
#   registry        Extract all contract IDs to registry file
#   check-orphans   Find MUST/MUST NOT without contract IDs
#   check-refs      Verify all cross-reference links
#   stats           Show statistics about specs
#   coverage        Analyze contract test coverage
#   gen-test        Generate test skeleton for a contract
#   traceability    Generate contract-to-test matrix
#   help            Show this help message

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Allow environment overrides for testing (defaults preserve existing behavior)
CONTRACTS_DIR="${SPEC_TOOL_CONTRACTS_DIR:-$SCRIPT_DIR/contracts}"
GENERATED_DIR="${SPEC_TOOL_GENERATED_DIR:-$SCRIPT_DIR/generated}"
TEST_DIR="${SPEC_TOOL_TEST_DIR:-$SCRIPT_DIR/../test/tests}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# ============================================================================
# Contract ID Regex Constants (Single Source of Truth)
# ============================================================================

CONTRACT_ID_RE='[a-z][a-z0-9-]*-[0-9]+[a-z]*'
BRACKETED_CONTRACT_RE="\[${CONTRACT_ID_RE}\]"

# ============================================================================
# Helper Functions
# ============================================================================

print_header() {
    echo -e "\n${BLUE}═══════════════════════════════════════════════════════════════${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}\n"
}

print_success() {
    echo -e "${GREEN}✓${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

print_info() {
    echo -e "${BLUE}ℹ${NC} $1"
}

# Get all spec files (numbered markdown files in contracts/)
get_spec_files() {
    find "$CONTRACTS_DIR" -maxdepth 1 -name '*.md' -type f \
        | grep -E '/[0-9]+_' \
        | sort -t'/' -k2 -V
}

# Extract title from a spec file (first # heading)
get_title() {
    local file="$1"
    grep -m1 '^# ' "$file" | sed 's/^# //' | sed 's/^Spec: //'
}

# Generate GitHub-style anchor from title
make_anchor() {
    echo "$1" | tr '[:upper:]' '[:lower:]' | sed 's/[^a-z0-9 -]//g' | sed 's/ /-/g'
}

# Extract spec slug from filename (e.g., "6_entity_scopes.md" -> "entity-scopes")
get_spec_slug() {
    local file="$1"
    basename "$file" | sed -E 's/^[0-9]+_//' | sed 's/\.md$//' | tr '_' '-'
}

# Extract all contract IDs from a line (handles multiple [id], [id], [id] patterns)
# Uses grep -P if available, otherwise falls back to perl
# Set SPEC_TOOL_FORCE_PERL=1 to force perl path for testing
extract_contract_ids() {
    local line="$1"

    # Force perl path if testing, otherwise try grep -P first (faster)
    if [[ "${SPEC_TOOL_FORCE_PERL:-0}" == "1" ]]; then
        # Forced perl path for testing
        echo "$line" | perl -nle "print for /\\[${CONTRACT_ID_RE}\\]/g" | tr -d '[]'
    elif echo "$line" | grep -P "$BRACKETED_CONTRACT_RE" &>/dev/null; then
        echo "$line" | grep -oP "$BRACKETED_CONTRACT_RE" | tr -d '[]'
    else
        # Fallback to perl (more portable)
        echo "$line" | perl -nle "print for /\\[${CONTRACT_ID_RE}\\]/g" | tr -d '[]'
    fi
}

# Find all test files containing a specific contract ID
find_test_files_for_contract() {
    local contract_id="$1"

    # Search for contract ID anywhere on a Contract: line
    grep -rlE "Contract:.*\[${contract_id}\]" "$TEST_DIR"/*.rs 2>/dev/null \
        | xargs -r basename -a \
        | sed 's/\.rs$//' \
        | sort -u
}

# ============================================================================
# Command: help
# ============================================================================

cmd_help() {
    cat << 'EOF'
spec_tool.sh - Comprehensive CLI for Naia specifications management

USAGE:
    ./spec_tool.sh <command> [options]

COMMANDS:
    bundle [output]     Generate NAIA_SPECS.md bundle
                        Options: --no-template (exclude template)

    lint                Check all specs for consistency issues
                        - Title format (Spec: prefix)
                        - Contract ID format
                        - Test obligation format
                        - Terminology consistency

    validate            Run all validation checks (lint + check-refs + check-orphans)

    registry [output]   Extract all contract IDs to registry file
                        Default output: CONTRACT_REGISTRY.md

    check-orphans       Find MUST/MUST NOT statements without contract IDs

    check-refs          Verify all cross-reference links resolve

    stats               Show statistics about specifications

    coverage            Analyze contract test coverage
                        Shows which contracts have test annotations

    gen-test <id>       Generate test skeleton for a contract
                        Example: ./spec_tool.sh gen-test entity-scopes-07

    traceability [out]  Generate contract-to-test traceability matrix
                        Default output: TRACEABILITY.md

    packet <id> [opts]  Generate contract review packet (spec + tests)
                        Options:
                          --out <path>      Output path (default: packets/<id>.md)
                          --full-tests      Include full test bodies (default: assertions only)
                        Example: ./spec_tool.sh packet connection-01

    verify [options]    CI-grade verification: validate + lint + tests + coverage
                        Options:
                          --contract <id>       Run tests only for specific contract
                          --strict-orphans      Fail if orphan MUSTs exist
                          --strict-coverage     Fail if any contracts uncovered
                          --full-report         Include full reports with --contract
                          --write-report <path> Write summary to file

    help                Show this help message

EXAMPLES:
    ./spec_tool.sh bundle                    # Generate NAIA_SPECS.md
    ./spec_tool.sh lint                      # Check for issues
    ./spec_tool.sh validate                  # Full validation
    ./spec_tool.sh registry                  # Generate contract registry
    ./spec_tool.sh stats                     # Show spec statistics

EOF
}

# ============================================================================
# Command: bundle
# ============================================================================

cmd_bundle() {
    local output_file="${1:-$GENERATED_DIR/NAIA_SPECS.md}"

    print_header "Generating NAIA_SPECS.md Bundle"

    # Collect spec files
    mapfile -t SPEC_FILES < <(get_spec_files)

    print_info "Found ${#SPEC_FILES[@]} specification files"

    # Start building output
    {
        # Header
        cat << EOF
# Naia Specifications Bundle

This document contains all normative specifications for the Naia networking engine, concatenated into a single reference.

**Generated:** $(date -u +"%Y-%m-%d %H:%M UTC")
**Spec Count:** ${#SPEC_FILES[@]}

---

## Table of Contents

EOF

        # Generate TOC
        for file in "${SPEC_FILES[@]}"; do
            title=$(get_title "$file")
            anchor=$(make_anchor "$title")
            basename_file=$(basename "$file")
            spec_num=$(echo "$basename_file" | grep -oE '^[0-9]+')
            echo "- [$spec_num. $title](#$anchor)"
        done

        echo ""
        echo "---"
        echo ""

        # Concatenate each spec with separators
        for file in "${SPEC_FILES[@]}"; do
            basename_file=$(basename "$file")

            echo "<!-- ======================================================================== -->"
            echo "<!-- Source: $basename_file -->"
            echo "<!-- ======================================================================== -->"
            echo ""

            cat "$file"

            echo ""
            echo ""
            echo "---"
            echo ""
        done

    } > "$output_file"

    print_success "Generated: $output_file"
    echo ""
    echo "Included specifications:"
    for file in "${SPEC_FILES[@]}"; do
        echo "  - $(basename "$file")"
    done
}

# ============================================================================
# Command: lint
# ============================================================================

cmd_lint() {
    print_header "Linting Specifications"

    local issues=0
    local warnings=0

    mapfile -t SPEC_FILES < <(get_spec_files)

    # Check 1: Title format inconsistency (Spec: prefix)
    echo "Checking title format..."
    for file in "${SPEC_FILES[@]}"; do
        if grep -q '^# Spec:' "$file"; then
            print_warning "$(basename "$file"): Has 'Spec:' prefix in title (should be removed)"
            ((warnings++)) || true
        fi
    done

    # Check 2: Contract ID format variants
    echo "Checking contract ID formats..."
    for file in "${SPEC_FILES[@]}"; do
        local basename_file=$(basename "$file")

        # Format 1: > contract-id (MUST):
        local format1
        format1=$(grep -c '> [a-z-]*-[0-9]* (MUST' "$file" 2>/dev/null) || format1=0

        # Format 2: ### contract-id — (without brackets)
        local format2
        format2=$(grep -cE '^### [a-z]+(-[a-z]+)*-[0-9]+ — ' "$file" 2>/dev/null) || format2=0

        # Format 3: **contract-id**:
        local format3
        format3=$(grep -cE '^\*\*[a-z-]+-[0-9]+[a-z]*\*\*:' "$file" 2>/dev/null) || format3=0

        # Target format: ### [contract-id] —
        local target
        target=$(grep -cE '^### \[[a-z-]+-[0-9]+[a-z]*\] — ' "$file" 2>/dev/null) || target=0

        if [[ $format1 -gt 0 ]]; then
            print_warning "$basename_file: Has $format1 contract IDs in format '> id (MUST):' (migrate to '### [id] —')"
            ((warnings++)) || true
        fi
        if [[ $format2 -gt 0 ]]; then
            print_warning "$basename_file: Has $format2 contract IDs without brackets (add brackets: '### [id] —')"
            ((warnings++)) || true
        fi
        if [[ $format3 -gt 0 ]]; then
            print_warning "$basename_file: Has $format3 contract IDs in bold format (migrate to '### [id] —')"
            ((warnings++)) || true
        fi
    done

    # Check 3: Cross-reference filename format
    echo "Checking cross-reference formats..."
    for file in "${SPEC_FILES[@]}"; do
        local basename_file=$(basename "$file")
        # Find refs without numeric prefix
        local bad_refs=$(grep -oE '`[a-z_]+\.md`' "$file" 2>/dev/null | grep -v '[0-9]_' | sort -u)
        if [[ -n "$bad_refs" ]]; then
            print_warning "$basename_file: Cross-refs missing numeric prefix:"
            echo "$bad_refs" | while read ref; do
                echo "    $ref"
            done
            ((warnings++)) || true
        fi
    done

    # Check 4: Terminology consistency (Debug vs Diagnostics)
    echo "Checking terminology consistency..."
    local debug_files=$(grep -l 'In Debug:' "$CONTRACTS_DIR"/*.md 2>/dev/null | wc -l)
    local diag_files=$(grep -l 'diagnostics.*enabled' "$CONTRACTS_DIR"/*.md 2>/dev/null | wc -l)
    if [[ $debug_files -gt 0 && $diag_files -gt 0 ]]; then
        print_warning "Mixed terminology: $debug_files files use 'Debug', $diag_files use 'Diagnostics'"
        ((warnings++)) || true
    fi

    # Check 5: Missing test obligations section
    echo "Checking for test obligations sections..."
    for file in "${SPEC_FILES[@]}"; do
        local basename_file=$(basename "$file")
        if ! grep -qiE '^## ([0-9]+\) )?Test [Oo]bligations' "$file"; then
            print_warning "$basename_file: Missing '## Test obligations' section"
            ((warnings++)) || true
        fi
    done

    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    if [[ $issues -eq 0 && $warnings -eq 0 ]]; then
        print_success "All checks passed!"
    else
        echo -e "Results: ${RED}$issues errors${NC}, ${YELLOW}$warnings warnings${NC}"
    fi

    return $issues
}

# ============================================================================
# Command: check-refs
# ============================================================================

cmd_check_refs() {
    print_header "Checking Cross-References"

    local errors=0

    mapfile -t SPEC_FILES < <(get_spec_files)

    # Build list of valid spec names (with and without numeric prefix)
    declare -A valid_specs
    for file in "${SPEC_FILES[@]}"; do
        local basename_file=$(basename "$file")
        valid_specs["$basename_file"]=1
        # Also add version without numeric prefix
        local no_prefix=$(echo "$basename_file" | sed -E 's/^[0-9]+_//')
        valid_specs["$no_prefix"]=1
    done

    echo "Validating cross-references in ${#SPEC_FILES[@]} files..."
    echo ""

    for file in "${SPEC_FILES[@]}"; do
        local basename_file=$(basename "$file")

        # Extract all .md references
        local refs=$(grep -oE '`[a-zA-Z0-9_]+\.md`' "$file" 2>/dev/null | tr -d '`' | sort -u)

        for ref in $refs; do
            # Check if reference exists
            if [[ -z "${valid_specs[$ref]}" ]]; then
                # Try to find with numeric prefix
                local found=0
                for valid in "${!valid_specs[@]}"; do
                    if [[ "$valid" == *"$ref" ]]; then
                        found=1
                        print_warning "$basename_file: '$ref' should be '$valid'"
                        break
                    fi
                done
                if [[ $found -eq 0 ]]; then
                    print_error "$basename_file: Invalid reference '$ref' (file not found)"
                    ((errors++)) || true
                fi
            fi
        done
    done

    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    if [[ $errors -eq 0 ]]; then
        print_success "All cross-references valid!"
    else
        print_error "$errors invalid references found"
    fi

    return $errors
}

# ============================================================================
# Command: check-orphans
# ============================================================================

cmd_check_orphans() {
    print_header "Checking for Orphan MUST/MUST NOT Statements"

    local orphans=0

    mapfile -t SPEC_FILES < <(get_spec_files)

    for file in "${SPEC_FILES[@]}"; do
        local basename_file=$(basename "$file")
        local file_orphans=0

        # Find all lines with MUST or MUST NOT
        local line_num=0
        while IFS= read -r line; do
            ((line_num++)) || true

            # Check if line contains MUST (but not in code blocks or comments)
            if echo "$line" | grep -qE '\bMUST\b' && ! echo "$line" | grep -qE '^(```|<!--|    )'; then
                # Check if there's a contract ID nearby (within 10 lines before)
                local context_start=$((line_num - 10))
                [[ $context_start -lt 1 ]] && context_start=1

                local has_contract_id=$(sed -n "${context_start},${line_num}p" "$file" | grep -cE '\[[a-z-]+-[0-9]+[a-z]*\]|[a-z-]+-[0-9]+[a-z]* —|> [a-z-]+-[0-9]+[a-z]*|\*\*[a-z-]+-[0-9]+[a-z]*\*\*:')

                if [[ $has_contract_id -eq 0 ]]; then
                    # Check if it's in a definition/glossary section (allowed)
                    local section=$(sed -n "1,${line_num}p" "$file" | grep -E '^## ' | tail -1)
                    # Also skip if line itself is normative keyword declaration
                    if echo "$line" | grep -qiE '^Normative keywords:'; then
                        continue
                    fi
                    if ! echo "$section" | grep -qiE '(glossary|vocabulary|definition|scope|normative)'; then
                        if [[ $file_orphans -eq 0 ]]; then
                            echo ""
                            echo "$basename_file:"
                        fi
                        print_warning "  Line $line_num: $(echo "$line" | head -c 80)..."
                        ((file_orphans++)) || true
                        ((orphans++)) || true
                    fi
                fi
            fi
        done < "$file"
    done

    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    if [[ $orphans -eq 0 ]]; then
        print_success "No orphan MUST/MUST NOT statements found!"
    else
        print_warning "$orphans potential orphan statements found (review manually)"
    fi

    return 0  # Don't fail on orphans, they need manual review
}

# ============================================================================
# Command: registry
# ============================================================================

cmd_registry() {
    local output_file="${1:-$GENERATED_DIR/CONTRACT_REGISTRY.md}"

    print_header "Generating Contract Registry"

    mapfile -t SPEC_FILES < <(get_spec_files)

    local total_contracts=0
    declare -A contracts_by_spec

    # Extract contracts from each file
    for file in "${SPEC_FILES[@]}"; do
        local basename_file=$(basename "$file")
        local spec_slug=$(get_spec_slug "$file")
        local file_contracts=()

        # Pattern 1: ### [contract-id] — (supports alphanumeric suffixes like -03a)
        while IFS= read -r line; do
            local id=$(echo "$line" | grep -oE '\[[a-z-]+-[0-9]+[a-z]*\]' | tr -d '[]')
            [[ -n "$id" ]] && file_contracts+=("$id")
        done < <(grep -E '### \[[a-z-]+-[0-9]+[a-z]*\]' "$file" 2>/dev/null)

        # Pattern 2: ### contract-id —
        while IFS= read -r line; do
            local id=$(echo "$line" | grep -oE '^### [a-z-]+-[0-9]+[a-z]*' | sed 's/^### //')
            [[ -n "$id" ]] && file_contracts+=("$id")
        done < <(grep -E '^### [a-z-]+-[0-9]+[a-z]* — ' "$file" 2>/dev/null)

        # Pattern 3: > contract-id (MUST
        while IFS= read -r line; do
            local id=$(echo "$line" | grep -oE '> [a-z-]+-[0-9]+[a-z]*' | sed 's/^> //')
            [[ -n "$id" ]] && file_contracts+=("$id")
        done < <(grep -E '> [a-z-]+-[0-9]+[a-z]* \(MUST' "$file" 2>/dev/null)

        # Pattern 4: **contract-id**:
        while IFS= read -r line; do
            local id=$(echo "$line" | grep -oE '\*\*[a-z-]+-[0-9]+[a-z]*\*\*' | tr -d '*')
            [[ -n "$id" ]] && file_contracts+=("$id")
        done < <(grep -E '^\*\*[a-z-]+-[0-9]+[a-z]*\*\*:' "$file" 2>/dev/null)

        # Remove duplicates and sort
        local unique_contracts=$(printf '%s\n' "${file_contracts[@]}" | sort -u -V)
        contracts_by_spec["$basename_file"]="$unique_contracts"
        local count
        count=$(echo "$unique_contracts" | grep -c '[a-z]') || count=0
        total_contracts=$((total_contracts + count))
    done

    # Generate registry file
    {
        cat << EOF
# Contract ID Registry

**Generated:** $(date -u +"%Y-%m-%d %H:%M UTC")
**Total Contracts:** $total_contracts

---

## Summary by Specification

| Spec File | Contract Count | ID Range |
|-----------|----------------|----------|
EOF

        for file in "${SPEC_FILES[@]}"; do
            local basename_file=$(basename "$file")
            local contracts="${contracts_by_spec[$basename_file]}"
            local count
            count=$(echo "$contracts" | grep -c '[a-z]' 2>/dev/null) || count=0
            local first=$(echo "$contracts" | head -1)
            local last=$(echo "$contracts" | tail -1)
            if [[ $count -gt 0 ]]; then
                echo "| $basename_file | $count | $first → $last |"
            fi
        done

        echo ""
        echo "---"
        echo ""
        echo "## Full Contract Index"
        echo ""

        for file in "${SPEC_FILES[@]}"; do
            local basename_file=$(basename "$file")
            local title=$(get_title "$file")
            local contracts="${contracts_by_spec[$basename_file]}"
            local count
            count=$(echo "$contracts" | grep -c '[a-z]' 2>/dev/null) || count=0

            if [[ $count -gt 0 ]]; then
                echo "### $title ($basename_file)"
                echo ""
                echo "$contracts" | while read -r id; do
                    [[ -n "$id" ]] && echo "- \`$id\`"
                done
                echo ""
            fi
        done

    } > "$output_file"

    print_success "Generated: $output_file"
    print_info "Total contracts: $total_contracts"
}

# ============================================================================
# Command: stats
# ============================================================================

cmd_stats() {
    print_header "Specification Statistics"

    mapfile -t SPEC_FILES < <(get_spec_files)

    local total_lines=0
    local total_words=0
    local total_contracts=0

    echo "Per-file statistics:"
    echo ""
    printf "%-35s %8s %8s %10s\n" "File" "Lines" "Words" "Contracts"
    printf "%-35s %8s %8s %10s\n" "----" "-----" "-----" "---------"

    for file in "${SPEC_FILES[@]}"; do
        local basename_file=$(basename "$file")
        local lines=$(wc -l < "$file")
        local words=$(wc -w < "$file")
        local contracts=$(grep -cE '(\[[a-z-]+-[0-9]+[a-z]*\]|^### [a-z]+(-[a-z]+)*-[0-9]+[a-z]* |> [a-z-]+-[0-9]+[a-z]* \(MUST|\*\*[a-z-]+-[0-9]+[a-z]*\*\*)' "$file" 2>/dev/null || true)
        [[ -z "$contracts" ]] && contracts=0

        printf "%-35s %8d %8d %10d\n" "$basename_file" "$lines" "$words" "$contracts"

        total_lines=$((total_lines + lines))
        total_words=$((total_words + words))
        total_contracts=$((total_contracts + contracts))
    done

    echo ""
    printf "%-35s %8s %8s %10s\n" "----" "-----" "-----" "---------"
    printf "%-35s %8d %8d %10d\n" "TOTAL" "$total_lines" "$total_words" "$total_contracts"

    echo ""
    echo "Additional metrics:"
    local num_specs=${#SPEC_FILES[@]}
    echo "  - Spec files: $num_specs"
    if [[ $num_specs -gt 0 ]]; then
        echo "  - Average lines per spec: $((total_lines / num_specs))"
        echo "  - Average contracts per spec: $((total_contracts / num_specs))"
    fi
}

# ============================================================================
# Command: validate (runs all checks)
# ============================================================================

cmd_validate() {
    print_header "Full Specification Validation"

    local total_errors=0

    echo "Running lint..."
    cmd_lint || ((total_errors += $?))

    echo ""
    echo "Running check-refs..."
    cmd_check_refs || ((total_errors += $?))

    echo ""
    echo "Running check-orphans..."
    cmd_check_orphans

    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    if [[ $total_errors -eq 0 ]]; then
        print_success "All validation checks passed!"
    else
        print_error "Validation failed with $total_errors errors"
    fi

    return $total_errors
}

# ============================================================================
# Command: coverage
# ============================================================================

cmd_coverage() {
    print_header "Contract Coverage Analysis"

    if [[ ! -d "$TEST_DIR" ]]; then
        print_error "Test directory not found: $TEST_DIR"
        return 1
    fi

    # Extract contract IDs from test annotations
    # Find lines with Contract: then extract ALL [contract-id] patterns from those lines
    # Uses -oP (PCRE) to capture all bracket patterns, not just first per line
    local test_contracts=$(grep -rhE '(///|//) Contract:' "$TEST_DIR"/*.rs 2>/dev/null \
        | grep -oP '\[[a-z][a-z0-9-]*-[0-9]+[a-z]*\]' | tr -d '[]' | sort -u)

    # Get all contracts from registry (supports alphanumeric suffixes like -03a)
    local all_contracts=$(grep -oE '`[a-z-]+-[0-9]+[a-z]*`' "$GENERATED_DIR/CONTRACT_REGISTRY.md" 2>/dev/null \
        | tr -d '`' | sort -u)

    local covered_count=0
    local total_count=0

    if [[ -n "$test_contracts" ]]; then
        covered_count=$(echo "$test_contracts" | grep -c '[a-z]' 2>/dev/null || true)
        [[ -z "$covered_count" ]] && covered_count=0
    fi

    if [[ -n "$all_contracts" ]]; then
        total_count=$(echo "$all_contracts" | grep -c '[a-z]' 2>/dev/null || true)
        [[ -z "$total_count" ]] && total_count=0
    fi

    if [[ $total_count -eq 0 ]]; then
        print_error "No contracts found. Run ./spec_tool.sh registry first."
        return 1
    fi

    local coverage_pct=$((covered_count * 100 / total_count))

    echo "Coverage Summary"
    echo "━━━━━━━━━━━━━━━━"
    echo "Contracts with test annotations: $covered_count"
    echo "Total contracts in registry:     $total_count"
    echo "Coverage:                        $coverage_pct%"
    echo ""

    # Find uncovered contracts
    local uncovered=$(comm -23 <(echo "$all_contracts") <(echo "$test_contracts") 2>/dev/null)
    local uncovered_count=0
    if [[ -n "$uncovered" ]]; then
        uncovered_count=$(echo "$uncovered" | grep -c '[a-z]' 2>/dev/null || true)
        [[ -z "$uncovered_count" ]] && uncovered_count=0
    fi

    if [[ $uncovered_count -gt 0 ]]; then
        echo "Uncovered Contracts ($uncovered_count):"
        echo "━━━━━━━━━━━━━━━━━━━━"
        echo "$uncovered" | while read -r id; do
            [[ -n "$id" ]] && echo "  - $id"
        done
    else
        print_success "All contracts have test annotations!"
    fi

    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    if [[ $coverage_pct -ge 80 ]]; then
        print_success "Coverage target met (≥80%)"
    else
        print_warning "Coverage below target (<80%)"
    fi
}

# ============================================================================
# Command: gen-test
# ============================================================================

cmd_gen_test() {
    local contract_id="$1"

    if [[ -z "$contract_id" ]]; then
        print_error "Usage: ./spec_tool.sh gen-test <contract-id>"
        echo "Example: ./spec_tool.sh gen-test entity-scopes-07"
        return 1
    fi

    # Find the spec file containing this contract
    local spec_file=$(grep -l "\[$contract_id\]" "$CONTRACTS_DIR"/*.md 2>/dev/null | grep -v REGISTRY | head -1)

    if [[ -z "$spec_file" ]]; then
        print_error "Contract [$contract_id] not found in any spec file"
        return 1
    fi

    local spec_basename=$(basename "$spec_file")
    print_info "Found contract in: $spec_basename"
    echo ""

    # Extract contract section (up to next ### or ## or end of file)
    local contract_content=$(sed -n "/### \[$contract_id\]/,/^##/p" "$spec_file" | head -n -1)

    if [[ -z "$contract_content" ]]; then
        # Try alternative format (without brackets)
        contract_content=$(sed -n "/### $contract_id —/,/^##/p" "$spec_file" | head -n -1)
    fi

    # Extract title
    local title=$(echo "$contract_content" | head -1 | sed 's/^### //' | sed 's/\[.*\] — //' | sed 's/ — .*//')

    # Convert contract-id to function name (replace - with _)
    local fn_name=$(echo "$contract_id" | tr '-' '_')

    cat << EOF
/// Contract: [$contract_id]
/// Source: $spec_basename
///
/// Guarantee: TODO - Copy from spec
///
/// Scenario: TODO - Describe Given/When/Then
/// Given:
///   - TODO: Initial conditions
/// When:
///   - TODO: Trigger action
/// Then:
///   - TODO: Expected outcome
#[test]
fn ${fn_name}_scenario_1() {
    use naia_server::ServerConfig;
    use naia_test::{protocol, Auth, Scenario};

    let mut scenario = Scenario::new();
    let test_protocol = protocol();

    scenario.server_start(ServerConfig::default(), test_protocol.clone());

    let room_key = scenario.mutate(|ctx| {
        ctx.server(|server| server.make_room().key())
    });

    // TODO: Connect clients as needed
    // let client_key = client_connect(&mut scenario, &room_key, "Client", Auth::new("user", "pass"), test_client_config(), test_protocol);

    // TODO: Setup preconditions (Given)
    scenario.mutate(|ctx| {
        // Setup
    });

    // TODO: Trigger action (When)
    scenario.mutate(|ctx| {
        // Action
    });

    // TODO: Verify postconditions (Then)
    scenario.expect(|ctx| {
        // Assertion
        todo!("Implement assertion for [$contract_id]")
    });
}
EOF
}

# ============================================================================
# Command: traceability
# ============================================================================

cmd_traceability() {
    local output_file="${1:-$GENERATED_DIR/TRACEABILITY.md}"

    print_header "Generating Traceability Matrix"

    {
        cat << EOF
# Contract Traceability Matrix

**Generated:** $(date -u +"%Y-%m-%d %H:%M UTC")

This matrix shows the bidirectional mapping between contracts and tests.

---

## Contracts → Tests

| Contract | Test Function | Test File | Status |
|----------|---------------|-----------|--------|
EOF

        # Get all contracts from registry (supports alphanumeric suffixes like -03a)
        local all_contracts=$(grep -oE '`[a-z-]+-[0-9]+[a-z]*`' "$GENERATED_DIR/CONTRACT_REGISTRY.md" 2>/dev/null \
            | tr -d '`' | sort -u)

        echo "$all_contracts" | while read -r contract; do
            [[ -z "$contract" ]] && continue

            # Search for this contract in test files (match anywhere on Contract: line)
            local test_match=$(grep -rlE "Contract:.*\[$contract\]" "$TEST_DIR"/*.rs 2>/dev/null | head -1)

            if [[ -n "$test_match" ]]; then
                local test_file=$(basename "$test_match")
                # Try to find the function name (line after Contract annotation containing this contract)
                local fn_name=$(grep -A5 -E "Contract:.*\[$contract\]" "$test_match" 2>/dev/null \
                    | grep -oE '^fn [a-z_0-9]+' | head -1 | sed 's/^fn //')
                [[ -z "$fn_name" ]] && fn_name="(manual check)"
                echo "| \`$contract\` | \`$fn_name\` | $test_file | COVERED |"
            else
                echo "| \`$contract\` | - | - | **UNCOVERED** |"
            fi
        done

        cat << EOF

---

## Tests → Contracts

| Test File | Test Function | Contracts Verified |
|-----------|---------------|--------------------|
EOF

        # List tests with their contracts
        for test_file in "$TEST_DIR"/*.rs; do
            [[ ! -f "$test_file" ]] && continue
            local basename_file=$(basename "$test_file")

            # Find all contract annotations in this file (extract ALL IDs from Contract: lines)
            # Uses -oP (PCRE) to capture all bracket patterns, not just first per line
            local contracts_in_file=$(grep -E 'Contract:' "$test_file" 2>/dev/null \
                | grep -oP '\[[a-z][a-z0-9-]*-[0-9]+\]' | tr -d '[]' | sort -u | tr '\n' ', ' | sed 's/, $//')

            if [[ -n "$contracts_in_file" ]]; then
                # Get function names
                local fn_names=$(grep -B5 'Contract: \[' "$test_file" 2>/dev/null \
                    | grep -oE '^fn [a-z_0-9]+' | sed 's/^fn //' | sort -u | head -5 | tr '\n' ', ' | sed 's/, $//')
                [[ -z "$fn_names" ]] && fn_names="(check manually)"
                echo "| $basename_file | $fn_names | $contracts_in_file |"
            fi
        done

        cat << EOF

---

## Summary

EOF
        local total=$(echo "$all_contracts" | grep -c '[a-z]' 2>/dev/null || echo 0)
        local covered=$(grep -rhE 'Contract:' "$TEST_DIR"/*.rs 2>/dev/null \
            | grep -oP '\[[a-z][a-z0-9-]*-[0-9]+\]' | tr -d '[]' | sort -u | grep -c '[a-z]' 2>/dev/null || echo 0)
        local pct=$((covered * 100 / total))

        echo "- **Total Contracts:** $total"
        echo "- **Contracts with Tests:** $covered"
        echo "- **Coverage:** $pct%"

    } > "$output_file"

    print_success "Generated: $output_file"
}

# ============================================================================
# Command: packet
# ============================================================================

cmd_packet() {
    local contract_id="$1"
    shift || true

    local output_file=""
    local full_tests=0

    # Parse options
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --out)
                output_file="$2"
                shift 2
                ;;
            --full-tests)
                full_tests=1
                shift
                ;;
            *)
                print_error "Unknown option: $1"
                return 1
                ;;
        esac
    done

    if [[ -z "$contract_id" ]]; then
        print_error "Usage: ./spec_tool.sh packet <contract-id> [--out <path>] [--full-tests]"
        echo "Example: ./spec_tool.sh packet connection-01"
        return 1
    fi

    # Set default output path
    if [[ -z "$output_file" ]]; then
        output_file="$GENERATED_DIR/packets/${contract_id}.md"
        mkdir -p "$GENERATED_DIR/packets"
    fi

    print_header "Generating Contract Review Packet: $contract_id"

    # Step 1: Find spec file containing the contract
    print_info "Searching for contract [$contract_id] in spec files..."
    local spec_file=$(grep -l "### \[$contract_id\]" "$CONTRACTS_DIR"/*.md 2>/dev/null | head -1)

    if [[ -z "$spec_file" ]]; then
        print_error "Contract [$contract_id] not found in any spec file"
        return 1
    fi

    local spec_basename=$(basename "$spec_file")
    local spec_title=$(get_title "$spec_file")
    print_success "Found in: $spec_basename"

    # Step 2: Extract spec excerpt
    print_info "Extracting spec excerpt..."
    local spec_excerpt=""
    local in_contract=0
    local line_num=0

    while IFS= read -r line; do
        ((line_num++)) || true

        # Start capturing at contract heading
        if [[ "$line" =~ ^###[[:space:]]\[$contract_id\] ]]; then
            in_contract=1
            spec_excerpt="$line"
            continue
        fi

        # Stop at next contract heading or section heading
        if [[ $in_contract -eq 1 ]]; then
            if [[ "$line" =~ ^###[[:space:]] ]] || [[ "$line" =~ ^##[[:space:]] ]]; then
                break
            fi
            spec_excerpt="${spec_excerpt}"$'\n'"${line}"
        fi
    done < "$spec_file"

    if [[ -z "$spec_excerpt" ]]; then
        print_error "Failed to extract spec excerpt for [$contract_id]"
        return 1
    fi

    # Step 3: Find tests referencing this contract
    print_info "Searching for tests covering [$contract_id]..."
    local test_files=$(grep -rlE "Contract:.*\[$contract_id\]" "$TEST_DIR"/*.rs 2>/dev/null || true)
    local test_count=0

    if [[ -n "$test_files" ]]; then
        test_count=$(echo "$test_files" | wc -l | tr -d ' ')
    fi

    if [[ $test_count -eq 0 ]]; then
        print_warning "No tests found for contract [$contract_id]"
    else
        print_success "Found tests in $test_count file(s)"
    fi

    # Step 4: Generate output file
    print_info "Writing packet to: $output_file"

    {
        # Header
        cat << EOF
# Contract Review Packet: $contract_id

**Generated:** $(date -u +"%Y-%m-%d %H:%M UTC")
**Spec File:** $spec_basename
**Test Files:** $test_count

---

## Spec: $spec_title

**Source:** \`$spec_basename\`

EOF

        # Spec excerpt
        echo '```'
        echo "$spec_excerpt"
        echo '```'
        echo ""
        echo "---"
        echo ""

        # Tests section
        echo "## Tests"
        echo ""

        if [[ $test_count -eq 0 ]]; then
            echo "**⚠️ WARNING:** No tests found for this contract."
            echo ""
        else
            # Process each test file
            while IFS= read -r test_file; do
                [[ -z "$test_file" ]] && continue

                local test_basename=$(basename "$test_file")
                echo "### Test File: \`$test_basename\`"
                echo ""

                # Find all test functions that reference this contract
                local in_doc_comment=0
                local doc_comment=""
                local fn_signature=""
                local fn_body=""
                local test_attrs=""
                local in_function=0
                local brace_depth=0

                while IFS= read -r line; do
                    # Check for doc comment line
                    if [[ "$line" =~ ^///[[:space:]]?Contract:.*\[$contract_id\] ]]; then
                        in_doc_comment=1
                        doc_comment="$line"
                        continue
                    fi

                    # Continue collecting doc comments
                    if [[ $in_doc_comment -eq 1 && "$line" =~ ^/// ]]; then
                        doc_comment="${doc_comment}"$'\n'"${line}"
                        continue
                    fi

                    # Collect #[test] or #[...] attributes after doc comment
                    if [[ $in_doc_comment -eq 1 && "$line" =~ ^#\[ ]]; then
                        test_attrs="${test_attrs}${line}"$'\n'
                        continue
                    fi

                    # Check for function signature after doc comment
                    if [[ $in_doc_comment -eq 1 && "$line" =~ ^fn[[:space:]] ]]; then
                        fn_signature="$line"
                        in_function=1
                        brace_depth=0
                        fn_body=""

                        # Count opening braces on the signature line
                        local open_braces=$(echo "$line" | grep -o '{' | wc -l)
                        brace_depth=$((brace_depth + open_braces))
                        continue
                    fi

                    # Collect function body
                    if [[ $in_function -eq 1 ]]; then
                        # Count braces
                        local open_braces=$(echo "$line" | grep -o '{' | wc -l)
                        local close_braces=$(echo "$line" | grep -o '}' | wc -l)
                        brace_depth=$((brace_depth + open_braces - close_braces))

                        fn_body="${fn_body}${line}"$'\n'

                        # Function complete when braces balanced
                        if [[ $brace_depth -eq 0 ]]; then
                            # Output the test
                            echo '```rust'
                            echo "$doc_comment"
                            echo "$test_attrs"
                            echo "$fn_signature"

                            if [[ $full_tests -eq 1 ]]; then
                                # Full test mode: output entire body
                                echo "$fn_body"
                            else
                                # Concise mode: extract assertion index
                                echo ""
                                echo "    // Assertion Index:"

                                # Extract expect_msg strings
                                local expect_msgs=$(echo "$fn_body" | grep -oE '\.?expect_msg\("([^"\\]|\\.)*"\)' | sed 's/^\.//; s/expect_msg(//; s/)$//' | sort -u)

                                if [[ -n "$expect_msgs" ]]; then
                                    echo "$expect_msgs" | while IFS= read -r msg; do
                                        [[ -n "$msg" ]] && echo "    //   - expect_msg($msg)"
                                    done
                                else
                                    # No expect_msg found, show counts
                                    local expect_count=$(echo "$fn_body" | grep -c 'scenario\.expect(' || echo 0)
                                    local until_count=$(echo "$fn_body" | grep -c 'scenario\.until(' || echo 0)
                                    echo "    //   NOTE: No expect_msg labels found. Add expect_msg(...) to make adequacy review deterministic."
                                    echo "    //   Signal: ${expect_count}x scenario.expect(), ${until_count}x scenario.until()"
                                fi
                                echo ""
                                echo "    // ... (use --full-tests to see complete body) ..."
                            fi
                            echo '```'
                            echo ""

                            # Reset state
                            in_doc_comment=0
                            in_function=0
                            doc_comment=""
                            fn_signature=""
                            fn_body=""
                            test_attrs=""
                        fi
                    fi

                    # Reset if we hit a non-related line
                    if [[ $in_doc_comment -eq 1 && $in_function -eq 0 && ! "$line" =~ ^/// && ! "$line" =~ ^#\[ && ! "$line" =~ ^fn[[:space:]] ]]; then
                        in_doc_comment=0
                        doc_comment=""
                        test_attrs=""
                    fi
                done < "$test_file"

                echo ""
            done <<< "$test_files"
        fi

        # Footer
        cat << EOF

---

**Note:** This packet was generated for contract adequacy review.
- To see full test implementations, use: \`--full-tests\`
- To run tests for this contract: \`./spec_tool.sh verify --contract $contract_id\`
EOF

    } > "$output_file"

    print_success "Packet generated: $output_file"

    # Show summary
    echo ""
    echo "Summary:"
    echo "  Contract:     $contract_id"
    echo "  Spec:         $spec_basename"
    echo "  Test files:   $test_count"
    echo "  Output:       $output_file"

    return 0
}

# ============================================================================
# Command: verify
# ============================================================================

cmd_verify() {
    print_header "Naia Verification Pipeline"

    # Parse options
    local target_contract=""
    local strict_orphans=0
    local strict_coverage=0
    local full_report=0
    local write_report=""

    while [[ $# -gt 0 ]]; do
        case "$1" in
            --contract)
                target_contract="$2"
                shift 2
                ;;
            --strict-orphans)
                strict_orphans=1
                shift
                ;;
            --strict-coverage)
                strict_coverage=1
                shift
                ;;
            --full-report)
                full_report=1
                shift
                ;;
            --write-report)
                write_report="$2"
                shift 2
                ;;
            *)
                print_error "Unknown option: $1"
                return 1
                ;;
        esac
    done

    local total_errors=0
    local test_status="UNKNOWN"

    # Step 1: Validate spec structure
    print_info "Running: validate (spec structure)"
    if ! cmd_validate; then
        print_error "Spec validation failed"
        return 1
    fi

    # Step 2: Check orphans (capture count for summary)
    echo ""
    print_info "Running: check-orphans"
    local orphan_count=0
    local orphan_files=""

    # Capture orphan check output
    local orphan_output
    orphan_output=$(cmd_check_orphans 2>&1)
    echo "$orphan_output"

    # Extract orphan count from output
    if echo "$orphan_output" | grep -q "orphan statements found"; then
        orphan_count=$(echo "$orphan_output" | grep -oE '[0-9]+ potential orphan' | grep -oE '[0-9]+' | head -1)
        [[ -z "$orphan_count" ]] && orphan_count=0
    fi

    if [[ $strict_orphans -eq 1 && $orphan_count -gt 0 ]]; then
        print_error "Strict orphan check failed ($orphan_count orphans)"
        return 1
    fi

    # Step 3: Run tests (targeted or full)
    echo ""
    if [[ -n "$target_contract" ]]; then
        print_info "Running: targeted tests for contract [$target_contract]"

        # Find test files containing this contract
        local test_files
        test_files=$(find_test_files_for_contract "$target_contract")

        if [[ -z "$test_files" ]]; then
            print_error "No test files found for contract [$target_contract]"
            print_info "Contract may be uncovered. Run './spec_tool.sh coverage' to check."
            return 1
        fi

        print_info "Found contract in test files: $(echo "$test_files" | tr '\n' ', ' | sed 's/, $//')"

        # Run tests for each matching file
        local test_failed=0
        while IFS= read -r test_file; do
            [[ -z "$test_file" ]] && continue
            echo ""
            print_info "Running: cargo test -p naia-test --test $test_file -- --nocapture --test-threads=1"
            if ! cargo test -p naia-test --test "$test_file" -- --nocapture --test-threads=1; then
                test_failed=1
            fi
        done <<< "$test_files"

        if [[ $test_failed -eq 1 ]]; then
            test_status="FAIL"
            ((total_errors++)) || true
        else
            test_status="PASS"
        fi

        # Skip coverage/traceability unless --full-report
        if [[ $full_report -eq 0 ]]; then
            echo ""
            echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
            if [[ $total_errors -eq 0 ]]; then
                print_success "VERIFY: PASS (targeted test for [$target_contract])"
            else
                print_error "VERIFY: FAIL (tests failed)"
            fi
            echo ""
            print_info "tests: $test_status"
            print_info "contract: $target_contract"
            print_info "test files: $(echo "$test_files" | wc -l | tr -d ' ')"
            return $total_errors
        fi
    else
        print_info "Running: cargo test -p naia-test -- --nocapture --test-threads=1"
        if cargo test -p naia-test -- --nocapture --test-threads=1; then
            test_status="PASS"
        else
            test_status="FAIL"
            ((total_errors++)) || true
        fi
    fi

    # Step 4: Coverage analysis
    echo ""
    print_info "Running: coverage"

    # Capture coverage output
    local coverage_output
    coverage_output=$(cmd_coverage 2>&1)
    echo "$coverage_output"

    # Extract coverage metrics
    local covered_count=0
    local total_count=0
    local coverage_pct=0
    local uncovered_list=""

    if echo "$coverage_output" | grep -q "Contracts with test annotations:"; then
        covered_count=$(echo "$coverage_output" | grep "Contracts with test annotations:" | grep -oE '[0-9]+' | head -1)
        total_count=$(echo "$coverage_output" | grep "Total contracts in registry:" | grep -oE '[0-9]+' | head -1)
        coverage_pct=$(echo "$coverage_output" | grep "Coverage:" | grep -oE '[0-9]+%' | tr -d '%')

        # Extract uncovered list
        uncovered_list=$(echo "$coverage_output" | sed -n '/^Uncovered Contracts/,/^━/p' | grep '  - ' | sed 's/  - //' | head -30)
    fi

    if [[ $strict_coverage -eq 1 ]]; then
        local uncovered_count=$((total_count - covered_count))
        if [[ $uncovered_count -gt 0 ]]; then
            print_error "Strict coverage check failed ($uncovered_count uncovered)"
            ((total_errors++)) || true
        fi
    fi

    # Step 5: Traceability
    echo ""
    print_info "Running: traceability (regenerating matrix)"
    cmd_traceability >/dev/null

    # Step 6: Final summary
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    if [[ $total_errors -eq 0 ]]; then
        print_success "VERIFY: PASS"
    else
        print_error "VERIFY: FAIL"
    fi

    echo ""
    echo "Summary:"
    echo "  tests:             $test_status"
    echo "  coverage:          ${coverage_pct}% ($covered_count/$total_count)"

    if [[ -n "$uncovered_list" ]]; then
        local uncovered_count
        uncovered_count=$(echo "$uncovered_list" | wc -l | tr -d ' ')
        echo "  uncovered:         $uncovered_count contracts"
        if [[ $uncovered_count -le 30 ]]; then
            echo ""
            echo "Uncovered contracts:"
            echo "$uncovered_list" | head -30 | sed 's/^/    /'
        fi
    else
        echo "  uncovered:         0 contracts"
    fi

    if [[ $orphan_count -gt 0 ]]; then
        if [[ $strict_orphans -eq 1 ]]; then
            echo "  orphans:           $orphan_count (strict mode - FAILED)"
        else
            echo "  orphans:           $orphan_count (non-strict)"
        fi
    else
        echo "  orphans:           0"
    fi

    # Write report file if requested
    if [[ -n "$write_report" ]]; then
        {
            echo "# Naia Verification Report"
            echo ""
            echo "**Generated:** $(date -u +"%Y-%m-%d %H:%M UTC")"
            echo ""
            echo "## Summary"
            echo ""
            echo "- **Overall:** $([ $total_errors -eq 0 ] && echo "PASS" || echo "FAIL")"
            echo "- **Tests:** $test_status"
            echo "- **Coverage:** ${coverage_pct}% ($covered_count/$total_count)"
            echo "- **Uncovered:** $(echo "$uncovered_list" | wc -l | tr -d ' ') contracts"
            echo "- **Orphans:** $orphan_count"
            echo ""
            echo "## Uncovered Contracts"
            echo ""
            if [[ -n "$uncovered_list" ]]; then
                echo "$uncovered_list" | sed 's/^/- /'
            else
                echo "All contracts covered!"
            fi
        } > "$write_report"
        print_info "Report written to: $write_report"
    fi

    return $total_errors
}

# ============================================================================
# Main Entry Point
# ============================================================================

main() {
    local command="${1:-help}"
    shift 2>/dev/null || true

    case "$command" in
        help|--help|-h)
            cmd_help
            ;;
        bundle)
            cmd_bundle "$@"
            ;;
        lint)
            cmd_lint "$@"
            ;;
        validate)
            cmd_validate "$@"
            ;;
        registry)
            cmd_registry "$@"
            ;;
        check-orphans)
            cmd_check_orphans "$@"
            ;;
        check-refs)
            cmd_check_refs "$@"
            ;;
        stats)
            cmd_stats "$@"
            ;;
        coverage)
            cmd_coverage "$@"
            ;;
        gen-test)
            cmd_gen_test "$@"
            ;;
        traceability)
            cmd_traceability "$@"
            ;;
        packet)
            cmd_packet "$@"
            ;;
        verify)
            cmd_verify "$@"
            ;;
        *)
            print_error "Unknown command: $command"
            echo ""
            cmd_help
            exit 1
            ;;
    esac
}

main "$@"
