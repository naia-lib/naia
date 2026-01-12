# Naia Development Plan

**Status:** Active - Phase B (Fix Implementation)
**Updated:** 2026-01-11
**Goal:** All spec contracts have compiling tests, then fix implementation

---

## Two-Phase Development Process

**Phase A: Complete Test Coverage (COMPLETE)**
- Every spec contract has a compiling E2E test
- Tests MUST compile with NO `todo!()` macros
- Tests are allowed to FAIL - that indicates implementation gaps
- Goal: 185/185 contracts covered, zero `todo!()` - ACHIEVED

**Phase B: Fix Implementation (CURRENT)**
- Run all tests, observe failures
- Systematically fix implementation
- Failing tests are the bug tracker

**Key insight:** A `todo!()` in a test is a **specification gap**, not an implementation bug. Write what you *expect* to happen, and let the test fail.

---

## Current State

| Metric | Value | Target |
|--------|-------|--------|
| Contracts with compiling tests | **185/185 (100%)** | 185/185 (100%) |
| Tests with `todo!()` | **0** | 0 |
| Phase A | **COMPLETE** | - |

---

## Immediate Next Actions (Phase B)

### Priority 1: Run All Tests and Collect Failures

```bash
cargo test --package naia-test 2>&1 | tee test_results.txt
```

### Priority 2: Known Implementation Gaps

Based on Phase A test failures, the following need implementation fixes:

1. **Bandwidth monitoring not enabled by default** - Tests `bandwidth_exposes_both_directions`, `metrics_apis_safe_after_construction`, `per_direction_metrics_consistency`, `throughput_must_be_non_negative` fail because bandwidth monitoring is disabled by default
   - Fix: Enable in `test_client_config()` or make it default-on

### Priority 3: For Each Failing Test

1. Understand what the test expects (read the spec)
2. Fix the implementation to match the spec
3. Verify test passes
4. Run 3x for flakiness
5. Ensure no regressions

---

## Phase A Progress

| Task | Status |
|------|--------|
| Annotate all existing tests | **DONE** |
| Gap analysis | **DONE** |
| Write tests for 176 contracts | **DONE** |
| Eliminate `todo!()` macros | **DONE** |
| Write tests for 9 observability contracts | **DONE** |
| **Phase A Complete** | **2026-01-11** |

---

## After Phase A: Phase B Planning

Once Phase A is complete, Phase B will:

1. Run all tests: `cargo test --package naia-test`
2. Collect list of failing tests
3. Prioritize by importance/risk
4. Systematically fix implementation
5. Each fix: run 3x for flakiness, check no regressions

**Do not start Phase B until Phase A is complete.**

---

## Session History

### 2026-01-12
- **Process change:** Adopted two-phase development (Phase A: tests, Phase B: impl)
- Updated CLAUDE.md, DEV_PROCESS.md, PLAN.md to reflect new process

### Previous Sessions
- Fixed entity-authority-11/12 implementation bug
- Fixed take_authority() sending wrong messages
- Added e2e_debug documentation
- Annotated 154 tests with contract IDs
- Generated GAP_ANALYSIS.md with 83 gaps identified

---

## Completion Checklist

```
[x] Annotate all existing tests with contract IDs
[x] Generate gap analysis
[x] Write tests for 176 contracts

--- PHASE A: Complete Test Coverage ---
[x] Eliminate ALL todo!() macros (write actual assertions)
[x] Write compiling tests for 9 observability contracts
[x] Verify: spec_tool.sh coverage shows 185/185
[x] Verify: grep -r "todo!" returns nothing
[x] Verify: cargo test --package naia-test --no-run succeeds

--- PHASE B: Fix Implementation (after Phase A) ---
[ ] Run all tests, collect failures
[ ] Prioritize fixes by importance
[ ] Fix implementation systematically
[ ] All tests pass
[ ] Run 3x for flakiness
```

---

## Test Files (1:1 Spec Mapping)

Test files now map directly to spec files for instant traceability:

