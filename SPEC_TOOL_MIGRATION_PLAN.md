# To: Claude — Plan: Migrate spec_tool.sh + spec_index.py → Rust crate (NO bash wrapper), zero regressions

## Non-negotiables
1. **Zero behavior regressions**: same commands, flags, exit codes, default paths, and **byte-for-byte output** (unless we explicitly decide otherwise).
2. **No wrapper bash script**: final user entrypoint is a Rust binary (e.g. `cargo run -p specs -- <cmd>` and/or `./target/.../spec_tool <cmd>`).
3. Keep dirs as-is: `specs/contracts/**`, `specs/generated/**` remain unchanged.
4. Deterministic output across machines: sorting, stable formatting, no timestamps unless already present (and if present, keep same behavior).
5. Tooling stays “fast” (target <= current ~250ms for adequacy on full tree).

---

## Phase 0 — Inventory + Golden Baseline (DO THIS FIRST)
**Goal:** Capture the current tool as the source-of-truth so we can prove “no regression”.

1. **Enumerate CLI surface area** from `spec_tool.sh`:
    - Commands + options + defaults:
        - `help`, `packet`, `verify`, `coverage`, `adequacy`, `gen-test`, `traceability`, `stats`, `check-refs` (and anything else present).
    - Inputs/outputs:
        - default dirs (contracts/tests/generated)
        - file writes vs stdout (e.g. `--out`, `generated/packets/...`)
    - Exit codes (success vs failure cases; strict vs non-strict).
2. **Create a baseline runner script (TEMP, for migration only)**:
    - Runs every command on a representative set and saves outputs + exit codes.
    - Store under `specs/tests/baseline/`:
        - `baseline/<command>/<case>.stdout`
        - `baseline/<command>/<case>.stderr`
        - `baseline/<command>/<case>.exit`
    - Include cases:
        - Contract with **no obligations** + no labels
        - Contract with **Obligations** t1/t2 + partial coverage (missing)
        - Contract with full coverage
        - Missing test annotation
        - Multi-contract annotation in one test
        - `--full-tests` packet mode
        - `adequacy` + `--strict`
3. **Port the existing `spec_tool_test.sh` suite into baseline artifacts too**:
    - Either run that script and capture its assertions, or replicate its fixtures under Rust tests (preferred).
4. Freeze: baseline artifacts so we can diff.

> NOTE: This “baseline runner” is allowed as an internal migration aid, but it must not be the final interface.

---

## Phase 1 — Create `specs/` as a Rust crate + binary
**Goal:** Establish the new tool skeleton without touching behavior.

1. Create `specs/Cargo.toml`:
    - crate name suggestion: `naia-specs` (library) + binary `spec_tool`
    - edition 2021
2. Add workspace membership if repo is a cargo workspace; otherwise add a local workspace entry (keep minimal).
3. Dependencies (keep tight, but pragmatic):
    - CLI: `clap` (derive)
    - Errors: `anyhow` (or `thiserror` + `anyhow`)
    - Parsing: `regex`
    - FS traversal: `walkdir`
    - JSON (for internal index + debug): `serde`, `serde_json`
    - Testing CLI: `assert_cmd`, `predicates`
    - Golden snapshots (optional but recommended for zero-regression): `insta`
    - Temp dirs for fixtures: `tempfile`
4. Binary name: `spec_tool` (match old mental model).

---

## Phase 2 — Implement Core Indexer as a Rust library (replaces spec_index.py)
**Goal:** One canonical in-memory index powering ALL commands.

1. Implement `Index` (single pass over tree; no subprocesses):
    - From `specs/contracts/*.md`:
        - contract ids
        - spec file mapping
        - spec excerpt extraction (for packet)
        - obligations extraction ONLY when `**Obligations:**` section exists
    - From `test/tests/*.rs`:
        - `/// Contract: [...]` annotations mapping contract → test fn(s) → file
        - label extraction: `spec_expect("...")` and legacy `expect_msg("...")`
2. Provide stable outputs:
    - sort contract ids lexicographically
    - stable ordering of files and labels
