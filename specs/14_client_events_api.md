# Client Events API Contract

This document defines the **only** valid semantics for the client-side Events API: what events exist, when they become observable, how they are drained, ordering guarantees, and behavior under reordering/duplication/scope changes/disconnects.

Normative keywords: **MUST**, **MUST NOT**, **MAY**, **SHOULD**.

---

## Glossary

- **Client Events API**: The public interface by which a client drains replicated-world events (spawns, despawns, component changes, messages, request/response).
- **World events**: Events describing the client’s replicated world changes and inbound app-level messages.
- **Tick events**: Events describing connection/tick/session-level happenings (if any are exposed to the client).
- **Receive step**: Ingesting packets from the transport into Naia’s internal packet buffer.
- **Process step**: Processing all buffered packets, applying protocol semantics, and producing new pending events / applying replicated state changes.
- **Drain**: Reading events such that they are removed from the pending queue (pure read+remove).
- **Tick**: Client tick as defined in `time_ticks_commands.md`. (Wrap-safe ordering applies.)
- **InScope(C,E)** / **OutOfScope(C,E)**: Whether entity `E` exists in client `C`’s local world (see `entity_scopes.md`).
- **Entity lifetime**: scope enter → scope leave, with the ≥1 tick out-of-scope rule (see entity suite).

---

## Cross-References

- Tick + time model: `specs/time_ticks_commands.md`
- Identity, replication legality, and “no updates before spawn / none after despawn”: `specs/entity_replication.md`
- Scope transitions, join snapshots, and scope leave/re-enter semantics: `specs/entity_scopes.md`
- Messaging ordering/reliability: `specs/messaging.md`
- Ownership/delegation/authority semantics (not defined here): `specs/entity_ownership.md`, `specs/entity_delegation.md`, `specs/entity_authority.md`

---

## API boundary model (normative)

This spec standardizes the client loop boundary as:

1) `receive_all_packets()`  (Receive step)
2) `process_all_packets()`  (Process step)
3) `take_tick_events()` and/or `take_world_events()` (Drain steps)

The *names* above reflect the current API. The **semantics** below are the contract.

### client-events-00 — Receive step is ingestion only
- The Receive step MUST only ingest packets into an internal buffer.
- The Receive step MUST NOT directly mutate the client world or produce observable events.

### client-events-01 — Process step is the only event-production / world-application boundary
- Replicated state application and new pending events MUST occur only as a result of the Process step.
- Drains MUST NOT “discover” new events unless a prior Process step produced them.

### client-events-02 — Drains are pure read+remove
- `take_world_events()` and `take_tick_events()` MUST be pure drains:
  - MUST NOT receive packets
  - MUST NOT process packets
  - MUST NOT advance tick
  - MUST have no side effects besides removing drained events from the pending queue

---

## Contracts

### client-events-03 — Drain is destructive and idempotent (no replay without new Process step)
**Rule:** Draining a given event stream MUST remove those events from the pending queue, and subsequent drains without an intervening Process step producing new pending events MUST return empty.

- Draining twice “back-to-back” MUST NOT return the same event twice.
- Draining does not advance time/tick and does not trigger receive/process.

**Test obligations:**
- `TODO: client_events_api::drain_is_destructive_and_idempotent_no_replay`

---

### client-events-04 — Spawn is the first event for an entity lifetime on that client
**Rule:** For any entity `E` that becomes present on client `C`, the first observable entity-lifetime event for that lifetime MUST be `Spawn(E)` (or an equivalent spawn event). The client MUST NOT observe component Update/Remove events for `E` before Spawn for that lifetime.

- Initial component presence delivered with the spawn snapshot MAY be represented as:
  - (a) Spawn + a batch of Insert events, or
  - (b) Spawn carrying a snapshot, with zero inserts,
    as long as the model is consistent and tests assert the chosen model.
- Under packet reordering/duplication, the API MUST still prevent “update-before-spawn” observability.

**Test obligations:**
- `TODO: client_events_api::no_update_or_remove_before_spawn_under_reordering`

---

### client-events-05 — No events for entities that were never in scope
**Rule:** If `E` is never `InScope(C,E)` for client `C` during a connection lifetime, the client Events API MUST not emit any entity events for `E` (no spawn/insert/update/remove/despawn).

This includes entities created and destroyed entirely while `C` is out of scope.

**Test obligations:**
- `TODO: client_events_api::no_events_for_never_in_scope_entities`

---

