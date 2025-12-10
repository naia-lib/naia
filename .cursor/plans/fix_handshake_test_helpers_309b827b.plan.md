---
name: Fix Handshake Test Helpers
overview: Add ClientKey-centric harness APIs with automatic UserKey mapping during simulation tick. Process events in Scenario::expect before creating ExpectCtx. Provide Option-returning read_event methods.
todos:
  - id: add-pending-auths
    content: Add pending_auths HashMap to Scenario for tracking auth payloads
    status: pending
  - id: fix-client-start
    content: "Fix broken client_start: remove handshake, store auth in pending_auths"
    status: pending
  - id: add-event-processing
    content: Add event processing in Scenario::expect to translate AuthEvent/ConnectEvent to ClientKey
    status: pending
  - id: add-register-method
    content: Add register_client_user method with normal &mut self
    status: pending
  - id: update-expect-ctx
    content: Add auth_events and connect_events vectors to ExpectCtx
    status: pending
  - id: add-read-event
    content: Add read_event to ServerExpectCtx returning Option types from translated events
    status: pending
  - id: add-accept-connection
    content: Add accept_connection(&ClientKey) bridge to ServerMutateCtx
    status: pending
  - id: delete-handshake
    content: Delete complete_handshake_with_name and all handshake helper methods
    status: pending
  - id: verify-tests
    content: Verify compilation and test execution success
    status: pending
---

# Fix Handshake Test Helpers - Complete Implementation Plan

## Goal

Make `test/tests/harness_scenarios.rs` compile and all tests pass by implementing the missing harness APIs that tests expect, with automatic ClientKey ↔ UserKey mapping hidden from test code.

## Test Requirements (from harness_scenarios.rs)

Line 122: `server.make_room().key()` → **Already works**

Line 133: `server.read_event::<AuthEvent<Auth>>()` → Must return `Option<(ClientKey, Auth)>`

Line 145: `server.accept_connection(&client_key)` → Must accept `ClientKey`

Line 152: `server.read_event::<ServerConnectEvent>()` → Must return `Option<ClientKey>`

Line 164: `room_mut(&room_key).add_user(&client_key)` → **Already works**

## Architecture: Event Processing Pipeline

**Flow**:

1. `client_start()` stores Auth in pending_auths, client has no UserKey yet
2. `expect()` ticks simulation → collects events → processes events → creates ExpectCtx
3. Event processing (with `&mut self`):

   - AuthEvent arrives with UserKey → match Auth to find ClientKey → establish mapping → store as `(ClientKey, Auth)`
   - ConnectEvent arrives with UserKey → lookup ClientKey → store as ClientKey

4. `ServerExpectCtx.read_event()` returns from pre-translated vectors as Option
5. Tests can then use ClientKey in other APIs (accept_connection, add_user, etc.)

**Result**: No RefCell, all mapping during tick with normal `&mut self`, UserKey never exposed.

## Implementation Steps

### 1. Add pending connection tracking to Scenario

**File**: `test/src/harness/scenario.rs`

Add field:

- `pending_auths: HashMap<ClientKey, Auth>`

Initialize in `new()`:

- `pending_auths: HashMap::new()`

Purpose: Track which Auth payloads correspond to which ClientKeys for matching incoming AuthEvents.

### 2. Fix broken client_start method

**File**: `test/src/harness/scenario.rs`

Current state: BROKEN - calls deleted `complete_handshake_with_name` with non-existent `main_room`

Fix to:

- Remove `complete_handshake_with_name` call entirely
- Remove manual mapping updates (lines 163-164)
- Clone and store auth in pending_auths: `self.pending_auths.insert(client_key, auth.clone());`
- Insert ClientState with no user_key: `ClientState::new(client, world)`
- Return ClientKey immediately

Result: Method compiles, creates connected client, defers handshake to test code.

### 3. Add event processing to Scenario::expect loop

**File**: `test/src/harness/scenario.rs`

In the expect loop, after collecting events but BEFORE creating ExpectCtx:

Process AuthEvents:

- Iterate through `server_events.read::<AuthEvent<Auth>>()`
- For each `(user_key, auth)`: match auth against `pending_auths` by comparing username/password
- When match found: call `register_client_user(client_key, user_key)` to establish mapping
- Store `(client_key, auth)` in `Vec<(ClientKey, Auth)>`

Process ConnectEvents:

- Iterate through `server_events.read::<ConnectEvent>()`
- For each `user_key`: lookup `client_key` in `user_to_client_map`
- If found, store `client_key` in `Vec<ClientKey>`

Pass translated events to ExpectCtx constructor.

### 4. Add register_client_user internal method

**File**: `test/src/harness/scenario.rs`

Add private method with normal `&mut self`:

- `fn register_client_user(&mut self, client_key: ClientKey, user_key: UserKey)`

Implementation:

- Update `ClientState.user_key_opt` via `clients.get_mut(&client_key).set_user_key(user_key)`
- Insert into `client_user_map`: `self.client_user_map.insert(client_key, user_key)`
- Insert into `user_to_client_map`: `self.user_to_client_map.insert(user_key, client_key)`
- Remove from pending: `self.pending_auths.remove(&client_key)`

Purpose: Establish bidirectional mapping after matching AuthEvent to ClientKey.

### 5. Update ExpectCtx to carry translated events

**File**: `test/src/harness/expect_ctx.rs`

Add fields:

- `auth_events: Vec<(ClientKey, Auth)>`
- `connect_events: Vec<ClientKey>`

Update constructor signature:

- `pub(crate) fn new(scenario, server_events, client_events_map, auth_events, connect_events)`

Update `server()` method to pass event vectors:

- `ServerExpectCtx::new(self.scenario, &mut self.server_events, &mut self.auth_events, &mut self.connect_events)`

Purpose: Provide pre-translated ClientKey-based events to ServerExpectCtx.

### 6. Add read_event to ServerExpectCtx

**File**: `test/src/harness/server_expect_ctx.rs`

Add fields for translated events:

- `auth_events: &'a mut Vec<(ClientKey, Auth)>`
- `connect_events: &'a mut Vec<ClientKey>`

Update constructor to accept these parameters.

Add method returning Option types:

- `pub fn read_event<E>(&mut self) -> Option<E::HarnessReturn>`

For `AuthEvent<Auth>`:

- Check if `self.auth_events` is empty
- If not empty: pop first element and return `Some((client_key, auth))`
- If empty: return `None`

For `ConnectEvent`:

- Check if `self.connect_events` is empty
- If not empty: pop first element and return `Some(client_key)`
- If empty: return `None`

Implementation approach: Use trait-based dispatch or provide specific helper methods that generic `read_event` delegates to.

### 7. Add accept_connection bridge to ServerMutateCtx

**File**: `test/src/harness/server_mutate_ctx.rs`

Add method accepting ClientKey:

- `pub fn accept_connection(&mut self, client_key: &ClientKey)`

Implementation:

- Get scenario reference
- Look up UserKey: `let user_key = scenario.user_key(client_key);` (panics if not mapped)
- Get mutable server access
- Call Naia API: `server.accept_connection(&user_key)`

Error handling: If ClientKey not mapped, `user_key()` panics with message about needing to read AuthEvent first.

### 8. Delete handshake helper methods

**File**: `test/src/harness/scenario.rs`

Remove entire methods (no longer needed):

- `complete_handshake_with_name` (~lines 642-693)
- `process_server_auth_events` (~lines 695-707)
- `add_user_to_room_if_ready` (~lines 709-717)
- `process_client_connection` (~lines 719-747)

Remove constants:

- `HANDSHAKE_MAX_ATTEMPTS` (line 62)
- `HANDSHAKE_TICK_ADVANCE_MS` (line 63)

Remove unused imports if any (e.g., `ConnectEvent as ClientConnectEvent` from naia_client).

Result: Clean harness without automatic handshake logic.

## Event Processing Pseudocode

In `Scenario::expect` after taking events:

