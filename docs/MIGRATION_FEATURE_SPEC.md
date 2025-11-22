# Entity Migration Feature Specification

## Overview

This document specifies the requirements for implementing entity migration during delegation, allowing entities to transition from client-controlled (RemoteEntity on server) to server-managed delegated entities (HostEntity on server), and vice versa on the client side.

## Problem Statement

When a client creates an entity and later requests delegation:
- **Server side:** Entity exists as `RemoteEntity` in `RemoteEngine` (receiving client updates)
- **After delegation:** Entity must exist as `HostEntity` in `HostEngine` (sending to clients)
- **Client side:** Entity exists as `HostEntity` in `HostEngine` (sending to server)
- **After delegation:** Entity must exist as `RemoteEntity` in `RemoteEngine` (receiving server updates)

This requires **migrating the entity between engines** while preserving all state and ensuring zero data loss.

## Current State (Incomplete Implementation)

### What's Already Implemented:
1. ✅ Delegation request/response flow (EnableDelegation, EnableDelegationResponse)
2. ✅ Authority management (RequestAuthority, ReleaseAuthority, SetAuthority)
3. ✅ MigrateResponse message structure and serialization
4. ✅ Global entity ownership tracking (EntityOwner transitions)
5. ✅ Entity channel state machines for both Host and Remote

### What's Broken:
1. ❌ `migrate_entity_remote_to_host()` incomplete (line 134 references undefined variable, line 133 calls non-existent method)
2. ❌ Client-side migration not implemented (todo!() at client.rs:1681)
3. ❌ No handling of in-flight messages during migration
4. ❌ No buffered message drainage before migration
5. ❌ No component state preservation mechanism

## Architecture Context

### Entity Type System

**Connection-Relative Entity Types:**
- `GlobalEntity`: Universal identifier across system
- `HostEntity(u16)`: Local ID for entities you send updates about
- `RemoteEntity(u16)`: Local ID for entities you receive updates about
- `OwnedLocalEntity`: Tagged union of Host/Remote

**Key Insight:** Host/Remote is perspective-relative. What's HostEntity on server is RemoteEntity on client for the same logical entity.

### Engine Architecture

**HostEngine:**
- Sends entity commands in order
- Has `HostEntityChannel` per entity
- Tracks components as `HashSet<ComponentKind>` (no per-component buffering)
- Used for entities you have authority over

**RemoteEngine:**
- Receives entity messages out-of-order
- Has `RemoteEntityChannel` per entity
- Has `RemoteComponentChannel` per component (with FSM buffering)
- Used for entities others have authority over

### Channel State

**RemoteEntityChannel:**
```
state: EntityChannelState (Despawned/Spawned)
last_epoch_id: Option<MessageIndex>
component_channels: HashMap<ComponentKind, RemoteComponentChannel>
auth_channel: AuthChannel
buffered_messages: OrderedIds<EntityMessage<()>>
```

**RemoteComponentChannel:**
```
inserted: bool (current state)
last_epoch_id: Option<MessageIndex>
buffered_messages: OrderedIds<bool> (pending insert/remove operations)
```

**HostEntityChannel:**
```
component_channels: HashSet<ComponentKind> (just which exist)
auth_channel: AuthChannel
buffered_messages: OrderedIds<EntityMessage<()>>
```

### Message Flow

**Reliable Channel:**
- Messages tagged with `MessageIndex` (global sequence)
- Sender retransmits until ACKed
- Stored in `sent_command_packets` until ACK received
- Receiver deduplicates by MessageIndex
- **Messages can arrive out of order!**

## Critical Requirements

### 1. Zero Data Loss

**Requirement:** No entity state, component state, or pending operations can be lost during migration.

**Implications:**
- All buffered messages must be processed or transferred
- Component `inserted` state must be preserved
- Pending operations must be resolved
- In-flight messages must be handled correctly

### 2. Atomic Migration

**Requirement:** From external observer's perspective, migration appears instantaneous.

**Implications:**
- Entity must not exist in both engines simultaneously
- Entity must not be unreachable during migration
- LocalEntityMap must always have valid mapping
- No race windows where messages are lost

### 3. Handle In-Flight Messages

**Requirement:** Messages sent before migration but received after must be handled correctly.

**Scenarios:**
- Server sends `InsertComponent(RemoteEntity(42))` at T0
- Server migrates entity at T1: RemoteEntity(42) → HostEntity(100)
- Client receives message at T2 (after own migration complete)
- Message references old entity ID but must apply correctly

**Solution Approach:** Entity redirect tables on both sides

### 4. Preserve Causality

**Requirement:** Message ordering guarantees must be maintained across migration.

**Implications:**
- Buffered operations reflect intended order
- MessageIndex sequences must be respected
- FSM state transitions must be valid

### 5. No Waiting/Blocking

**Requirement:** Migration must complete in single tick without waiting for network.

