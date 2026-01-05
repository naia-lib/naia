# Next Steps - E2E Test Work

## Current Status

**Last Updated:** 2025-01-27

This document tracks the next items to work on after the instrumentation cleanup and e2e_trace! infrastructure work.

---

## Immediate Next Steps

### 1. Fix Pre-existing Test Failure
**Priority:** Medium  
**Status:** Known failure, needs investigation

- **Test:** `stable_logical_identity_across_clients_in_steady_state` (in `entities_lifetime_identity.rs`)
- **Issue:** Test timeout after 200 ticks without satisfying condition
- **Ownership:** Pre-existing failure (unrelated to instrumentation changes)
- **Action:** Investigate entity identity mapping logic that causes the timeout
- **Note:** The `entity_delegation_toggle` suite passes, confirming instrumentation changes don't affect core functionality

### 3. Review Untracked Test Files
**Priority:** Medium  
**Status:** Files exist but not tracked

The following test files are untracked (may need to be added or reviewed):
- `test/tests/entity_authority_client_ops.rs`
- `test/tests/entity_authority_server_ops.rs`
- `test/tests/entity_client_owned.rs`
- `test/tests/entity_migration_and_events.rs`
- `test/tests/entity_scope_coupling.rs`

**Action:** Review these files to determine if they should be:
- Added to git (if they're complete and working)
- Completed/implemented (if they're partial)
- Removed (if they're obsolete)

---

## Known Issues & Technical Debt

### Instrumentation
- [ ] Handle JITTER_BUFFER/HUB println! statements in separate codebase (socket backends)
  - These are currently excluded from hygiene checks but should be addressed
  - Located in: `client/src/connection/jitter_buffer.rs`, `shared/src/transport/local/hub.rs`

### Test Infrastructure
- [ ] Address pre-existing test failures beyond `stable_logical_identity_across_clients_in_steady_state`
  - Review `test/E2E_TEST_AUDIT.md` for full list of known failures
  - Many tests are marked as TODO or partially implemented

### Code Quality
- [ ] Fix compiler warnings (unused imports, dead code, etc.)
  - Multiple warnings in test files (unused imports)
  - Some dead code warnings in harness code

---

## Future Work Items

### Test Coverage
- Continue implementing remaining TODO tests from `E2E_TEST_PLAN.md`
- Focus on high-value test scenarios that are currently marked TODO
- Improve test harness capabilities where needed

### Documentation
- Update any documentation that references old instrumentation patterns
- Ensure new e2e_trace! usage is documented for future developers

### Performance
- Review instrumentation overhead (should be zero in non-debug builds)
- Consider if any trace points are too verbose or need adjustment

---

## Test Suite Status

### Overall Test Status (from E2E_TEST_AUDIT.md)
- **Total Tests in Plan:** 130
- **Fully Implemented:** ~95 tests (73%)
- **TODO/Partial:** ~35 tests (27%)
- **Currently Passing:** 56 tests (43%)
- **Currently Failing:** 61 tests (47%)
- **Ignored:** 5 tests (4%) - require features not yet available

### Test Files Status
- ✅ `time_ticks_transport`: 24 passed, 0 failed ✅
- ✅ `harness_scenarios`: 2 passed, 0 failed ✅
- ✅ `entity_delegation_toggle`: 7 passed, 0 failed ✅ (newly added)
- ⚠️ `entities_lifetime_identity`: 9 passed, 1 failed, 1 ignored
- ⚠️ `connection_auth_identity`: 10 passed, 5 failed, 4 ignored
- ⚠️ `events_world_integration`: 8 passed, 10 failed
- ⚠️ `messaging_channels`: 9 passed, 9 failed
- ⚠️ `protocol_schema_versioning`: 6 passed, 1 failed
- ⚠️ `rooms_scope_snapshot`: 0 passed, 15 failed
- ⚠️ `integration_transport_parity`: 2 passed, 1 failed

### High-Priority Test Domains (from E2E_TEST_AUDIT.md)
1. **Domain 4 (Ownership & Delegation):** 10/12 implemented (83%) - Authority/delegation tests
2. **Domain 3 (Entities & Identity):** 11/11 implemented (100%) - Some may need fixes
3. **Domain 1 (Connection & Auth):** 14/14 implemented (100%) - Mostly passing
4. **Domain 2 (Rooms & Scope):** 15/15 implemented (100%) - Some failing

### Low-Priority Test Domains (blocked on features)
- **Domain 6 (Time, Ticks, Transport):** 1/26 implemented (4%) - Most require link conditioner
- **Domain 7 (Protocol & Schema):** 2/7 implemented (29%) - Most require protocol mismatch
- **Domain 9 (Transport Parity):** 0/3 implemented (0%) - Requires transport comparison

## Notes

- All instrumentation is now gated by `cfg(feature="e2e_debug")` only (no environment variables)
- The 4 allowed trace classes are:
  1. Server SEND: EnableDelegation / DisableDelegation / SetAuthority
  2. Client RECV+APPLY: EnableDelegation / DisableDelegation / SetAuthority
  3. AuthChannel validate/apply: from_status -> to_status
  4. Client DisableDelegation handler entry: delegated_at_entry=...

- Deleted files (E2E_INVESTIGATION_2025-01-27.md, previous NEXT_STEPS.md, ownership_delegation.rs) were intentional cleanup
- See `test/E2E_TEST_AUDIT.md` for comprehensive test status and `test/E2E_TEST_PLAN.md` for original test plan
