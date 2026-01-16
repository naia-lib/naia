# SUNSET.md — Legacy Spec Tool Deprecation Notice

## Status: FROZEN

This crate is **frozen** — no new feature work should be done here.

---

## What This Was

`naia_spec_tool` was the pre-Namako specification management system for Naia. It operated directly on Markdown contract files (`.spec.md`) and provided:

- **Validation:** Lint, orphan checks, cross-reference verification
- **Coverage analysis:** Contract-to-test traceability matrices
- **Documentation:** Bundle generation, packet generation for reviews
- **Utilities:** Registry extraction, obligation fixing, statistics

The contracts live in `naia/specs/contracts/` (15 files, 00–14).

---

## Why It's Frozen

The Namako v1 pipeline replaces this approach:

| Aspect | Legacy (spec_tool) | Namako v1 |
|--------|-------------------|-----------|
| Spec format | Markdown with embedded annotations | Executable Gherkin `.feature` files |
| Verification | Static text analysis | Runtime execution + hash-based identity |
| Baseline | None (manual review) | `certification.json` with cryptographic hashes |
| Binding | Implicit (comments reference code) | Explicit step bindings with `#[given]`/`#[when]`/`#[then]` |

Maintaining two systems creates confusion and maintenance burden. Namako provides stronger guarantees.

---

## Delete Criteria

This crate may be deleted when:

1. **All 15 contracts converted:** Each `.spec.md` has a corresponding `.feature` file with passing scenarios
2. **Utilities extracted:** Any reusable code (e.g., Markdown parsing, traceability logic) has been extracted to appropriate locations
3. **No references remain:** No CI scripts, documentation, or workflows depend on this tool
4. **Team sign-off:** Explicit decision that the migration is complete

---

## Current Dependencies

- The legacy tool references `naia/specs/contracts/` for Markdown spec files
- It may have been used by older CI scripts (check and remove)
- Some extraction utilities (registry, traceability) may have value for migration

---

## Timeline

- **Frozen:** January 2026
- **Target deletion:** After contract conversion complete (tracked in `naia/specs/CONVERSION_PLAN.md`)

---

## Contact

If you need to understand this tool for migration purposes, review the source in `src/` and tests in `tests/`. Do not extend functionality — invest in the Namako pipeline instead.