**Implications:**
- Cannot wait for ACK of all messages (use redirects instead)
- Cannot wait for buffers to drain naturally (force-drain instead)
- Must handle edge cases synchronously

## What Needs to Be Implemented

### 1. Entity Redirect Tables

**Purpose:** Map old entity IDs to new entity IDs for messages in-flight during migration.

**Server Side:**
- After migrating RemoteEntity(42) → HostEntity(100)
- Store redirect: `RemoteEntity(42)` → `HostEntity(100)`
- When writing commands: check redirect table and update entity reference
- When reading ACKs: accept ACKs for either old or new entity

**Client Side:**
- After receiving MigrateResponse with old=HostEntity(42), new=RemoteEntity(100)
- Store redirect: `HostEntity(42)` → `RemoteEntity(100)`
- When reading messages: check redirect table and update entity reference
- When writing commands: use new entity ID

**Expiry:**
- Keep redirects for COMMAND_RECORD_TTL (60 seconds)
- Clean up expired redirects automatically

### 2. Update Sent Command References

**Purpose:** Ensure retransmitted messages use new entity ID.

**Implementation:**
- Scan `sent_command_packets` for messages referencing migrated entity
- Update `EntityMessage<OwnedLocalEntity>` to use new entity ID
- Future retransmissions will use correct entity

### 3. Force-Drain Buffered Messages

**Purpose:** Resolve all pending operations before migration to ensure clean state.

**Server Side (RemoteEntityChannel):**
- Iterate through `buffered_messages`
- Force-apply each message regardless of FSM state
- For each ComponentChannel: force-resolve buffered insert/remove operations
- Apply operations to game world immediately
- After draining: all buffers empty, clean state for migration

