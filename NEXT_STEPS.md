# Next Steps: Reproducing the Authority Re-Acquisition Bug

## Context

The local transport implementation was built to enable TRUE end-to-end tests for reproducing **Bug #7: Authority Status Mismatch After Migration** from the Cyberlith Editor.

## The Bug

**Scenario in Cyberlith Editor:**
1. Create a vertex (entity)
2. Deselect it (releases authority)
3. Reselect it (requests authority again)
4. **BUG**: Authority is never granted → "No authority over vertex, skipping" messages

**Root Cause:**
After client-side migration (HostEntity → RemoteEntity), the `RemoteEntityChannel`'s internal `AuthChannel` had incorrect state:
- State: `Unpublished` (should be `Delegated`)
- Auth Status: `None` (should be `Available`)

**Status:** Fix already implemented in production (see `TEST_COVERAGE_GAPS_AND_FIXES.md`)

## Current Status

### ✅ Complete
1. **Local transport implementation** (Phase 1-6)
   - HTTP-style auth
   - Server address discovery
   - Async runtime support
   - Link conditioner integration
   - All local transport E2E tests passing (6/6)

2. **Basic E2E test infrastructure**
   - `e2e_client_server_handshake` test passing
   - Client-server connection working with local transport
   - Auth flow working correctly

### 🚧 In Progress
1. **`e2e_authority_release_and_reacquire`** test
   - Handshake complete ✅
   - Need to implement:
     - Entity creation and publishing
     - Delegation request/response
     - Authority request/grant/release cycle
     - **Critical**: Re-request authority after release (Bug #7 repro)

### ❌ Not Started
1. **Other E2E tests**
   - `e2e_entity_id_conversion_bug.rs` - Needs full implementation
   - `e2e_client_delegation_authority_bug.rs.disabled` - Needs to be enabled and completed
   - `e2e_authority_release_and_reacquire.rs` - Unit test, not E2E

## Implementation Path

### Step 1: Complete `e2e_authority_release_and_reacquire` Test

This is the critical test that will reproduce Bug #7 in a controlled environment.

**Required API:**
```rust
// Client API needed:
client.spawn_entity(world, protocol_kind)  // Create entity
client.entity_enable_delegation(entity)     // Request delegation
client.entity_request_authority(entity)     // Request authority
client.entity_release_authority(entity)     // Release authority
client.entity_has_authority(entity)         // Check authority status

// Server API needed:
server.accept_delegation(user_key, entity)  // Accept delegation request
server.grant_authority(user_key, entity)    // Grant authority
```

**Test Flow:**
1. ✅ Complete handshake (working)
2. Client creates entity and publishes it
3. Client enables delegation
4. Server receives delegation request and accepts it
5. Client receives MigrateResponse
6. Client requests authority (first time)
7. Server grants authority
8. Verify: Client can modify entity
9. Client releases authority
10. Verify: Client cannot modify entity
11. **Critical**: Client requests authority AGAIN (Bug #7 should appear if not fixed)
12. Server grants authority
13. Verify: Client can modify entity again

**Success Criteria:**
- Test passes if all authority operations complete correctly
- Test reproduces Bug #7 if the fix is reverted

### Step 2: Implement Entity Creation & Delegation

Look at how the Cyberlith Editor creates vertices:
```rust
// Typical editor flow:
let entity = world.spawn(Position::new(x, y));
client.spawn_entity(&entity);
client.entity_publish(&entity);
client.entity_enable_delegation(&entity);
```

**Questions to Answer:**
1. What protocol kind should we use for test entities?
2. How does `spawn_entity` differ from `entity_publish`?
3. When should we call `entity_enable_delegation`?
4. How do we wait for delegation to complete?

### Step 3: Implement Authority Request/Release Cycle

```rust
// Authority lifecycle:
client.entity_request_authority(&entity);
// ... wait for server response ...
assert!(client.entity_has_authority(&entity));

client.entity_release_authority(&entity);
assert!(!client.entity_has_authority(&entity));

// THE CRITICAL TEST - Re-request after release:
client.entity_request_authority(&entity);
// ... wait for server response ...
assert!(client.entity_has_authority(&entity)); // This should pass!
```

**Questions to Answer:**
1. How do we wait for authority grant responses?
2. How do we verify authority state on both client and server?
3. How do we check BOTH the global tracker and RemoteEntityChannel state?

## Reference Documentation

### Key Files
- **Test Design**: `docs/TEST_COVERAGE_GAPS_AND_FIXES.md` - Bug #7 details
- **Migration Spec**: `docs/MIGRATION_FEATURE_SPEC.md` - Architecture overview
- **Implementation**: `docs/MIGRATION_IMPLEMENTATION_PLAN.md` - Technical details
- **Current Test**: `test/tests/e2e_authority_bug_real.rs` - In-progress test
- **Unit Test**: `test/tests/e2e_authority_release_and_reacquire.rs` - Reference impl

### Critical Code Locations
- **Client MigrateResponse handler**: `client/src/client.rs` (Bug #7 fix location)
- **RemoteEntityChannel**: `shared/src/world/sync/remote_entity_channel.rs`
- **AuthChannel**: `shared/src/world/sync/auth_channel.rs`
- **LocalWorldManager**: `shared/src/world/local/local_world_manager.rs`

## Next Actions

1. **Examine client API** for entity lifecycle:
   ```bash
   grep -r "fn spawn_entity\|fn entity_enable_delegation\|fn entity_request_authority" client/src/
   ```

2. **Examine server API** for delegation handling:
   ```bash
   grep -r "delegation\|authority" server/src/ | grep "pub fn"
   ```

3. **Study the working unit test** (`e2e_authority_release_and_reacquire.rs`) to understand the lower-level API

4. **Implement Step 2** of the test (entity creation + delegation)

5. **Implement Step 3** of the test (authority lifecycle)

6. **Verify the test fails** if Bug #7 fix is reverted (regression test)

## Expected Outcome

After completing this test:
- We'll have a TRUE end-to-end regression test for Bug #7
- The test will prove the fix works in production-like scenarios
- Future changes that break authority re-acquisition will be caught immediately
- We'll have a template for implementing the other E2E tests

## Timeline Estimate

- **Entity creation API investigation**: 30-60 min
- **Implement entity creation + delegation**: 1-2 hours
- **Implement authority lifecycle**: 1-2 hours
- **Debug and verify**: 1-2 hours
- **Total**: 3-6 hours

## Success Metrics

- [x] Local transport implementation complete
- [x] Basic E2E handshake working
- [ ] Full authority lifecycle test implemented
- [ ] Test reproduces Bug #7 when fix is reverted
- [ ] Test passes with current codebase
- [ ] Other E2E tests implemented using same pattern

---

**Status**: Ready to proceed with entity creation and delegation implementation.

