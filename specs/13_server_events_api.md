# Server Events API

This spec defines the **only** valid semantics for the server-side Events API surface: what is collected, when it becomes observable, how it is drained, and what ordering/duplication guarantees exist.

Normative keywords: **MUST**, **MUST NOT**, **MAY**, **SHOULD**.

Related specs:
- `specs/entity_replication.md` (spawn/update/remove/despawn semantics)
- `specs/entity_scopes.md` (in-scope vs out-of-scope and snapshot behavior)
- `specs/messaging.md` (message ordering, reliability, request/response semantics)
- `specs/time_ticks_commands.md` (tick definition, wrap ordering, command timing model)
- `specs/connection_lifecycle.md` (connect/disconnect/auth ordering + cleanup)

---

## Glossary

- **Events API**: The server-facing interface that buffers and exposes observable happenings (auth/connect/disconnect, world mutations, messages, requests).
- **World events**: Events that describe replicated-world changes and inbound app-level messages (spawn/despawn, insert/update/remove, message/request/response).
- **Tick events**: Events that describe connection/tick/session-level happenings (auth/connect/disconnect, tick-related meta events if any).
- **Receive step**: The act of ingesting available packets from the transport into Naia’s internal packet buffer.
- **Process step**: The act of processing all buffered packets, applying protocol semantics, and producing new pending events.
- **Drain**: Reading events from the API such that they are removed from the pending queue (pure read+remove).
- **In scope**: A user is considered a recipient for an entity only if `InScope(user, entity)` per `entity_scopes`.
- **Tick**: Server simulation tick as defined in `5_time_ticks_commands.md`. (Wrap-safe ordering applies.)

---

## API boundary model (normative)

This spec standardizes the server loop boundary as:

1) `receive_all_packets()`  (Receive step)
2) `process_all_packets()`  (Process step)
3) `take_tick_events()` and/or `take_world_events()` (Drain steps)

The *names* above reflect the current API. The **semantics** below are the contract.

### server-events-00 — Receive step is ingestion only
- The Receive step MUST only ingest packets into an internal buffer.
- The Receive step MUST NOT advance tick, mutate the world, or produce observable events directly.

### server-events-01 — Process step is the only event-production boundary
- New events MUST become pending/observable only as a result of the Process step.
- If no Process step occurs, drains MUST NOT “discover” new events.

### server-events-02 — Drains are pure read+remove
- `take_world_events()` and `take_tick_events()` MUST be pure drains:
  - MUST NOT receive packets
  - MUST NOT process packets
  - MUST NOT advance tick
  - MUST have no side effects other than removing the drained events from the pending queue

---

## Contracts

### server-events-03 — Drain operations are destructive and idempotent (no replay without new Process step)
**Rule**
- Each drain call MUST remove the returned events from the pending buffer.
- Repeating the same drain call again **without any intervening Process step that produced new pending events** MUST return empty.
- This MUST hold even if drains are called multiple times within the same server tick.

**Notes**
- “Idempotent” here means “subsequent drains see nothing,” not “same payload returned.”

**Test obligations**
- `server-events-03.t1` (TODO) Given one insert+update+remove becomes pending, When draining twice without another Process step, Then first drain returns expected events and second drain returns none.
- `server-events-03.t2` (TODO) Given no new pending events, When calling all drains, Then all are empty.

---

### server-events-04 — Event types are partitioned; no cross-contamination
**Rule**
- World mutation events MUST NOT appear in message/request streams.
- Message/request streams MUST NOT appear in world mutation streams.
- Tick/session events (auth/connect/disconnect) MUST NOT appear in world mutation streams.

**Test obligations**
- `server-events-04.t1` (TODO) Given mixed activity (spawn + message + request + connect), When draining each category, Then each appears only in the correct stream.

---

### server-events-05 — Auth/connect/disconnect ordering is stable and exactly-once per session transition
**Rule**
- For each connection attempt when auth is enabled:
  - exactly one auth decision event MUST be exposed
  - if accepted, exactly one connect event MUST be exposed after auth for that session
  - if rejected, a connect event MUST NOT occur for that attempt
- For each session termination:
  - exactly one disconnect event MUST be exposed
  - duplicate lower-level disconnect signals MUST NOT duplicate the disconnect event

**Test obligations**
- `server-events-05.t1` (TODO) `require_auth=true`, valid credentials → auth event occurs before connect.
- `server-events-05.t2` (TODO) invalid credentials → auth event occurs, connect does not.
- `server-events-05.t3` (TODO) duplicate disconnect signals → exactly one disconnect event.

---

### server-events-06 — Disconnect cleanup is consistent with scope + ownership contracts
**Rule**
- After a disconnect is observed, the server MUST have cleaned up all per-connection scoped state attributable solely to that session (no “ghost” scoped entities for that user).
- Additionally, ownership cleanup MUST follow `9_entity_ownership.md` (client-owned entities despawn when owner disconnects).

