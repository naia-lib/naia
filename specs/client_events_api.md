# Client Events API Contract

This document defines the **only** valid semantics for the client-side Events API: what events exist, when they are emitted, how they are drained, ordering guarantees, and behavior under reordering/duplication/scope changes/disconnects.  
Normative keywords: **MUST**, **MUST NOT**, **MAY**, **SHOULD**.

---

## Glossary

- **Client Events API**: The public interface by which a client drains replicated-world events (spawns, despawns, component changes, messages, request/response).
- **Drain**: Reading events from the API such that they are removed from the pending queue.
- **Tick**: A single client update step; events are accumulated by the runtime and made available for draining.
- **Entity `E`**: A replicated object with stable logical identity (see `entity_replication.md`).
- **InScope(C,E)** / **OutOfScope(C,E)**: Whether entity `E` exists in client `C`’s local world (see `entity_scopes.md`).
- **Spawn**: The moment `E` becomes present in the client world (entering scope or initial join snapshot).
- **Despawn**: The moment `E` is removed from the client world (leaving scope or server despawn).
- **Insert/Update/Remove**: Component lifecycle changes applied to entities in the client world.

---

## Cross-References

- Identity, replication ordering, and “no updates before spawn / none after despawn”: `specs/entity_replication.md`
- Scope transitions, join snapshots, and scope leave/re-enter semantics: `specs/entity_scopes.md`
- Ownership/delegation/authority event semantics (explicitly not part of this spec): `specs/entity_authority.md`, `specs/entity_delegation.md`, `specs/entity_ownership.md`

---

## Contracts

### client-events-01 — Drain is destructive and idempotent

**Rule:** Draining a given event stream **MUST** remove those events from the pending queue, and subsequent drains without new underlying changes **MUST** return empty.

- Draining twice in the same tick **MUST NOT** return the same event twice.
- Draining across ticks **MUST NOT** “replay” past events.

**Test obligations:**
- `TODO: client_events_api::drain_is_destructive_and_idempotent`

---

### client-events-02 — Spawn is the first event for an entity lifetime on that client

**Rule:** For any entity `E` that becomes present on client `C`, the first observable event for that lifetime on `C` **MUST** be `Spawn(E)` (or an equivalent “spawn” event), and the client **MUST NOT** observe component `Update/Remove` events for `E` before `Spawn(E)`.

- Initial component presence delivered with the spawn snapshot **MAY** be represented as:
  - (a) Spawn + a batch of Insert events, or
  - (b) Spawn carrying a snapshot, with zero inserts,
  as long as the model is consistent and the tests assert the chosen model.
- Under packet reordering/duplication, the API **MUST** still prevent “update-before-spawn” observability.

**Test obligations:**
- Implemented elsewhere for replication legality, but **must** be asserted via client API drains:
  - `TODO: client_events_api::no_update_or_remove_before_spawn_under_reordering`

---

### client-events-03 — No events for entities that were never in scope

**Rule:** If `E` is never `InScope(C,E)` for client `C` during a connection lifetime, then `C`’s Events API **MUST** not emit any entity events for `E` (no spawn/insert/update/remove/despawn).

This includes entities created and destroyed entirely while `C` is out of scope.

**Test obligations:**
- `TODO: client_events_api::no_events_for_never_in_scope_entities`

---

### client-events-04 — Despawn ends the entity lifetime; no further events for that lifetime

**Rule:** After `Despawn(E)` is emitted for client `C`, the Events API **MUST NOT** emit any further entity-related events for that lifetime of `E` on `C` (no updates/inserts/removes/messages tied to that entity handle, etc.).

- Late packets referencing the despawned lifetime **MUST** be ignored safely (see `entity_replication.md`).
- If `E` later re-enters scope as a *new lifetime* under the chosen scope model, that is a new `Spawn(E)` and a new lifetime.

**Test obligations:**
- `TODO: client_events_api::no_events_after_despawn_under_reordering`

---

### client-events-05 — Component insert/update/remove are one-shot per applied change