### client-events-06 — Despawn ends the entity lifetime; no further events for that lifetime
**Rule:** After `Despawn(E)` is emitted for client `C`, the Events API MUST NOT emit any further entity-related events for that lifetime of `E` on `C`.

- Late packets referencing the despawned lifetime MUST be ignored safely (see `entity_replication.md`).
- If `E` later re-enters scope as a new lifetime under the scope model, that is a new Spawn and a new lifetime.

**Test obligations:**
- `TODO: client_events_api::no_events_after_despawn_under_reordering`

---

### client-events-07 — Component insert/update/remove are one-shot per applied change
**Rule:** When a component change is applied to an entity `E` on client `C`, the Events API MUST surface exactly one corresponding event for that applied change.

- Insert: exactly once when a component becomes present on `E`
  - If a replicated-backed component replaces a local-only component instance of the same type, the Events API MUST emit an Insert event (not Update) for that transition.
- Update: exactly once per distinct applied update
- Remove: exactly once when a component is removed from `E`

Duplicate packets or retries MUST NOT cause duplicate events if they do not cause a new applied state transition.

**Test obligations:**
- `TODO: client_events_api::component_insert_update_remove_are_one_shot`

---

### client-events-08 — Per-entity ordering: spawn → (inserts/updates/removes)* → despawn
**Rule:** For a given entity lifetime on client `C`, the API-visible ordering MUST respect:

`Spawn(E)` happens before any component events for that lifetime, and `Despawn(E)` happens after all component events for that lifetime.

This is an observability constraint: internal buffering/reordering is allowed, but the Events API must never violate this ordering.

**Test obligations:**
- `TODO: client_events_api::per_entity_ordering_is_never_violated`

---

### client-events-09 — Scope transitions are reflected as spawn/despawn (with the defined model)
**Rule:** When an entity `E` transitions between OutOfScope and InScope on client `C`, the client Events API MUST reflect that transition using spawn/despawn semantics consistent with `entity_scopes.md`.

- Leaving scope MUST cause Despawn(E) (entity removed from client world).
- Re-entering scope MUST cause Spawn(E) with a coherent snapshot, consistent with the identity/lifetime model.

**Test obligations:**
- `TODO: client_events_api::scope_leave_reenter_emits_spawn_despawn_consistently`

---

### client-events-10 — Message events are typed, correctly routed, and drain once
**Rule:** Client message events:
- MUST be exposed via typed message events grouped by:
  - channel type, and
  - message type
- Iteration MUST yield the sender identity (server or user depending on channel direction semantics) and the decoded payload.

(Example shape: `world_events.read::<MessageEvent<Channel, Msg>>() -> (sender, msg)`.)

Additional requirements:
- MUST be drained exactly once (no duplicates on repeated drains).
- MUST NOT be emitted for messages not actually delivered (e.g., dropped unreliable traffic).
- Ordering/reliability constraints are defined in `messaging.md`; this contract covers API surfacing correctness + drain semantics.

**Test obligations:**
- `TODO: client_events_api::message_events_are_typed_routed_and_one_shot`

---

### client-events-11 — Request/response events are matched, one-shot, and cleaned up on disconnect
**Rule:** If the client exposes request/response events via its Events API:
- Each delivered request/response MUST be surfaced exactly once and drain cleanly.
- Responses MUST be matchable to the originating request handle/ID per the public API.
- On disconnect with in-flight requests, the client MUST follow the defined failure behavior and MUST NOT leak request tracking state (see `messaging.md`).

**Test obligations:**
- `TODO: client_events_api::request_response_events_are_one_shot_and_matched`
- `TODO: client_events_api::in_flight_requests_fail_cleanly_on_disconnect`

---

### client-events-12 — Authority events are out of scope for this spec
**Rule:** Authority-related events MUST follow `entity_authority.md`. This spec does not define them, except:

- If authority events are surfaced through the same drain mechanism, they MUST obey drain semantics (no duplicates) as per this spec.

**Test obligations:**
- `TODO: client_events_api::authority_events_obey_drain_semantics_without_duplicates`

---

## Forbidden behaviors

- Producing new observable events during drains (drains must be pure).
- Replaying already-drained events without an intervening Process step producing new pending events.
- Emitting Update or Remove before Spawn for an entity lifetime.
- Emitting entity events for an entity never in scope.
- Emitting entity events after Despawn for that lifetime.
- Misrouting message events to the wrong channel/type.
- Panicking on empty drains or repeated drains.
