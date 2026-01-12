# Naia Development Plan

**Status:** Active - Phase B (Fix Implementation)
**Updated:** 2026-01-11
**Goal:** Get all E2E tests passing

---

## Two-Phase Development Process

**Phase A: Complete Test Coverage (COMPLETE ✓)**
- Every spec contract has a compiling E2E test
- Tests MUST compile with NO `todo!()` macros
- Tests are allowed to FAIL - that indicates implementation gaps
- Goal: 236/236 contracts covered, zero `todo!()`
- **STATUS: ACHIEVED** ✅

**Phase B: Fix Implementation (CURRENT)**
- Run all tests, observe failures
- Systematically fix implementation and test structure
- Failing tests are the bug tracker
- **Current: 158/200 tests passing (79%)**

**Key insight:** A `todo!()` in a test is a **specification gap**, not an implementation bug. Write what you *expect* to happen, and let the test fail.

---

## Current State (Phase B)

| Metric | Value | Target |
|--------|-------|--------|
| Tests passing | **195/215 (91%)** | 215/215 (100%) |
| Tests failing | **20** | 0 |
| Critical bugs fixed | **5** (overflow, bandwidth, replication, 2x framework violations) | - |
| Phase A | **COMPLETE ✓** | - |
| Phase B | **IN PROGRESS** | Complete |

### Test Results by File

| Test File | Status | Notes |
|-----------|--------|-------|
| 00_common | ✅ 17/17 | All passing |
| 01_connection_lifecycle | ✅ 17/21 | 4 ignored (need harness features) |
| 02_transport | ✅ 14/14 | All passing |
| 03_messaging | ⚠️ 28/29 | 1 failure (disconnect_cancels_pending_requests) |
| 04_time_ticks_commands | ✅ 20/20 | All passing |
| 05_observability_metrics | ✅ 12/12 | All passing |
| 06_entity_scopes | ⚠️ 10/14 | 4 timeout failures |
| 07_entity_replication | ✅ 16/16 | All passing |
| 08_entity_ownership | ⚠️ 13/14 | 1 delegation bug (enabling_delegation_transfers_ownership_to_server) |
| 09_entity_publication | ✅ 11/11 | All passing |
| 10_entity_delegation | ⚠️ 8/17 | 9 failures (delegation state machine) |
| 11_entity_authority | ⚠️ 11/16 | 5 failures (authority state machine) |
| 12_server_events_api | ✅ 8/8 | All passing |
| 13_client_events_api | ✅ 6/6 | All passing |
| 14_world_integration | ✅ 4/4 | All passing |

---

## Phase B: Implementation Fixes

### Completed Fixes (2026-01-12)

1. **Arithmetic overflow in base_time_manager.rs:128**
   - Issue: `round_trip_time_millis - server_process_time_millis` could underflow
   - Fix: Used `saturating_sub()` for safe edge case handling
   - Impact: Fixed 12+ messaging/observability test failures

2. **Bandwidth monitoring panic in io.rs:169**
   - Issue: Tests called `outgoing_bandwidth()` without enabling monitoring
   - Fix: Enabled `bandwidth_measure_duration` in `test_client_config()`
   - Impact: Fixed all 12 observability tests
   - **Learning:** Don't return default values - implement the feature properly

3. **Client.rs:660 replication config panic**
   - Issue: Test called `configure_replication(Private)` when already Private by default
   - Fix: Removed redundant configuration call
   - Impact: Unblocked entity_ownership tests

4. **Test framework violations in 04_time_ticks_commands**
   - Issue: Sequential `mutate()` calls without `expect()` between them
   - Fix: Merged read+write operations into single `mutate()` block
   - Impact: Fixed all 4 command tests (20/20 passing)
   - **Learning:** Reading state is NOT mutation - combine with actual mutation