**Rule:** When a component change is applied to an entity `E` on client `C`, the Events API **MUST** surface exactly one corresponding event for that applied change:

- Insert: exactly once when a component becomes present on `E`
- Update: exactly once per distinct applied update
- Remove: exactly once when a component is removed from `E`

Duplicate packets or retries **MUST NOT** cause duplicate events if they do not cause a new applied state transition.

**Test obligations:**
- `TODO: client_events_api::component_insert_update_remove_are_one_shot`

---

### client-events-06 — Per-entity ordering: spawn → (inserts/updates/removes)* → despawn

**Rule:** For a given entity lifetime on client `C`, the API-visible ordering **MUST** respect:

`Spawn(E)` happens before any component events for that lifetime, and `Despawn(E)` happens after all component events for that lifetime.

This is an *observability* constraint: internal buffering/reordering is allowed, but the Events API must never violate this ordering.

**Test obligations:**
- `TODO: client_events_api::per_entity_ordering_is_never_violated`

---

### client-events-07 — Scope transitions are reflected as spawn/despawn (with a defined model)

**Rule:** When an entity `E` transitions between `OutOfScope(C,E)` and `InScope(C,E)`, the client Events API **MUST** reflect that transition using spawn/despawn semantics consistent with the scope model defined in `entity_scopes.md`.

- Leaving scope **MUST** cause `Despawn(E)` (entity removed from client world).
- Re-entering scope **MUST** cause `Spawn(E)` with a coherent snapshot, consistent with the chosen identity/lifetime model.

**Test obligations:**
- Covered by scope specs, but must be asserted via client events drain:
  - `TODO: client_events_api::scope_leave_reenter_emits_spawn_despawn_consistently`

---

### client-events-08 — Message events are typed, correctly routed, and drain once

**Rule:** Client message events:
- **MUST** be exposed under the correct channel/type grouping promised by the public API.
- **MUST** be drained exactly once (no duplicates on repeated drains).
- **MUST NOT** be emitted for messages that were not actually delivered (e.g., dropped unreliable traffic).

Ordering constraints are defined in `messaging.md`; this contract covers *API surfacing correctness and drain semantics*.

**Test obligations:**
- `TODO: client_events_api::message_events_are_typed_routed_and_one_shot`

---

### client-events-09 — Request/response events are matched, one-shot, and cleaned up on disconnect

**Rule:** If the client exposes request/response events via its Events API:
- Each delivered request/response **MUST** be surfaced exactly once and drain cleanly.
- Responses **MUST** be matchable to the originating request handle/ID per the public API.
- If the connection drops with in-flight requests, the client **MUST** surface the defined failure/timeout outcome and **MUST NOT** leak request tracking state (see `messaging.md`).

**Test obligations:**
- `TODO: client_events_api::request_response_events_are_one_shot_and_matched`
- `TODO: client_events_api::in_flight_requests_fail_cleanly_on_disconnect`

---

### client-events-10 — Authority events are out of scope for this spec

**Rule:** Authority-related events (`AuthGranted/AuthDenied/AuthLost` or equivalent) **MUST** follow `entity_authority.md`. This spec does not define them, except to state:

- Client Events API **MUST NOT** emit authority events for non-delegated entities (already a rule in authority/delegation/ownership contracts).
- If authority events are surfaced through the same drain mechanism, they **MUST** still obey `client-events-01` drain semantics (no duplicates).

**Test obligations:**
- Covered by authority specs; ensure any client API drain path obeys `client-events-01`.
  - `TODO: client_events_api::authority_events_obey_drain_semantics_without_duplicates`

---

## Forbidden Behaviors

- Emitting an entity `Update` or `Remove` event for `E` before emitting `Spawn(E)` for that lifetime on that client.
- Emitting any entity events for an entity that was never in scope for that client.
- Emitting any entity events for a lifetime of `E` after emitting `Despawn(E)` for that lifetime.
- Re-emitting the same event on repeated drains without an underlying applied change.
- Emitting duplicate component events due solely to duplicated packets that do not create a new applied transition.

---
