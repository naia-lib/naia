#!/bin/bash
set -e

# This script runs spec_tool.sh commands and saves output to 'baseline' dir

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SPECS_DIR="$SCRIPT_DIR/../../"
TOOL="$SPECS_DIR/spec_tool.sh"
BASELINE_DIR="$SCRIPT_DIR/output"

# Ensure tool is executable
chmod +x "$TOOL"

run_cmd() {
    local cmd_name="$1"
    shift
    local cmd_args=("$@")
    
    # Create directory for command
    mkdir -p "$BASELINE_DIR/$cmd_name"
    
    # Construct a safe filename from args
    local case_name="default"
    if [ ${#cmd_args[@]} -gt 0 ]; then
        case_name=$(echo "${cmd_args[@]}" | tr ' ' '_')
    fi
    
    echo "Running: $cmd_name ${cmd_args[*]}"
    
    # Run command, capture stdout, stderr, and exit code
    set +e
    "$TOOL" "$cmd_name" "${cmd_args[@]}" > "$BASELINE_DIR/$cmd_name/$case_name.stdout" 2> "$BASELINE_DIR/$cmd_name/$case_name.stderr"
    echo $? > "$BASELINE_DIR/$cmd_name/$case_name.exit"
    set -e
}

# Run cases
run_cmd "help"
run_cmd "stats"
run_cmd "coverage"
run_cmd "adequacy"
run_cmd "registry"
run_cmd "lint"
run_cmd "check-orphans"
run_cmd "check-refs"
run_cmd "packet" "connection-01"
# run_cmd "packet" "connection-01" "--full-tests" # Careful, this might produce a lot of output
run_cmd "gen-test" "connection-01"
run_cmd "traceability"

echo "Baseline generation complete."
