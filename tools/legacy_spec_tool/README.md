# naia_spec_tool

> ⚠️ **LEGACY / FROZEN** — Pre-Namako Markdown spec workflow. Do not extend.
> See [SUNSET.md](SUNSET.md) for deprecation details.

## Overview

This tool was the original specification management system for Naia, operating on Markdown contract files. It has been superseded by the Namako v1 BDD pipeline.

## Commands (for reference)

| Command | Purpose |
|---------|---------|
| `bundle` | Generate combined NAIA_SPECS.md |
| `lint` | Check specs for consistency issues |
| `validate` | Run all validation checks |
| `check-orphans` | Find MUST statements without IDs |
| `check-refs` | Verify cross-reference links |
| `coverage` | Analyze contract test coverage |
| `adequacy` | Check obligation-to-label mapping |
| `stats` | Show spec statistics |
| `registry` | Extract contract IDs |
| `gen-test` | Generate test skeleton |
| `traceability` | Generate contract-to-test matrix |
| `fix-obligations` | Add missing Obligations sections |
| `packet` | Generate contract review packet |

## Do Not Use For New Work

For specification work, use the Namako pipeline:
- Features: `naia/specs/features/`
- Baseline: `naia/specs/certification.json`
- Commands: `namako lint`, `namako run`, `namako verify`
