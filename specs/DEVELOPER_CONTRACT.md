# Developer Contract for Naia Specifications

This document outlines the usage of the Rust-based Specification Tooling.

## Canonical Tool
The single source of truth for all spec operations is the `spec_tool` binary (via the `naia-specs` crate).

## Routine Operations
Run via `cargo run -p naia-specs -- <command>`:

- **Check Coverage:** `cargo run -p naia-specs -- coverage`
- **Lint Specs:** `cargo run -p naia-specs -- lint`
- **Regenerate Files:** 
  - `registry` (CONTRACT_REGISTRY.md)
  - `traceability` (TRACEABILITY.md)
  - `bundle` (NAIA_SPECS.md)
- **Verify:** `cargo run -p naia-specs -- verify` (Full health check)

## Determinism & Goldens
The tests rely on **Golden Files** stored in `specs/tests/golden/`.
These files ensure that CLI output remains consistent byte-for-byte.

### Updating Goldens
If you modify the tool output (e.g., change formatting), valid tests will fail. 
To update the goldens:

1. **Verify your changes are correct.**
2. Run the command with `--deterministic` and overwrite the golden file.

**Example: Updating Registry Golden**
```bash
cargo run -p naia-specs -- registry specs/tests/golden/registry.md --deterministic
```

**Example: Updating Help Golden**
```bash
cargo run -p naia-specs -- help > specs/tests/golden/help.stdout
```
(Note: For stdout-only commands, redirect output. For commands taking an output arg, use that).

### Deterministic Mode
The `--deterministic` flag enforces:
- "1970-01-01 00:00 UTC" timestamps.
- Stable sorting order for file processing (though the tool does this by default now mostly).