5. **Test framework violations in 08_entity_ownership (13 tests)** 🎉
   - Issue: Sequential `expect()` → `expect()` calls violating mutate/expect alternation
   - Fix: Merged sequential expect() blocks into single expect() with staged conditions
   - Impact: **Fixed 13/14 entity_ownership tests** (1 remaining has delegation bug)
   - **Learning:** Wait for replication THEN verify state in single expect() block
   - **Progress:** +13 tests passing, revealed 1 implementation bug

6. **Delegation ownership restriction in world_server.rs**
   - Issue: Server panicking when enabling delegation on client-owned entities
   - Spec: [entity-ownership-11] allows server to enable delegation, transferring ownership
   - Fix: Removed panics at lines 838 and 862 (spec vs implementation mismatch)
   - Impact: Unblocked delegation transition path, revealed deeper message handling bug
   - **Status:** Test now fails on message type handling, needs further investigation

### Current Failures (20 tests)

**Category 1: Timeout Failures (5 tests - NEXT PRIORITY)** ⚠️
- Files: `03_messaging` (1), `06_entity_scopes` (4)
- Problem: `expect()` times out after 100 ticks
- Likely cause: Missing implementation or incorrect assertions
- Priority: HIGH (reduced from 8 to 5 tests)
- **Progress:** Fixed 3 messaging timeouts via structure fixes

**Category 2: Delegation/Authority Logic (14 tests)** ⚠️
- Files: `08_entity_ownership` (1), `10_entity_delegation` (9), `11_entity_authority` (5)
- Problem: Complex state machine bugs, message handling
- Examples:
  - `enabling_delegation_transfers_ownership_to_server`: EnableDelegation message not handled for client-owned entities
  - 9 delegation tests: State transitions not implemented
  - 5 authority tests: Authority grant/revoke logic bugs
- Priority: MEDIUM-HIGH
- **Progress:** Reduced from 15 to 14 tests, identified root causes

**Category 3: Ignored Tests (4 tests)**
- File: `01_connection_lifecycle`
- Reason: Require harness features not yet implemented
- Priority: LOW (defer until core tests pass)

---

## Immediate Next Actions (Phase B)

### Priority 1: Investigate Timeout Failures (5 tests) ✅ NEXT

**Files affected:**
- `03_messaging.rs` (1 failure: disconnect_cancels_pending_requests)
- `06_entity_scopes.rs` (4 failures)

**Debugging approach:**
1. Run failing test with `--features e2e_debug -- --nocapture`
2. Check if `expect()` condition is ever becoming true
3. Verify implementation matches spec requirements
4. Check for missing replication triggers or state updates

**Common causes:**
- Implementation missing entirely
- Incorrect assertion (checking wrong thing)
- Missing scope/publication setup

**Expected outcome:** Reduce from 5 to 0 timeout failures

### Priority 2: Fix Delegation/Authority Logic (14 tests)

**Files:** `10_entity_delegation.rs` (10), `11_entity_authority.rs` (5)

These are likely real implementation bugs in the delegation/authority state machines. Defer until test structure issues are fixed.

---

## Session Workflow for Phase B

```bash
# Start of session - pick one approach:
./specs/spec_tool.sh verify                              # Full health check (5-10 min)
./specs/spec_tool.sh verify --contract <id>              # Fast: target specific contract
cargo test --package naia-test 2>&1 | grep FAILED       # Quick: see only failures

# Fix approach
1. Identify failure type (panic location, timeout, assertion)
2. For panics at scenario.rs:155/213 → test structure issue
3. For timeouts → implementation gap or wrong assertion
4. For assertion failures → logic bug

# Verify fix (fast iteration)
./specs/spec_tool.sh verify --contract <id>              # Fast: targeted verification
cargo test --package naia-test --test <file>             # Alternative: single file
./specs/spec_tool.sh verify                              # Full: check no regressions

# Update docs
# Update PLAN.md current state numbers
# Note any learnings in CLAUDE.md or DEV_PROCESS.md
```

---

## Key Learnings (Phase B)