**Test obligations**
- `server-events-06.t1` (TODO) Disconnect while scoped → scope membership removed.
- `server-events-06.t2` (TODO) Disconnect owner → owned entities are despawned (ownership contract).

---

### server-events-07 — Entity spawn/enter events: per user, in-scope only, exactly-once
**Rule**
- When an entity `E` enters scope for user `U` (including initial join snapshot), the World events stream MUST expose exactly one spawn/enter event for `(U, E)`.
- Spawn/enter events MUST be emitted only for users for which `InScope(U, E)` becomes true.
- Spawn/enter events MUST NOT be emitted for out-of-scope users.

**Test obligations**
- `server-events-07.t1` (TODO) E becomes in-scope for A but not B → only A gets spawn/enter.
- `server-events-07.t2` (TODO) Late join snapshot → spawn/enter for all in-scope entities exactly once.

---

### server-events-08 — Component insert/update/remove: per user and per component, no duplicates
**Rule**
- For each user `U` with `InScope(U, E)` at the time the change becomes observable:
  - inserting component `C` on `E` MUST produce exactly one insert event for `(U, E, C)`
  - updating MUST produce exactly one update event per underlying applied update
  - removing MUST produce exactly one remove event per underlying removal
- Duplicate packets/retries MUST NOT create duplicate events unless they cause a new applied transition.

**Test obligations**
- `server-events-08.t1` (TODO) One update replicated to two users → two update events, no duplicates.
- `server-events-08.t2` (TODO) Insert then update then remove in same tick → each appears exactly once.

---

### server-events-09 — Despawn/leave-scope events are exactly-once and end that user’s lifecycle
**Rule**
- When `E` leaves scope for `U` (scope change or true despawn), the World events stream MUST expose exactly one despawn/exit event for `(U, E)`.
- After `(U, E)` has exited, the server MUST NOT surface further insert/update/remove events for `(U, E, *)` unless `E` re-enters scope for `U` as a new lifecycle (per `7_entity_scopes.md` + `8_entity_replication.md`).

**Test obligations**
- `server-events-09.t1` (TODO) Despawn while in scope → exit once; no further component events for that lifecycle.
- `server-events-09.t2` (TODO) Leave scope then re-enter after ≥1 tick → fresh spawn/enter event.

---

### server-events-10 — No “component events before spawn/enter” for any user
**Rule**
- For any user `U`, the World events stream MUST NOT surface insert/update/remove events for entity `E` before `U` has observed spawn/enter for `E`.
- Under reordering/duplication, internal buffering is allowed, but the API-visible ordering MUST respect this invariant.

**Test obligations**
- `server-events-10.t1` (TODO) Under simulated reorder, assert no insert/update/remove for `(U, E)` is observed before spawn/enter for `(U, E)`.

---

### server-events-11 — Message events: grouped by channel and message type; each yields sender + payload; drain once
**Rule**
- Inbound messages MUST be exposed via typed message events grouped by:
  - **channel type** and
  - **message type**
- Iteration MUST yield the sender user key and the decoded message payload.

(Example shape: `world_events.read::<MessageEvent<Channel, Msg>>() -> (user_key, msg)`.)

Additional requirements:
- Each inbound delivered message MUST appear exactly once to the application across drains.
- Messages MUST be decoded to the correct message type per protocol configuration and MUST NOT be misrouted to the wrong channel/type.

**Test obligations**
- `server-events-11.t1` (TODO) Multiple senders + channels → correct channel/type grouping; each yields correct sender; each appears once.
- `server-events-11.t2` (TODO) Mixed message types → decoded to correct type and not misrouted.

---

### server-events-12 — Request/response events: exactly-once surfacing, correct matching, drain once
**Rule**
- For each incoming request accepted by the protocol layer, the server MUST surface exactly one corresponding request event/handle to the application.
- Any response matching MUST be correct per `4_messaging.md` and MUST NOT surface duplicates under retransmit/duplication.
- Draining request/response events MUST be destructive and MUST NOT replay already-drained items.

**Test obligations**
- `server-events-12.t1` (TODO) One request → exactly one server-visible request event.
- `server-events-12.t2` (TODO) Duplicate packets → still exactly one request event.

---

### server-events-13 — API misuse safety: drains MUST NOT panic
**Rule**
- Calling any drain method at any time (including when empty) MUST NOT panic.
- Empty drains MUST return empty.

**Test obligations**
- `server-events-13.t1` (TODO) Call drains repeatedly in an empty world; assert empties and no panic.

---

## Forbidden behaviors

- Producing new observable events during drains (drains must be pure).
- Replaying already-drained events without an intervening Process step producing new pending events.
- Emitting component events for `(U, E)` before spawn/enter for `(U, E)`.
- Emitting entity/component events for out-of-scope users.
- Duplicating auth/connect/disconnect events for a single session transition.
- Misrouting messages to the wrong channel/type or losing sender attribution.
- Panicking on empty drains or repeated drains.