**Client Side (HostEntityChannel):**
- Host channels have minimal buffering
- Drain any buffered auth messages
- Should typically be empty (host sends, doesn't buffer receives)

**Rationale:** Accepting brief potential inconsistency during forced drainage is acceptable because:
- Entity is undergoing major state change (delegation)
- Eventual consistency via continued replication will fix any issues
- Zero data loss is more important than perfect FSM adherence

### 4. Extract and Transfer Component State

**Purpose:** Preserve which components exist on the entity.

**Server Side (Remote → Host):**
- Extract `component_channels.keys()` from RemoteEntityChannel
- Filter to only components where `inserted == true`
- Create HostEntityChannel with `component_channels: HashSet<ComponentKind>`
- Result: both engines agree on which components exist

**Client Side (Host → Remote):**
- Extract `component_channels` from HostEntityChannel (already just HashSet)
- Create RemoteEntityChannel with ComponentChannels for each
- Set `inserted = true` for each component
- Result: Remote channels initialized with current component state

### 5. Server-Side Migration (Remote → Host)

**What Happens:**
1. Receive `EnableDelegationResponse` from client
2. Force-drain all buffered messages in RemoteEntityChannel
3. Extract component state (which components inserted)
4. Remove entity from RemoteEngine (extract channel)
5. Remove from LocalEntityMap
6. Generate new HostEntity ID
7. Create HostEntityChannel with extracted component state
8. Insert into HostEngine
9. Update LocalEntityMap with new mapping
10. Install entity redirect: old RemoteEntity → new HostEntity
11. Update all sent_command_packets references
12. Queue MigrateResponse to client
13. Update global ownership (EntityOwner → Server)
14. Enable delegation in GlobalWorldManager
15. Grant authority to requesting client

**Invariants:**
- Entity must be in Spawned state
- ReplicationConfig must be Public before migration
- Entity must be client-owned
- No critical errors during force-drain

### 6. Client-Side Migration (Host → Remote)

**What Happens:**
1. Receive `MigrateResponse(old_host_entity, new_remote_entity)`
2. Buffer any outgoing commands from HostEntityChannel
3. Force-drain buffered messages (should be empty)
4. Extract component state
5. Remove entity from HostEngine (extract channel)
6. Remove from LocalEntityMap
7. Use RemoteEntity ID from MigrateResponse
8. Create RemoteEntityChannel with extracted component state
9. Insert into RemoteEngine
10. Update LocalEntityMap with new mapping
11. Install entity redirect: old HostEntity → new RemoteEntity
12. Update sent_command_packets references
13. Complete delegation (mark as Delegated, register with auth handler)
14. Set authority status to Granted
15. Re-validate and re-queue buffered commands to new RemoteEntityChannel
16. Emit AuthGrant event

**Invariants:**
- Must have received valid MigrateResponse
- Old HostEntity must exist in engine
- New RemoteEntity ID must be valid

### 7. Buffered Command Replay (Client Only)

**Purpose:** Preserve commands client queued before migration completed.

**What Happens:**
- Client may have queued mutations while MigrateResponse in flight
- These were in HostEntityChannel.outgoing_commands
- After migration, need to re-queue to RemoteEntityChannel
- Re-validate each command (some may be invalid in delegated context)
- Drop invalid commands (e.g., Publish/Unpublish)
- Queue valid commands to new channel

### 8. New Helper Methods Required

**RemoteEngine:**
- `remove_entity_channel(&RemoteEntity) -> RemoteEntityChannel`
- `force_drain_channel(&RemoteEntity)` 

**HostEngine:**
- `remove_entity_channel(&HostEntity) -> HostEntityChannel`

**RemoteEntityChannel:**
- `force_drain_all_buffers()`
- `extract_component_kinds() -> HashSet<ComponentKind>`
- `get_state() -> EntityChannelState`

**HostEntityChannel:**
- `new_with_components(HostType, HashSet<ComponentKind>) -> Self`
- `extract_outgoing_commands() -> Vec<EntityCommand>`

**LocalWorldManager:**
- `install_entity_redirect(OwnedLocalEntity, OwnedLocalEntity)`
- `apply_entity_redirect(OwnedLocalEntity) -> OwnedLocalEntity`
- `update_sent_command_entity_refs(GlobalEntity, OwnedLocalEntity, OwnedLocalEntity)`
- `force_drain_entity_buffers(GlobalEntity)`

## Non-Requirements

### What We Don't Need to Do:

1. **Full Component Sync After Migration**
   - Buffered operations are force-drained (not discarded)
   - Component state is preserved exactly
   - No divergence, so no sync needed

2. **Wait for ACKs**
   - Redirect tables handle in-flight messages
   - No blocking on network round-trip

3. **State Machine for Multi-Tick Migration**
   - Single-tick atomic operation
   - Force-draining eliminates async waiting

4. **Remap MessageIndex**
   - MessageIndex is global to connection, not per-entity
   - Stays the same across migration
   - Only entity references change

5. **Handle Authority Already Granted**
   - Not an edge case
   - Server atomically enables delegation and grants authority
   - No race window exists

## Edge Cases to Handle

### EC1: Client Commands During Migration Window
**Issue:** Client queues commands while MigrateResponse in flight  
**Solution:** Buffer and replay after migration (client-side only)

### EC2: In-Flight Messages Arrive After Migration
**Issue:** Message references old entity ID  
**Solution:** Entity redirect tables on both sides

### EC6: Buffered Messages in Channels
**Issue:** Operations waiting in channel buffers  
**Solution:** Force-drain all buffers before migration

### EC7: Component State Preservation
**Issue:** Component inserted state must transfer  
**Solution:** Extract and transfer component_kinds HashSet

### EC9: Component Operations During Migration
**Issue:** Component insert/remove arrives during migration  
**Solution:** Force-drain queues operation, redirect table handles late arrivals

### EC10: Component Channel Buffered Operations
**Issue:** Pending insert/remove in ComponentChannels  
**Solution:** Force-resolve all buffered operations before migration

### EC11: Authority Requests During Migration
**Issue:** Client tries to request authority on migrating entity  
**Solution:** Migration is single-tick, no window for conflict

## Success Criteria

### Functional:
- ✅ Client can create entity, publish, and enable delegation
- ✅ Server successfully migrates entity from Remote to Host
- ✅ Client successfully migrates entity from Host to Remote
- ✅ Authority is granted correctly after migration
- ✅ Component state is preserved exactly
- ✅ Client can mutate delegated entity and updates replicate
- ✅ Other clients receive updates for delegated entity

### Non-Functional:
- ✅ Migration completes in single tick (< 16ms)
- ✅ Zero data loss (all operations preserved)
- ✅ No race conditions or deadlocks
- ✅ Handles packet loss gracefully (via redirect tables)
- ✅ No memory leaks (redirect tables expire)

## Testing Strategy

### Unit Tests:
- Entity redirect table operations
- Force-drain buffer mechanics
- Component state extraction
- sent_command_packets reference updates

### Integration Tests:
- Full client-originated delegation flow
- Migration with in-flight messages
- Migration with buffered operations
- Packet loss during migration
- Multiple clients with delegated entity

### Manual Tests:
- Create entity → publish → delegate → request authority → mutate
- Verify updates replicate to other clients
- Test with poor network conditions
- Test rapid delegation enable/disable cycles

## Open Questions

None! All architectural decisions have been made through rigorous analysis.

## References

- Original incomplete implementation: `shared/src/world/local/local_world_manager.rs:117-136`
- Client-side todo: `client/src/client.rs:1681`
- Server delegation handler: `server/src/server/world_server.rs:1499-1586`
- Entity channel architecture: `shared/src/world/sync/`
- Reliable message channel: `shared/src/messages/channels/`

---

**Document Status:** Complete  
**Last Updated:** [Generated]  
**Next Steps:** Create detailed implementation plan with specific code changes

