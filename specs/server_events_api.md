# Server Events API

This spec defines the **only** valid semantics for the server-side Events API surface: what is collected, how it is grouped, how it is drained, and what ordering/duplication guarantees exist.

Normative keywords: **MUST**, **MUST NOT**, **MAY**, **SHOULD**.

Related specs:
- `specs/entity_replication.md` (spawn/update/remove/despawn semantics)
- `specs/entity_scopes.md` (in-scope vs out-of-scope and snapshot behavior)
- `specs/messaging.md` (message ordering, reliability, request/response semantics)
- `specs/connection_lifecycle.md` (connect/disconnect events and cleanup)

---

## Glossary

- **Tick**: One server simulation step / update cycle.
- **Events API**: The server-facing interface that buffers and exposes observable happenings (connects, disconnects, entity/component changes, messages, requests).
- **Drain**: A “take_*” style read that removes buffered events from the internal queue.
- **Event batch**: The set of events returned by a single drain call.
- **In scope**: A user is considered a recipient for an entity only if `InScope(user, entity)` per `entity_scopes`.

---

## Contracts

### server-events-01 — Drain operations are destructive and idempotent per tick
**Rule**
- Each drain call (e.g., `take_*`) MUST remove the returned events from the Events buffer.
- Repeating the same drain call again **without new underlying changes** MUST return an empty result.
- This MUST hold regardless of whether the drain is called multiple times within the same tick.

**Notes**
- “Idempotent” here means “subsequent drains see nothing,” not “same payload returned.”

**Test obligations**
- `server-events-01.t1` (TODO) Given one component insert+update+remove in one tick, When calling the corresponding drains twice, Then first call returns expected events and second call returns none.
- `server-events-01.t2` (TODO) Given no new world changes, When calling all drains, Then all are empty.

---

### server-events-02 — Event types are partitioned; no cross-contamination
**Rule**
- Entity/component lifecycle events MUST NOT appear in message/request drains.
- Message events MUST NOT appear in entity/component drains.
- Connect/disconnect/auth events MUST NOT appear in any world-change drains.

**Test obligations**
- `server-events-02.t1` (TODO) Given mixed activity in one tick (spawn + message + request), When draining each category, Then each event appears only in its correct drain.

---

### server-events-03 — Connect events: exactly-once and stable ordering
**Rule**
- For each successful connection establishment, the Events API MUST expose exactly one connect event.
- Connect events MUST be ordered by their occurrence in server processing (the server’s canonical order).
- Duplicate connect events MUST NOT be emitted for the same connection/session.

**Test obligations**
- `server-events-03.t1` (TODO) Given A connects then B connects, When draining connect events, Then order is [A, B] exactly once each.
- `server-events-03.t2` (TODO) Given a reconnect model that produces a new session, When draining connect events, Then it produces exactly one connect for the new session.

---

### server-events-04 — Disconnect events: exactly-once, cleanup-consistent, and idempotent
**Rule**
- For each connection/session termination, the Events API MUST expose exactly one disconnect event.
- Disconnect handling MUST be idempotent: duplicate lower-level disconnect signals MUST NOT produce multiple disconnect events.
- After a disconnect is observed, the server MUST have cleaned up all per-connection scoped state for that connection (no “ghost” scoped entities attributable solely to the disconnected session).

**Test obligations**
- `server-events-04.t1` (TODO) Given A disconnects and a duplicate disconnect signal arrives, When draining disconnect events, Then exactly one disconnect event exists.
- `server-events-04.t2` (TODO) Given A disconnects while scoped to entities, Then scoped membership for A is fully removed.

---

### server-events-05 — Auth events (when enabled) precede connect events
**Rule**
- If authentication is enabled (per `connection_lifecycle`), and a client attempts to connect:
  - An auth decision event MUST be emitted for that attempt.
  - If accepted, the connect event MUST occur **after** the auth event for the same session.
  - If rejected, a connect event MUST NOT occur for that attempt.

**Test obligations**
- `server-events-05.t1` (TODO) Given `require_auth=true` and valid credentials, When draining events, Then auth event appears before connect event.
- `server-events-05.t2` (TODO) Given invalid credentials, Then auth event exists and connect event does not.

---

### server-events-06 — Entity spawn events: per user, in-scope only, exactly-once
**Rule**
- When an entity `E` enters scope for user `U` (including initial join snapshot), the Events API MUST expose exactly one “spawn/enter” event for `(U, E)` (or its equivalent canonical representation).
- Spawn events MUST be emitted only for users for which `InScope(U, E)` becomes true.
- Spawn events MUST NOT be emitted for users that are out of scope.

**Test obligations**
- `server-events-06.t1` (TODO) Given E becomes in-scope for A but not B, Then only A’s spawn event is present.
- `server-events-06.t2` (TODO) Given B joins late and receives snapshot, Then B gets spawn events for all in-scope entities exactly once.

