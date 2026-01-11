#!/bin/bash
# build_spec_bundle.sh
# Concatenates all Naia specs into a single well-formatted Markdown file with TOC.
#
# Usage: ./build_spec_bundle.sh [output_file]
# Default output: NAIA_SPECS.md

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUTPUT_FILE="${1:-$SCRIPT_DIR/NAIA_SPECS.md}"

# Collect all numbered spec files (excluding template) in numeric order
mapfile -t SPEC_FILES < <(
    find "$SCRIPT_DIR" -maxdepth 1 -name '*.md' -type f \
        | grep -E '/[0-9]+_' \
        | grep -v '1_template\.md' \
        | sort -t'/' -k2 -V
)

# Function to extract title from a spec file (first # heading)
get_title() {
    local file="$1"
    grep -m1 '^# ' "$file" | sed 's/^# //'
}

# Function to generate anchor from title (GitHub-style)
make_anchor() {
    echo "$1" | tr '[:upper:]' '[:lower:]' | sed 's/[^a-z0-9 -]//g' | sed 's/ /-/g'
}

# Start building output
{
    # Header
    cat <<'EOF'
# Naia Specifications Bundle

This document contains all normative specifications for the Naia networking engine, concatenated into a single reference.

**Generated:** $(date -u +"%Y-%m-%d %H:%M UTC")

---

## Table of Contents

EOF

    # Generate TOC
    for file in "${SPEC_FILES[@]}"; do
        title=$(get_title "$file")
        anchor=$(make_anchor "$title")
        basename=$(basename "$file")
        # Extract spec number from filename
        spec_num=$(echo "$basename" | grep -oE '^[0-9]+')
        echo "- [$spec_num. $title](#$anchor)"
    done

    echo ""
    echo "---"
    echo ""

    # Concatenate each spec with separators
    for file in "${SPEC_FILES[@]}"; do
        title=$(get_title "$file")
        basename=$(basename "$file")

        echo "<!-- ======================================================================== -->"
        echo "<!-- Source: $basename -->"
        echo "<!-- ======================================================================== -->"
        echo ""

        # Output the file content
        cat "$file"

        echo ""
        echo ""
        echo "---"
        echo ""
    done

} > "$OUTPUT_FILE"

# Fix the date substitution (it was literal in heredoc)
sed -i "s/\$(date -u +\"%Y-%m-%d %H:%M UTC\")/$(date -u +"%Y-%m-%d %H:%M UTC")/" "$OUTPUT_FILE"

echo "Generated: $OUTPUT_FILE"
echo "Included ${#SPEC_FILES[@]} specifications:"
for file in "${SPEC_FILES[@]}"; do
    echo "  - $(basename "$file")"
done
