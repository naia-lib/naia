# Naia Development Plan

**Status:** Active - Phase 3 (Fill Coverage Gaps)
**Updated:** 2026-01-12
**Goal:** 100% spec coverage, feature-complete implementation

---

## Current State

| Metric | Value | Target |
|--------|-------|--------|
| Contract coverage | 176/185 (95%) | 185/185 (100%) |
| Covered contracts | 176 | 185 |
| Blocked by impl bug | 0 | 0 |
| Need harness extension | 9 | 0 |

---

## Session 2026-01-12 Progress

### Completed
1. **Fixed entity-authority-11/12 implementation bug** ✓
   - Fixed 3 locations in `remote_world_manager.rs` (graceful error handling for missing entities)
   - Added `user_is_authority_holder()` helper to `server_auth_handler.rs`
   - Added automatic authority release when holder loses scope in `world_server.rs`
   - Test `out_of_scope_ends_authority_for_that_client` now passes 3x

2. **Implemented 8 test stubs in entity_authority_server_ops.rs**
   - 3 tests pass: `duplicate_authority_signals_are_idempotent`, `out_of_scope_ends_authority_for_that_client`, `give_authority_assigns_to_client_and_denies_everyone_else`
   - 5 tests need API investigation (see below)

3. **Added e2e_debug documentation** to CLAUDE.md, DEV_PROCESS.md, and PLAN.md

### In Progress: API Investigation Needed
Several tests fail due to API semantics misunderstanding:
- `take_authority()` - Sets authority to None/Available, not Server-held Denied
- `release_authority()` - Only works when server is holder, not when client holds

**Tests needing fixes:**
- `take_authority_forces_server_hold_all_clients_denied` - timeout
- `server_held_authority_is_indistinguishable_from_client_is_denied` - timeout
- `server_priority_take_authority_overrides_a_client_holder` - timeout
- `server_release_authority_clears_holder_all_clients_available` - NotHolder error
- `former_holder_sees_granted_to_available_on_server_release` - NotHolder error

---

## Immediate Next Actions

### Priority 1: Fix Remaining Server Ops Tests
Investigate and fix the 5 failing tests by understanding the actual API semantics:

```bash
# Run with debug to understand flow
cargo test --package naia-test --features e2e_debug server_release_authority -- --nocapture

# Check API documentation
grep -A 20 "fn take_authority" server/src/world/entity_mut.rs
grep -A 20 "fn release_authority" server/src/world/entity_mut.rs
```

**Key question:** Does `release_authority()` release any holder, or only server-held authority?

### Priority 2: Assess Observability APIs (Unblocks 9 contracts)

**Contracts:** observability-01 through observability-09

**Steps:**
```
1. Grep for existing metrics APIs:
   grep -rn "rtt\|throughput\|connection.*count\|latency" server/src/ client/src/

2. For each observability contract, check if API exists:
   - observability-01: Connection count → server.users_count()?
   - observability-02: Disconnection count → track in test?
   - observability-03: Message throughput → needs harness
   - observability-04: RTT → client.rtt()?
   - observability-05: Latency → needs harness
   - observability-06: Entity count → server.entities_count()?

3. If API exists: Write test
4. If API missing: Design minimal harness extension
```

**Harness Extension Option (if needed):**
```rust
#[cfg(feature = "test-metrics")]
impl Server {
    pub fn test_get_metrics(&self) -> TestMetrics { ... }
}
```

---

### Priority 3: Full Validation

After above priorities complete:

```bash
# 1. Run full test suite 3x
cargo test --package naia-test
cargo test --package naia-test
cargo test --package naia-test

# 2. Verify coverage
./specs/spec_tool.sh coverage  # Should show 185/185 (100%)

# 3. Regenerate artifacts
./specs/spec_tool.sh traceability
./specs/spec_tool.sh bundle

# 4. Quality gates
cargo clippy --no-deps
cargo fmt -- --check
```

---

## Phase Progress

| Phase | Status | Key Deliverable |
|-------|--------|-----------------|
| Phase 1: Establish Traceability | **DONE** | All 154 tests annotated with contracts |
| Phase 2: Gap Analysis | **DONE** | GAP_ANALYSIS.md created, 83 gaps identified |
| Phase 3: Fill Coverage Gaps | **95% DONE** | 176/185 covered, 9 remaining |
| Phase 4: Spec Refinements | Pending | Orphan MUSTs → contracts |
| Phase 5: Automation & CI | Pending | GitHub Actions workflow |

---

## Completion Checklist

```
[x] Phase 1: Annotate all existing tests with contract IDs
[x] Phase 2: Generate gap analysis and prioritize
[x] Phase 3a: Cover high-risk contracts (authority, delegation, publication)
[x] Phase 3b: Fix entity-authority-11/12 implementation bug (2026-01-12)
[ ] Phase 3c: Fix remaining server ops tests (5 failing - API semantics)
[ ] Phase 3d: Assess and test observability contracts
[ ] Verify 185/185 coverage
[ ] Run full test suite 3x for flakiness
[ ] Phase 4: Resolve orphan MUST statements
[ ] Phase 5: Add CI pipeline for spec validation
```

---

## Test Files by Domain

| Domain | Test File | Status |
|--------|-----------|--------|
| connection-* | `connection_auth_identity.rs` | Mostly covered |
| entity-authority-* | `entity_authority_server_ops.rs`, `entity_authority_client_ops.rs` | 3/10 pass, 5 need API fix |
| entity-delegation-* | `entity_delegation_toggle.rs`, `entity_migration_and_events.rs` | Covered |
| entity-publication-* | `entity_client_owned.rs` | Covered |
| entity-replication-* | `entities_lifetime_identity.rs` | Mostly covered |
| entity-scopes-* | `rooms_scope_snapshot.rs`, `entity_scope_coupling.rs` | Covered |
| messaging-* | `messaging_channels.rs`, `protocol_schema_versioning.rs` | Covered |
| observability-* | (none yet) | 9 uncovered |
| server-events-* | `events_world_integration.rs` | Mostly covered |
| client-events-* | `events_world_integration.rs` | Mostly covered |
| time-*, commands-* | `time_ticks_transport.rs` | Covered |
| transport-* | `integration_transport_parity.rs` | Fully covered |

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

1. **No timing hacks** - Use `expect()` polling, not `sleep()`
2. **Reordering-tolerant** - Tests must pass regardless of async message order
3. **Contract annotations required** - Every test needs `/// Contract: [id]`
4. **Minimal assertions** - Test exactly what the contract specifies
5. **Run 3x** - Verify no flakiness before marking complete

---

## Session Workflow

```bash
# Start of session
./specs/spec_tool.sh coverage          # Check current state
grep -r "todo!" test/tests/*.rs        # Find blocked tests

# Working on contracts
grep -l "entity-authority" test/tests/*.rs  # Find relevant test files
# Read similar tests for patterns
# Write/fix test
cargo test --package naia-test --test <file>  # Run test file

# Debugging failing tests (enable detailed tracing)
cargo test --package naia-test --features e2e_debug <test_name> -- --nocapture

# End of session
./specs/spec_tool.sh coverage          # Verify improvement
./specs/spec_tool.sh traceability      # Update matrix
```

### e2e_debug Feature

When debugging test failures, enable `e2e_debug` for detailed network event tracing:

```bash
cargo test --package naia-test --features e2e_debug <test_name> -- --nocapture
```

This outputs `[SERVER_SEND]` and `[CLIENT_RECV]` events showing entity IDs, authority states, and code locations. Useful for understanding message flow and state transitions.