3. Implement the “obligation coverage” logic (literal matching):
    - obligation `tN` covered iff label contains prefix `"{contract_id}.tN:"`
    - if no obligations defined:
        - adequacy requires at least one label with prefix `"{contract_id}:"` OR `"{contract_id}."` (match existing policy)
4. Expose a library API:
    - `Index::build(repo_root: Path) -> Index`
    - query helpers:
        - `contract_packet(contract_id) -> PacketData`
        - `adequacy_report() -> AdequacyData`
        - `coverage_report() -> CoverageData`
        - `gen_test_skeleton(contract_id) -> String`
        - etc.

---

## Phase 3 — Port Commands 1:1 (with Golden Tests after each command)
**Rule:** Implement one command, then lock it with golden tests before moving on.

### Command order (recommended)
1. `help` (easy; locks CLI contract)
2. `packet` (most used; drives workflow)
3. `coverage`
4. `adequacy` (+ `--strict`)
5. `verify`
6. `gen-test`
7. `traceability`
8. `stats`
9. `check-refs`

### `packet` command parity checklist
- Same headings and layout
- Same spec excerpt extraction rules
- Same “Tests” section format (concise vs `--full-tests`)
- Same Assertion Index behavior:
    - list labels (prefer `spec_expect` but include legacy)
    - if none, show NOTE + counts of `scenario.expect()` + `scenario.until()` (replicate current output)
- Same default write location(s) under `specs/generated/packets/`
- Same `--out` semantics (if supported today)

### `adequacy` command parity checklist
- Same category buckets + ranking (Priority 1/2/3/OK)
- Same exit behavior for `--strict`
- Deterministic ordering
- Performance: avoid O(N * grep) style loops; use the Index.

---

## Phase 4 — Replace `spec_tool_test.sh` with Rust tests in `specs/tests/`
**Goal:** The Rust crate is self-testing; no shell test runner.

1. Create `specs/tests/cli_parity.rs` using `assert_cmd`:
    - For each baseline case:
        - run old tool (TEMP during migration) and new binary and compare outputs
        - OR compare new binary output to baseline files (preferred, once baseline is captured)
2. Create `specs/tests/fixtures/`:
    - Minimal synthetic repos for targeted test cases (fast and deterministic)
    - Include the “Test Case J” coverage for obligation mapping regression
3. Use snapshot testing for large outputs:
    - `insta` snapshots for `packet` output for representative contracts
4. Validate exit codes exactly.

---

## Phase 5 — Migration Cutover + Deletion / Archival
**Goal:** Remove bash/python without breaking workflows.

1. Update docs (DEV_PROCESS.md / CLAUDE.md / PLAN.md) to call:
    - `cargo run -p specs -- packet <id>`
    - or `cargo run --manifest-path specs/Cargo.toml -- packet <id>`
    - optionally add `cargo install --path specs` as recommended local install
2. Remove or archive:
    - `spec_tool.sh`
    - `spec_index.py`
    - `spec_tool_test.sh`
    - If you want history, move to `specs/legacy/` but ensure nothing references them.
3. Ensure CI runs:
    - `cargo test -p specs`
    - plus whatever existing repo tests

---

## Hard Quality Gates (must pass before declaring migration “done”)
1. **Golden parity**: new tool output matches baseline for all captured cases.
2. **Full suite sanity**: `packet`, `coverage`, `adequacy`, `verify` run on the real repo successfully.
3. **Speed**: adequacy on full repo stays within target.
4. **Determinism**: outputs stable across repeated runs.

---

## Deliverables (what I expect you to produce)
1. `specs/` Rust crate with `src/lib.rs` + `src/main.rs`
2. Modular command impls under `specs/src/commands/*`
3. `specs/tests/*` parity + fixture tests
4. Baseline artifacts (or snapshots) proving “no regression”
5. Old scripts removed/archived + docs updated

---

## Execution Discipline
- Never “improve formatting” during parity phase. If you want improvements, do a post-migration v2 with explicit, reviewed diffs.
- If you discover ambiguous legacy behavior, preserve it first; log it as a follow-up.

GO.