### 1. Framework Violations Are Design Enforcement
The harness panics on `mutate()` → `mutate()` by design. This prevents bad test patterns. **Never work around it** - fix the test structure.

### 2. Implement Features, Don't Fake Them
When tests need bandwidth monitoring, **enable it in config**, don't return 0.0 as a default. Quality engineering means implementing the feature properly.

### 3. Reading State Is Not Mutation
Getting a tick, querying a value, checking status - these aren't mutations. Combine them with actual state changes in a single `mutate()` block.

### 4. Test Structure Issues vs Implementation Bugs
Many "failures" are test structure problems, not implementation bugs. Fix test structure first before assuming implementation is broken.

---

## Phase B Completion Checklist

```
--- PHASE A: Complete Test Coverage ---
[x] Annotate all existing tests with contract IDs
[x] Generate gap analysis
[x] Write tests for original 185 contracts
[x] Eliminate ALL todo!() macros
[x] Write tests for 51 new spec contracts (236/236)
[x] Verify: spec_tool.sh coverage shows 236/236
[x] Verify: grep -r "todo!" returns nothing
[x] Verify: cargo test --package naia-test --no-run succeeds
[x] **PHASE A COMPLETE** ✅

--- PHASE B: Fix Implementation (CURRENT) ---
[x] Run all tests, catalog failures (42 failures identified)
[x] Fix arithmetic overflow in base_time_manager.rs
[x] Fix bandwidth monitoring (implement properly in config)
[x] Fix test framework violations in 04_time_ticks_commands
[x] Fix client.rs:660 replication panic
[x] Fix 13 test structure issues in 08_entity_ownership ✅ **MAJOR WIN**
[x] Reduce test failures from 42 to 20 (+37 tests passing!)
[x] Improve coverage from 79% to 91%
[ ] Investigate 5 timeout failures (03_messaging: 1, 06_entity_scopes: 4)
[ ] Fix 14 delegation/authority logic bugs
[ ] All tests pass (target: 215/215, currently 195/215)
[ ] Run 3x for flakiness
[ ] **PHASE B COMPLETE**
```

---

## Session Workflow

```bash
# Start of session
./specs/spec_tool.sh coverage          # Check contracts with tests (target: 236/236)
grep -r "todo!" test/tests/*.rs        # Find incomplete tests (target: 0)

# Phase A work: Write compiling tests for uncovered contracts
# Example: writing tests for commands-03a through commands-03d
vim test/tests/04_time_ticks_commands.rs
cargo test --package naia-test --test 04_time_ticks_commands --no-run  # Must compile

# Verify coverage improved
./specs/spec_tool.sh coverage

# Debugging (if needed)
cargo test --package naia-test --features e2e_debug <test_name> -- --nocapture

# End of session
./specs/spec_tool.sh coverage          # Verify annotation coverage
grep -r "todo!" test/tests/*.rs        # Verify no incomplete tests
```

---

## Key Files Reference

| File | Purpose |
|------|---------|
| `CLAUDE.md` | Session instructions, quick reference |
| `PLAN.md` | This file - current state, next actions |
| `DEV_PROCESS.md` | Full SDD methodology |
| `specs/generated/CONTRACT_REGISTRY.md` | All 236 contract IDs |
| `specs/generated/TRACEABILITY.md` | Contract↔test mapping |

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

## Session History

### 2026-01-11 (Current)
- **Spec hardening:** Added 51 new contracts
  - Protocol identity (`protocol_id`) with u128 little-endian encoding
  - Command sequence with `MAX_COMMANDS_PER_TICK_PER_CONNECTION = 64`
  - Error taxonomy with `ProtocolMismatch` error type
  - Metrics testability with inequality-style assertions
- **Tool fix:** Updated `spec_tool.sh` to support alphanumeric contract suffixes
- **Status:** Phase A incomplete (185/236 contracts covered)

### Previous Sessions
- Completed tests for original 185 contracts
- Fixed entity-authority-11/12 implementation bug
- Added e2e_debug documentation
- Adopted two-phase development process