---

### server-events-07 — Component insert/update/remove events: per user and per component, no duplicates
**Rule**
- If the server inserts component `C` on entity `E`, then for each user `U` such that `InScope(U, E)` holds at the time the change becomes observable, the Events API MUST include exactly one “insert” event for `(U, E, C)`.
- Similarly, updates MUST emit exactly one “update” event per `(U, E, C)` change occurrence, and removes MUST emit exactly one “remove” event per `(U, E, C)` change occurrence.
- The Events API MUST NOT emit duplicate insert/update/remove events for the same underlying change.

**Notes**
- This contract is about Events API reporting. Replication semantics live in `entity_replication`.

**Test obligations**
- `server-events-07.t1` (TODO) Given server updates one component once while replicated to two users, Then two update events exist (one per user) and no duplicates.
- `server-events-07.t2` (TODO) Given insert then update then remove in the same tick, Then each appears exactly once in the appropriate drains.

---

### server-events-08 — Despawn/leave-scope events are emitted exactly once and end the lifecycle for that user
**Rule**
- When `E` leaves scope for user `U` (either due to scope policy change or true despawn), the Events API MUST expose exactly one “despawn/exit” event for `(U, E)` for that transition.
- After `(U, E)` has an exit event, the Events API MUST NOT emit further insert/update/remove events for `(U, E, *)` unless `E` re-enters scope for `U` as a new lifecycle per `entity_scopes` and `entity_replication`.

**Test obligations**
- `server-events-08.t1` (TODO) Given E despawns while in scope, Then despawn event appears once and no subsequent component events for that user occur.
- `server-events-08.t2` (TODO) Given E leaves scope (not despawned globally), Then exit event occurs and later re-entry yields a fresh spawn event (per model in scopes spec).

---

### server-events-09 — No “updates before spawn” for any user
**Rule**
- For any user `U`, the Events API MUST NOT surface component insert/update/remove events for entity `E` before `U` has observed a spawn/enter event for `E`.
- If underlying processing receives late/out-of-order data, it MUST be ignored or buffered such that the Events API invariant holds.

**Test obligations**
- `server-events-09.t1` (TODO) Under simulated reordering, assert that `take_updates` cannot return an event for `(U, E)` before `take_spawns` (or equivalent) has produced `(U, E)`.

---

### server-events-10 — Message events: grouped by channel and sender, drained once
**Rule**
- The Events API MUST expose inbound messages received by the server, grouped in a stable, documented structure (at minimum by channel and by sender).
- Each inbound message MUST appear exactly once across drains.
- Messages MUST be typed correctly (decoded to the correct message type per protocol configuration) and MUST NOT be misrouted to the wrong channel/type bucket.

**Test obligations**
- `server-events-10.t1` (TODO) Given multiple senders and channels in one tick, Then drains group by channel and sender correctly and each message appears once.
- `server-events-10.t2` (TODO) Given mixed message types, Then each is decoded to the right type and appears in the right bucket.

---

### server-events-11 — Request/response events: exactly-once delivery and matching handles
**Rule**
- For each incoming request to the server that is accepted by the protocol layer, the Events API MUST surface exactly one request event (or request handle) to the application.
- For each response sent by the application, the system MUST ensure that on the receiving side the matching is correct and duplicates are not surfaced, per `messaging`.
- Draining requests/responses MUST be destructive (covered by server-events-01) and MUST NOT re-emit already-drained request/response events.

**Test obligations**
- `server-events-11.t1` (TODO) Given a client sends a request and server responds, Then server sees exactly one request event and no duplicates.
- `server-events-11.t2` (TODO) Given duplicate packets, Then server still surfaces only one request event.

---

### server-events-12 — API misuse safety: drains MUST NOT panic
**Rule**
- Calling any Events API drain method at any time (including when empty) MUST NOT panic.
- Empty drains MUST return empty collections/results.

**Test obligations**
- `server-events-12.t1` (TODO) Call all drains repeatedly in an empty world; assert no panic and empties returned.

---

## Cross-spec constraints

### server-events-13 — Scope-aware per-user event attribution
**Rule**
- Any per-user entity/component event attribution MUST be consistent with `entity_scopes`.
- If a user is not in scope for an entity at the time an event would otherwise be attributed, that event MUST NOT be attributed to that user.

**Test obligations**
- `server-events-13.t1` (TODO) Move entity in/out of a user’s scope while mutating; assert only in-scope intervals produce events.

---

## Forbidden behaviors

- Emitting duplicate connect/disconnect/auth events for a single session transition.
- Emitting component events for a user/entity before that user has observed a spawn/enter event for that entity.
- Emitting component events for out-of-scope users.
- Re-emitting already-drained events.
- Panicking on empty drains or repeated drains.
