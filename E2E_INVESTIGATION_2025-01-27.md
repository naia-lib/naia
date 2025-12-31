# E2E Investigation: server_can_revoke_authority_reset Timeout

**Date:** 2025-01-27  
**Test:** `server_can_revoke_authority_reset`  
**Failing Expect:** "client observes authority granted"

## 1. Symptom

The test `server_can_revoke_authority_reset` times out after 100 ticks at the labeled expectation:
```
Scenario::expect timed out after 100 ticks: client observes authority granted
```

The client never receives the `ClientEntityAuthGrantedEvent` that should be emitted when authority is granted for a client-spawned public entity.

## 2. Final Pipeline Proof Lines

### Wire Pipeline
```
[wire] srv_sent=1 cli_recv=2 saw_set_auth=1 to_event_ok=0
```

**Interpretation:**
- `srv_sent=1`: Server successfully sent 1 world packet containing SetAuthority
- `cli_recv=2`: Client received 2 world packets from transport
- `saw_set_auth=1`: Client decoder recognized SetAuthority message KIND on wire
- `to_event_ok=0`: Conversion from `EntityMessage::SetAuthority` to `EntityEvent::SetAuthority` failed

### Protocol ID Sanity Check
```
[proto] server_set_auth_kind=8
[proto] client_set_auth_kind=8
```

**Interpretation:** Protocol kind IDs match (both 8), eliminating protocol mismatch as root cause.

## 3. Architectural Inference

**Root Cause:** SetAuthority messages arrive at the client before the client can map `RemoteEntity` → `GlobalEntity`. 

The failure occurs in `EntityMessage::to_event()` at:
```rust
let global_entity = match local_entity_map.global_entity_from_remote(&remote_entity) {
    Some(ge) => *ge,
    None => {
        error!("to_event() failed to find RemoteEntity({:?}) in entity_map! Message type: {:?}", 
            remote_entity, self.get_type());
        panic!("RemoteEntity not found in entity_map during to_event conversion");
    }
};
```

**Contract Violation:** The current implementation assumes that when a `SetAuthority` message arrives, the corresponding `RemoteEntity` must already exist in the client's `local_entity_map`. This invariant is violated when:
1. Server sends `SetAuthority` for a client-spawned entity
2. The entity's spawn/publish messages haven't been fully processed yet
3. The `RemoteEntity` → `GlobalEntity` mapping hasn't been established

**One-tick defer attempt:** A one-tick defer queue was implemented to delay `SetAuthority` emission until after spawn/publish, but this did not resolve the issue, indicating the mapping may still be unavailable or there's a deeper ordering problem.

## 4. Candidate Canonical Fixes (Ranked)

### Option 1: Buffer SetAuthority Messages Until Mapping Exists (PREFERRED)
**Approach:** Store pending `SetAuthority` messages keyed by `RemoteEntity` in a waitlist. When a `RemoteEntity` → `GlobalEntity` mapping is established (via spawn/publish processing), replay any pending authority updates for that entity.

**Pros:**
- Deterministic and robust
- Handles out-of-order delivery gracefully
- No protocol changes required
- Aligns with existing waitlist patterns in codebase

**Cons:**
- Requires maintaining a pending authority buffer
- Must handle entity lifecycle (what if entity is despawned before mapping exists?)

**Implementation Notes:**
- Similar to existing `EntityWaitlist` pattern
- Replay should happen after entity mapping is established in `take_incoming_events` or equivalent
- Must ensure replay happens before entity events are processed

### Option 2: Change SetAuthority to Reference GlobalEntity on Wire
**Approach:** Modify the wire protocol so `SetAuthority` messages include `GlobalEntity` directly instead of `RemoteEntity`, eliminating the need for local entity mapping.

**Pros:**
- Eliminates mapping dependency entirely
- Simpler client-side processing

**Cons:**
- Requires protocol change (breaking change)
- `GlobalEntity` may be larger than `RemoteEntity` (wire efficiency)
- May require protocol versioning

### Option 3: Ensure Mapping Established Earlier (Ordering Guarantees)
**Approach:** Guarantee that entity spawn/publish messages are always processed before `SetAuthority` messages, ensuring mapping exists when conversion happens.

**Pros:**
- No code changes to message handling
- Maintains current protocol

**Cons:**
- Difficult to prove globally (network reordering, jitter buffer behavior)
- Fragile - any ordering violation breaks the invariant
- One-tick defer already attempted and failed

## 5. Scope Impact

**Affected Tests:**
- `server_can_revoke_authority_reset` (confirmed failing)
- Any test involving client-spawned entities with authority grants
- Any test where `SetAuthority` may arrive before entity registration

**Likely Safe:**
- Server-owned entity authority flows (server spawns, then grants authority)
- Tests without client-spawned entities

## 6. Minimal Repro Command

```bash
cargo test -p naia-test server_can_revoke_authority_reset -- --nocapture
```

## 7. Investigation Timeline

1. **Initial Symptom:** Test timeout, zero server TX frames
2. **ACK Fix:** Implemented one-shot ACK-only latch (resolved transport)
3. **Timeout Pinpointing:** Added labeled expects → "client observes authority granted" fails
4. **Counter Chain:** Added server-side counters → `enq=0` (server never enqueued SetAuthority)
5. **Semantic Fix:** Moved auto-grant to `EntityEvent::Publish` arm → `enq=1` (server enqueues)
6. **Wire Fix:** Fixed `world_writer.rs` to handle RemoteEntity for SetAuthority → `wrote=1` (server writes)
7. **One-tick Defer:** Added defer queue → still `rx=0` (client never receives)
8. **Wire Pipeline Tracing:** Added binary counters → `saw_set_auth=1, to_event_ok=0` (conversion fails)

## 8. Key Code Locations

- **Server auto-grant:** `server/src/server/world_server.rs:2245-2302` (EntityEvent::Publish handler)
- **Server defer queue:** `server/src/server/world_server.rs:90, 610-633` (pending_auth_grants)
- **Wire serialization:** `shared/src/world/world_writer.rs:451-502` (SetAuthority writing)
- **Wire deserialization:** `shared/src/world/world_reader.rs:190-209` (SetAuthority reading)
- **Entity mapping conversion:** `shared/src/world/entity/entity_message.rs:198-232` (to_event for RemoteEntity)
- **Client event processing:** `client/src/client.rs:1761-1773` (EntityEvent::SetAuthority handler)
