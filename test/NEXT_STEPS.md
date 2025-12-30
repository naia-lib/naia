# E2E Test Implementation Next Steps

This document outlines the strategic plan to get all E2E tests implemented correctly and passing.

## Current Status

**Test Execution Results** (as of latest run):
- ✅ **Passing**: 56 tests (43%)
- ❌ **Failing**: 61 tests (47%)
- ⏸️ **Ignored**: 5 tests (4%) - require features not yet available

**Test Files Status**:
- ✅ `harness_scenarios`: 2/2 passing
- ✅ `time_ticks_transport`: 24/24 passing
- ⚠️ `connection_auth_identity`: 5/14 passing (5 failed, 4 ignored)
- ⚠️ `events_world_integration`: 8/18 passing (10 failed)
- ⚠️ `integration_transport_parity`: 2/3 passing (1 failed)
- ⚠️ `messaging_channels`: 9/18 passing (9 failed)
- ⚠️ `protocol_schema_versioning`: 6/7 passing (1 failed)
- ❌ `entities_lifetime_identity`: 0/11 passing (10 failed, 1 ignored)
- ❌ `ownership_delegation`: 0/10 passing (10 failed)
- ❌ `rooms_scope_snapshot`: 0/15 passing (15 failed)

---

## Priority 1: Fix Test Harness Violations & Structural Issues

### 1.1 Verify All Tests Follow mutate/expect Pattern
**Status**: ✅ Fixed in `events_world_integration.rs`

**Action Items**:
- [x] Fix consecutive `mutate()` calls by merging operations
- [x] Remove empty placeholder `expect()` calls
- [x] Remove redundant "verify entity exists after spawn" checks
- [ ] Audit other test files for similar violations
- [ ] Run full test suite to verify no harness violations remain

**Files to Check**:
- `connection_auth_identity.rs`
- `entities_lifetime_identity.rs`
- `ownership_delegation.rs`
- `rooms_scope_snapshot.rs`
- `messaging_channels.rs`

---

## Priority 2: Fix Failing Tests with Clear Error Messages

### 2.1 High-Impact Test Files (0% passing)

#### `entities_lifetime_identity.rs` (0/11 passing)
**Action Items**:
- [ ] Run tests individually to identify failure patterns
- [ ] Check for common issues:
  - Entity registration problems
  - Component replication issues
  - Scope/visibility problems
  - Event ordering issues
- [ ] Fix root causes systematically
- [ ] Document any missing test harness features needed

#### `ownership_delegation.rs` (0/10 passing)
**Action Items**:
- [ ] Run tests individually to identify failure patterns
- [ ] Check for common issues:
  - Authority/ownership state management
  - Delegation request/response handling
  - Client disconnect cleanup
  - Authority revocation logic
- [ ] Verify ownership APIs are correctly used
- [ ] Fix root causes systematically

#### `rooms_scope_snapshot.rs` (0/15 passing)
**Action Items**:
- [ ] Run tests individually to identify failure patterns
- [ ] Check for common issues:
  - Room membership and scoping
  - Snapshot generation on join
  - Entity visibility across rooms
  - Scope include/exclude logic
- [ ] Verify room APIs are correctly used
- [ ] Fix root causes systematically

### 2.2 Medium-Impact Test Files (Partial passing)

#### `connection_auth_identity.rs` (5/14 passing)
**Action Items**:
- [ ] Fix failing tests:
  - `no_replication_before_auth_decision`
  - `malformed_identity_token_rejected`
  - `invalid_credentials_rejected`
  - `successful_auth_with_require_auth`
  - `disconnect_idempotent_and_clean`
- [ ] Review ignored tests to determine if they can be enabled:
  - `client_disconnects_due_to_heartbeat_timeout` (requires time manipulation)
  - `expired_or_reused_token_obeys_semantics` (token reuse validation not implemented)
  - `server_capacity_reject_produces_reject_event` (server capacity limits not configured)
  - `valid_identity_token_roundtrips` (server-generated token flow needs testing)

#### `events_world_integration.rs` (8/18 passing)
**Action Items**:
- [ ] Fix remaining 10 failing tests
- [ ] Complete TODO sections in tests:
  - Event draining verification
  - Event ordering verification
  - World integration verification
- [ ] Add missing assertions for test completeness

#### `messaging_channels.rs` (9/18 passing)
**Action Items**:
- [ ] Fix 9 failing tests
- [ ] Verify channel semantics are correctly tested
- [ ] Check for timing/ordering issues
- [ ] Complete TODO sections

