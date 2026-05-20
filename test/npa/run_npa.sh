#!/usr/bin/env bash
# Fast NPA adapter runner for naia BDD specs.
#
# Replaces `cargo run --manifest-path test/npa/Cargo.toml --` as the
# adapter_cmd in .tesaki/config.toml. On warm builds (nothing changed),
# `cargo build` completes in < 0.5s. Cold builds pay the full compile cost
# once; all subsequent invocations in the same session are fast.
#
# Usage (via .tesaki/config.toml):
#   adapter_cmd = "bash test/npa/run_npa.sh"
#
# To force a release build (faster binary, slower compile):
#   adapter_cmd = "bash test/npa/run_npa.sh --release"
set -euo pipefail

PROFILE="debug"
EXTRA_FLAGS=()
for arg in "$@"; do
    if [ "$arg" = "--release" ]; then
        PROFILE="release"
        EXTRA_FLAGS+=("--release")
    fi
done

# Build (fast no-op when binary is current, full compile when sources changed)
cargo build -p naia-npa "${EXTRA_FLAGS[@]}" --quiet

exec target/${PROFILE}/naia_npa "$@"