| Spec File | Test File | Status |
|-----------|-----------|--------|
| `1_connection_lifecycle.md` | `01_connection_lifecycle.rs` | Mostly covered |
| `2_transport.md` | `02_transport.rs` | Fully covered |
| `3_messaging.md` | `03_messaging.rs` | Covered |
| `4_time_ticks_commands.md` | `04_time_ticks_commands.rs` | Covered |
| `5_observability_metrics.md` | `05_observability_metrics.rs` | 9 uncovered (needs harness) |
| `6_entity_scopes.md` | `06_entity_scopes.rs` | Covered |
| `7_entity_replication.md` | `07_entity_replication.rs` | Mostly covered |
| `8_entity_ownership.md` | `08_entity_ownership.rs` | (via publication tests) |
| `9_entity_publication.md` | `09_entity_publication.rs` | Covered |
| `10_entity_delegation.md` | `10_entity_delegation.rs` | Covered |
| `11_entity_authority.md` | `11_entity_authority.rs` | Covered |
| `12_server_events_api.md` | `12_server_events_api.rs` | Mostly covered |
| `13_client_events_api.md` | `13_client_events_api.rs` | Mostly covered |
| `14_world_integration.md` | `14_world_integration.rs` | Covered |

**To find tests for a contract:** Open the matching numbered test file

---

## Resolved Issues

### entity-authority-11/12: Out-of-scope Authority Cleanup ✓ FIXED 2026-01-12

**Spec Text:**
> [entity-authority-11]: If a client becomes out-of-scope for delegated entity E, their authority status MUST be cleared
> [entity-authority-12]: If the authority-holding client loses scope for E, the server MUST release/reset authority and other clients transition to Available

**Fix Applied:**
1. `shared/src/world/remote/remote_world_manager.rs:146` - Changed `.unwrap()` to graceful error handling
2. `shared/src/world/remote/remote_world_manager.rs:168,181` - Same pattern for related functions
3. `server/src/world/server_auth_handler.rs` - Added `user_is_authority_holder()` helper
4. `server/src/server/world_server.rs:1295-1313` - Added automatic authority release when holder loses scope

**Test:** `out_of_scope_ends_authority_for_that_client` passes 3x

---

## Key Files Reference

| File | Purpose |
|------|---------|
| `CLAUDE.md` | Session instructions, quick reference |
| `PLAN.md` | This file - current state, next actions |
| `DEV_PROCESS.md` | Full SDD methodology |
| `specs/generated/CONTRACT_REGISTRY.md` | All 185 contract IDs |
| `specs/generated/TRACEABILITY.md` | Contract↔test mapping |
| `specs/generated/GAP_ANALYSIS.md` | Prioritized uncovered contracts |

---

## Quality Rules for Tests

**Phase A requirements (test coverage):**
1. **Must compile** - No `todo!()` macros, actual assertions
2. **Contract annotations required** - Every test needs `/// Contract: [id]`
3. **Minimal assertions** - Test exactly what the contract specifies
4. **Failures are OK** - Test documents implementation gap

**Phase B requirements (implementation fixes):**
1. **No timing hacks** - Use `expect()` polling, not `sleep()`
2. **Reordering-tolerant** - Tests must pass regardless of async message order
3. **Run 3x** - Verify no flakiness before marking complete
4. **No regressions** - Full test suite must still pass

---

## Session Workflow

```bash
# Start of session
./specs/spec_tool.sh coverage          # Check contracts with tests (target: 185/185)
grep -r "todo!" test/tests/*.rs        # Find incomplete tests (target: 0)

# Phase A work: Write compiling tests
# For entity-authority contracts → open 11_entity_authority.rs
cargo test --package naia-test --test 11_entity_authority --no-run  # Must compile
cargo test --package naia-test --test 11_entity_authority           # May fail - OK!

# Phase B work (ONLY after Phase A complete): Fix implementation
cargo test --package naia-test         # See all failures
# Fix implementation, then verify:
cargo test --package naia-test <test_name>  # 3x for flakiness

# Debugging failing tests (enable detailed tracing)
cargo test --package naia-test --features e2e_debug <test_name> -- --nocapture

# End of session
./specs/spec_tool.sh coverage          # Verify annotation coverage
grep -r "todo!" test/tests/*.rs        # Verify no incomplete tests
```

### e2e_debug Feature

When debugging test failures, enable `e2e_debug` for detailed network event tracing:

```bash
cargo test --package naia-test --features e2e_debug <test_name> -- --nocapture
```

This outputs `[SERVER_SEND]` and `[CLIENT_RECV]` events showing entity IDs, authority states, and code locations. Useful for understanding message flow and state transitions.