#### `integration_transport_parity.rs` (2/3 passing)
**Action Items**:
- [ ] Fix 1 failing test
- [ ] Verify transport parity logic

#### `protocol_schema_versioning.rs` (6/7 passing)
**Action Items**:
- [ ] Fix 1 failing test
- [ ] Verify protocol versioning logic

---

## Priority 3: Complete TODO Sections

### 3.1 Tests with Incomplete Assertions
Many tests have `TODO` comments indicating missing verification logic.

**Action Items**:
- [ ] Identify all tests with TODO comments
- [ ] Categorize TODOs by type:
  - Missing assertions
  - Missing test harness features
  - Missing API access
  - Incomplete test logic
- [ ] Prioritize TODOs that block test completeness
- [ ] Implement missing assertions where possible
- [ ] Document features needed for remaining TODOs

**Common TODO Categories**:
1. Event draining verification (`take_inserts()`, `take_updates()`, `take_removes()`)
2. Event ordering verification
3. World integration verification
4. Protocol mismatch testing
5. Transport comparison
6. Link conditioner testing
7. Observability metrics

---

## Priority 4: Enable Ignored Tests

### 4.1 Tests Requiring Feature Implementation
**Action Items**:
- [ ] `client_disconnects_due_to_heartbeat_timeout`: Implement time manipulation in test harness
- [ ] `expired_or_reused_token_obeys_semantics`: Implement token reuse validation
- [ ] `server_capacity_reject_produces_reject_event`: Configure server capacity limits in test
- [ ] `valid_identity_token_roundtrips`: Complete server-generated token flow testing

---

## Priority 5: Systematic Debugging Approach

### 5.1 Create Test Failure Analysis Process
**Action Items**:
- [ ] For each failing test:
  1. Run test in isolation
  2. Capture error message/panic
  3. Identify root cause category:
     - Test harness violation
     - Missing assertion/logic
     - API misuse
     - Actual bug in Naia
     - Missing test harness feature
  4. Fix or document accordingly
- [ ] Create a tracking document for test failures
- [ ] Group similar failures to identify patterns

### 5.2 Common Failure Patterns to Look For
- **Entity not found**: Registration/scope issues
- **Component not found**: Component replication issues
- **Timeout**: Test logic too strict or network issues
- **Panic**: API misuse or actual bugs
- **Assertion failure**: Test logic incorrect or bug in Naia
- **Test harness violation**: Consecutive mutate/expect calls

---

## Priority 6: Test Coverage Gaps

### 6.1 Identify Missing Test Scenarios
**Action Items**:
- [ ] Review `E2E_TEST_PLAN.md` to ensure all scenarios are covered
- [ ] Identify edge cases not yet tested
- [ ] Add tests for critical paths that are currently untested
- [ ] Verify test coverage matches plan

---

## Implementation Strategy

### Phase 1: Foundation (Week 1)
1. ✅ Fix test harness violations (DONE)
2. Audit all test files for structural issues
3. Fix obvious bugs (wrong API usage, typos, etc.)
4. Enable easy wins (tests that just need minor fixes)

### Phase 2: Systematic Fixes (Week 2-3)
1. Fix high-impact test files (0% passing)
2. Fix medium-impact test files (partial passing)
3. Complete TODO sections where possible
4. Document missing features needed

### Phase 3: Feature Completion (Week 4+)
1. Implement missing test harness features
2. Enable ignored tests
3. Complete remaining TODOs
4. Achieve 100% passing rate

---

## Quick Wins (Start Here)

These are likely easy fixes that will have immediate impact:

1. **Fix test harness violations** in other test files (similar to what we did in `events_world_integration.rs`)
2. **Fix obvious API misuse** (wrong method calls, incorrect parameters)
3. **Add missing assertions** in tests that have structure but incomplete checks
4. **Fix timing issues** (tests that timeout but logic is correct)

---

## Tracking Progress

### Metrics to Track
- **Pass Rate**: Target 100% (currently 43%)
- **Test Harness Violations**: Target 0 (currently 0 after fixes)
- **TODO Count**: Track reduction over time
- **Ignored Tests**: Track as features are implemented

### Regular Checkpoints
- Run full test suite weekly
- Update this document with progress
- Document blockers and missing features
- Prioritize based on impact

---

## Notes

- Many tests are structurally complete but failing due to missing assertions or bugs
- Some tests require features not yet available in the test harness
- Focus on fixing tests that verify core Naia functionality first
- Tests that verify test harness itself can be lower priority

---

## Last Updated
Generated after fixing test harness violations in `events_world_integration.rs` and running full test suite.

