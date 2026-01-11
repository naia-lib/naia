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
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

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

# Get all spec files (numbered markdown files)
get_spec_files() {
    local include_template="${1:-false}"
    if [[ "$include_template" == "true" ]]; then
        find "$SCRIPT_DIR" -maxdepth 1 -name '*.md' -type f \
            | grep -E '/[0-9]+_' \
            | sort -t'/' -k2 -V
    else
        find "$SCRIPT_DIR" -maxdepth 1 -name '*.md' -type f \
            | grep -E '/[0-9]+_' \
            | grep -v '1_template\.md' \
            | sort -t'/' -k2 -V
    fi
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

# Extract spec slug from filename (e.g., "7_entity_scopes.md" -> "entity-scopes")
get_spec_slug() {
    local file="$1"
    basename "$file" | sed -E 's/^[0-9]+_//' | sed 's/\.md$//' | tr '_' '-'
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
    local output_file="${1:-$SCRIPT_DIR/NAIA_SPECS.md}"
    local include_template="true"

    # Parse options
    for arg in "$@"; do
        case $arg in
            --no-template)
                include_template="false"
                shift
                ;;
        esac
    done

    print_header "Generating NAIA_SPECS.md Bundle"

    # Collect spec files
    mapfile -t SPEC_FILES < <(get_spec_files "$include_template")

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

    mapfile -t SPEC_FILES < <(get_spec_files "false")

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
        format3=$(grep -cE '^\*\*[a-z-]+-[0-9]+\*\*:' "$file" 2>/dev/null) || format3=0

        # Target format: ### [contract-id] —
        local target
        target=$(grep -cE '^### \[[a-z-]+-[0-9]+\] — ' "$file" 2>/dev/null) || target=0

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
    local debug_files=$(grep -l 'In Debug:' "$SCRIPT_DIR"/*.md 2>/dev/null | wc -l)
    local diag_files=$(grep -l 'diagnostics.*enabled' "$SCRIPT_DIR"/*.md 2>/dev/null | wc -l)
    if [[ $debug_files -gt 0 && $diag_files -gt 0 ]]; then
        print_warning "Mixed terminology: $debug_files files use 'Debug', $diag_files use 'Diagnostics'"
        ((warnings++)) || true
    fi

    # Check 5: Missing test obligations section
    echo "Checking for test obligations sections..."
    for file in "${SPEC_FILES[@]}"; do
        local basename_file=$(basename "$file")
        if ! grep -qiE '^## ([0-9]+\) )?Test [Oo]bligations' "$file"; then
            if [[ "$basename_file" != "0_README.md" && "$basename_file" != "1_template.md" ]]; then
                print_warning "$basename_file: Missing '## Test obligations' section"
                ((warnings++)) || true
            fi
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

    mapfile -t SPEC_FILES < <(get_spec_files "true")

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

    mapfile -t SPEC_FILES < <(get_spec_files "false")

    for file in "${SPEC_FILES[@]}"; do
        local basename_file=$(basename "$file")
        local file_orphans=0

        # Skip README and template
        if [[ "$basename_file" == "0_README.md" || "$basename_file" == "1_template.md" ]]; then
            continue
        fi

        # Find all lines with MUST or MUST NOT
        local line_num=0
        while IFS= read -r line; do
            ((line_num++)) || true

            # Check if line contains MUST (but not in code blocks or comments)
            if echo "$line" | grep -qE '\bMUST\b' && ! echo "$line" | grep -qE '^(```|<!--|    )'; then
                # Check if there's a contract ID nearby (within 10 lines before)
                local context_start=$((line_num - 10))
                [[ $context_start -lt 1 ]] && context_start=1

                local has_contract_id=$(sed -n "${context_start},${line_num}p" "$file" | grep -cE '\[[a-z-]+-[0-9]+\]|[a-z-]+-[0-9]+ —|> [a-z-]+-[0-9]+|\*\*[a-z-]+-[0-9]+\*\*:')

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
    local output_file="${1:-$SCRIPT_DIR/CONTRACT_REGISTRY.md}"

    print_header "Generating Contract Registry"

    mapfile -t SPEC_FILES < <(get_spec_files "false")

    local total_contracts=0
    declare -A contracts_by_spec

    # Extract contracts from each file
    for file in "${SPEC_FILES[@]}"; do
        local basename_file=$(basename "$file")
        local spec_slug=$(get_spec_slug "$file")
        local file_contracts=()

        # Pattern 1: ### [contract-id] —
        while IFS= read -r line; do
            local id=$(echo "$line" | grep -oE '\[[a-z-]+-[0-9]+\]' | tr -d '[]')
            [[ -n "$id" ]] && file_contracts+=("$id")
        done < <(grep -E '### \[[a-z-]+-[0-9]+\]' "$file" 2>/dev/null)

        # Pattern 2: ### contract-id —
        while IFS= read -r line; do
            local id=$(echo "$line" | grep -oE '^### [a-z-]+-[0-9]+' | sed 's/^### //')
            [[ -n "$id" ]] && file_contracts+=("$id")
        done < <(grep -E '^### [a-z-]+-[0-9]+ — ' "$file" 2>/dev/null)

        # Pattern 3: > contract-id (MUST
        while IFS= read -r line; do
            local id=$(echo "$line" | grep -oE '> [a-z-]+-[0-9]+' | sed 's/^> //')
            [[ -n "$id" ]] && file_contracts+=("$id")
        done < <(grep -E '> [a-z-]+-[0-9]+ \(MUST' "$file" 2>/dev/null)

        # Pattern 4: **contract-id**:
        while IFS= read -r line; do
            local id=$(echo "$line" | grep -oE '\*\*[a-z-]+-[0-9]+\*\*' | tr -d '*')
            [[ -n "$id" ]] && file_contracts+=("$id")
        done < <(grep -E '^\*\*[a-z-]+-[0-9]+\*\*:' "$file" 2>/dev/null)

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

    mapfile -t SPEC_FILES < <(get_spec_files "true")

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
        local contracts=$(grep -cE '(\[[a-z-]+-[0-9]+\]|^### [a-z]+(-[a-z]+)*-[0-9]+ |> [a-z-]+-[0-9]+ \(MUST|\*\*[a-z-]+-[0-9]+\*\*)' "$file" 2>/dev/null || true)
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

    local test_dir="$SCRIPT_DIR/../test/tests"

    if [[ ! -d "$test_dir" ]]; then
        print_error "Test directory not found: $test_dir"
        return 1
    fi

    # Extract contract IDs from test annotations
    # Pattern: /// Contract: [contract-id] or // Contract: [contract-id]
    local test_contracts=$(grep -rhoE '(///|//) Contract: \[[a-z-]+-[0-9]+\]' "$test_dir"/*.rs 2>/dev/null \
        | grep -oE '\[[a-z-]+-[0-9]+\]' | tr -d '[]' | sort -u)

    # Get all contracts from registry
    local all_contracts=$(grep -oE '`[a-z-]+-[0-9]+`' "$SCRIPT_DIR/CONTRACT_REGISTRY.md" 2>/dev/null \
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
    local spec_file=$(grep -l "\[$contract_id\]" "$SCRIPT_DIR"/*.md 2>/dev/null | grep -v REGISTRY | head -1)

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
    local output_file="${1:-$SCRIPT_DIR/TRACEABILITY.md}"

    print_header "Generating Traceability Matrix"

    local test_dir="$SCRIPT_DIR/../test/tests"

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

        # Get all contracts from registry
        local all_contracts=$(grep -oE '`[a-z-]+-[0-9]+`' "$SCRIPT_DIR/CONTRACT_REGISTRY.md" 2>/dev/null \
            | tr -d '`' | sort -u)

        echo "$all_contracts" | while read -r contract; do
            [[ -z "$contract" ]] && continue

            # Search for this contract in test files
            local test_match=$(grep -rln "Contract: \[$contract\]" "$test_dir"/*.rs 2>/dev/null | head -1)

            if [[ -n "$test_match" ]]; then
                local test_file=$(basename "$test_match")
                # Try to find the function name (line after Contract annotation)
                local fn_name=$(grep -A5 "Contract: \[$contract\]" "$test_match" 2>/dev/null \
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
        for test_file in "$test_dir"/*.rs; do
            [[ ! -f "$test_file" ]] && continue
            local basename_file=$(basename "$test_file")

            # Find all contract annotations in this file
            local contracts_in_file=$(grep -oE 'Contract: \[[a-z-]+-[0-9]+\]' "$test_file" 2>/dev/null \
                | grep -oE '\[[a-z-]+-[0-9]+\]' | tr -d '[]' | sort -u | tr '\n' ', ' | sed 's/, $//')

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
        local covered=$(grep -rhoE 'Contract: \[[a-z-]+-[0-9]+\]' "$test_dir"/*.rs 2>/dev/null \
            | grep -oE '\[[a-z-]+-[0-9]+\]' | tr -d '[]' | sort -u | grep -c '[a-z]' 2>/dev/null || echo 0)
        local pct=$((covered * 100 / total))

        echo "- **Total Contracts:** $total"
        echo "- **Contracts with Tests:** $covered"
        echo "- **Coverage:** $pct%"

    } > "$output_file"

    print_success "Generated: $output_file"
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
        *)
            print_error "Unknown command: $command"
            echo ""
            cmd_help
            exit 1
            ;;
    esac
}

main "$@"