```rust
let mut auth_events = Vec::new();
let mut connect_events = Vec::new();

// Process AuthEvents: match to ClientKey and establish mapping
for (user_key, auth) in server_events.read::<AuthEvent<Auth>>() {
    // Find ClientKey by matching Auth payload
    if let Some((&client_key, _)) = self.pending_auths.iter()
        .find(|(_, pending)| 
            pending.username == auth.username && 
            pending.password == auth.password) 
    {
        // Establish mapping
        self.register_client_user(client_key, user_key);
        // Store translated event
        auth_events.push((client_key, auth));
    }
}

// Process ConnectEvents: translate UserKey to ClientKey
for user_key in server_events.read::<ConnectEvent>() {
    if let Some(&client_key) = self.user_to_client_map.get(&user_key) {
        connect_events.push(client_key);
    }
}

// Create ExpectCtx with translated events
let mut ctx = ExpectCtx::new(self, server_events, client_events_map, auth_events, connect_events);
```

## Type Dispatch for read_event

To support generic `read_event<E>()`, implement trait:

```rust
pub trait HarnessEvent {
    type HarnessReturn;
    fn read_from_ctx(ctx: &mut ServerExpectCtx) -> Option<Self::HarnessReturn>;
}

impl HarnessEvent for AuthEvent<Auth> {
    type HarnessReturn = (ClientKey, Auth);
    fn read_from_ctx(ctx: &mut ServerExpectCtx) -> Option<Self::HarnessReturn> {
        ctx.auth_events.pop()
    }
}

impl HarnessEvent for ConnectEvent {
    type HarnessReturn = ClientKey;
    fn read_from_ctx(ctx: &mut ServerExpectCtx) -> Option<Self::HarnessReturn> {
        ctx.connect_events.pop()
    }
}

impl ServerExpectCtx {
    pub fn read_event<E: HarnessEvent>(&mut self) -> Option<E::HarnessReturn> {
        E::read_from_ctx(self)
    }
}
```

Alternative: Provide explicit methods and use type matching.

## Complete Test Execution Flow

1. Test calls `client_start("Client A", auth)` → ClientKey returned, auth stored in pending_auths
2. Test calls `expect()`:

   - Scenario ticks simulation
   - AuthEvent arrives with UserKey
   - Event processing matches auth → finds ClientKey → calls register_client_user → adds to auth_events
   - ExpectCtx created with auth_events vector

3. Test calls `ctx.server(|server| server.read_event::<AuthEvent<Auth>>())` → returns `Some((client_key, auth))`
4. Test calls `mutate()` → `server.accept_connection(&client_key)` → looks up UserKey → calls Naia API
5. Test calls `expect()` again:

   - ConnectEvent arrives with UserKey
   - Event processing looks up ClientKey → adds to connect_events

6. Test calls `read_event::<ServerConnectEvent>()` → returns `Some(client_key)`
7. Test calls `room_mut().add_user(&client_key)` → already works (RoomMut has ClientKey bridge)

## Files Modified

- `test/src/harness/scenario.rs` - pending_auths, event processing, registration, fix client_start, delete handshake
- `test/src/harness/expect_ctx.rs` - Add translated event vectors, update constructor
- `test/src/harness/server_expect_ctx.rs` - Add read_event returning Option types
- `test/src/harness/server_mutate_ctx.rs` - Add accept_connection accepting ClientKey

## Files Already Compatible

- `test/src/harness/room.rs` - RoomMut.add_user already accepts ClientKey
- `test/src/harness/client_state.rs` - Already has set_user_key method
- `test/tests/harness_scenarios.rs` - Updated to use Option returns

## Success Criteria

1. **Code compiles**: `cargo check --package naia-test` succeeds
2. **Tests pass**: Both harness_scenarios.rs tests pass
3. **UserKey hidden**: Only ClientKey visible in test-facing APIs
4. **No RefCell**: Normal mutable semantics throughout
5. **Clean architecture**: Mappings during tick with &mut self, not during read

## Verification Commands

```bash
cargo check --package naia-test
cargo test --package naia-test harness_single_client_spawn_replicates_to_server
cargo test --package naia-test harness_two_clients_entity_mapping
```

All must succeed.