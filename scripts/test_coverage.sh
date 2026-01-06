#!/bin/bash
# Test Coverage Script for Naia
# Generates HTML coverage report using cargo-tarpaulin

set -e

echo "🔍 Naia Test Coverage Report Generator"
echo "======================================"
echo ""

# Check if tarpaulin is installed
if ! command -v cargo-tarpaulin &> /dev/null; then
    echo "📦 Installing cargo-tarpaulin..."
    cargo install cargo-tarpaulin
fi

echo "🧪 Running tests with coverage tracking..."
echo ""

# Run tarpaulin with appropriate exclusions
cargo tarpaulin \
    --workspace \
    --exclude-files "demos/*" \
    --exclude-files "adapters/*" \
    --exclude-files "*/tests/*" \
    --out Html \
    --output-dir coverage \
    --target-dir target/coverage \
    --timeout 300

echo ""
echo "✅ Coverage report generated!"
echo "📊 Open coverage/index.html to view results"
echo ""
echo "Key files to check:"
echo "  - shared/src/world/local/local_world_manager.rs"
echo "  - shared/src/world/component/entity_property.rs"
echo "  - shared/src/world/sync/auth_channel.rs"
echo "  - client/src/client.rs"
echo ""

