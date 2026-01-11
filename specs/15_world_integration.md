# World Integration Contract

This spec defines the only valid semantics for integrating Naia’s replicated state into an external “game world” (engine ECS, custom world, adapter layer), on both server and client.

Normative keywords: **MUST**, **MUST NOT**, **SHOULD**, **MAY**.

---

## Scope

This spec covers:
- How Naia delivers world mutations (spawn/despawn, component insert/update/remove) to an external world implementation.
- Ordering and “exactly-once” expectations per tick/drain.
- Integration lifecycle: connect, disconnect, scope in/out, join-in-progress, reconnect.
- Misuse safety requirements at the integration boundary (no panics, defined no-ops/errors).

This spec does **not** define:
- The replication rules themselves (see `specs/entity_replication.md`).
- Scope policy semantics (see `specs/entity_scopes.md`).
- Ownership/delegation/authority rules (see `specs/entity_ownership.md`, `specs/entity_delegation.md`, `specs/entity_authority.md`).
- Messaging and request/response (see `specs/messaging.md`).
- Transport behavior (see `specs/transport.md`).

Related specs:
- `specs/entity_replication.md`
- `specs/entity_scopes.md`
- `specs/server_events_api.md`
- `specs/client_events_api.md`

---

## Terms

- **External World**: The user/engine-owned state container that mirrors Naia’s view (ECS, scene graph, entity-component store).
- **Integration Adapter**: Code that takes Naia events/mutations and applies them to the External World.
- **Naia World View**: The authoritative state Naia believes exists (server world; or client local world scoped per-client).
- **World Mutation**: One of: Spawn, Despawn, ComponentInsert, ComponentUpdate, ComponentRemove.
- **Tick**: The discrete step at which Naia advances and produces mutations/events.
- **Drain**: A single pass where the integration adapter consumes the available Naia events/mutations for a tick (or for a poll loop iteration).
- **In Scope**: An entity is present in the client’s Naia World View (see `specs/entity_scopes.md`).

---

## Contracts

### [world-integration-01] — World mirrors Naia view

For any participant `P` (server or client), if an External World is integrated, it MUST converge to exactly the Naia World View for `P` as mutations are drained and applied.

- Entities present in Naia view MUST exist in External World after applying all mutations through that tick.
- Entities absent in Naia view MUST NOT exist in External World after applying all mutations through that tick.
- For each entity, the set of components and their values MUST match Naia view after applying all mutations through that tick.

Test obligations:
- `world-integration-01.t1` (TODO → `test/tests/world_integration.rs::server_world_integration_stays_in_lockstep`)
  - Given a fake server External World wired to the integration adapter; when server spawns/inserts/updates/removes/despawns across ticks; then fake world matches Naia server view each tick.
- `world-integration-01.t2` (TODO → `test/tests/world_integration.rs::client_world_integration_stays_in_lockstep_with_scope`)
  - Given two clients with scope changes; when entities enter/leave scope and update; then each client External World matches that client’s Naia local view.

---

### [world-integration-02] — Mutation ordering is deterministic per tick

Within a single tick and for a single entity `E`, the integration adapter MUST apply mutations in a deterministic, valid order:

1) Spawn(E) (if E becomes present this tick)
2) ComponentInsert(E, X) (initial or newly added components)
3) ComponentUpdate(E, X) (updates to existing components)
4) ComponentRemove(E, X)
5) Despawn(E) (if E becomes absent this tick)

Constraints:
- ComponentInsert/Update/Remove MUST NOT be applied to an entity that is not present in External World at that moment.
- Despawn MUST occur after all other mutations for that entity in that tick.

This contract concerns integration application order; Naia’s event production rules are defined elsewhere.

Test obligations:
- `world-integration-02.t1` (TODO → `test/tests/world_integration.rs::per_tick_order_spawn_then_components_then_despawn`)
  - Given a tick where E spawns and receives inserts/updates; then the integration adapter can apply in the valid order without needing retries or panics.
- `world-integration-02.t2` (TODO → `test/tests/world_integration.rs::remove_before_despawn_in_same_tick_is_safe_and_deterministic`)
  - Given E has a component removed and E despawns in the same tick; then adapter applies remove then despawn deterministically.

---

### [world-integration-03] — Exactly-once delivery per drain

For a given participant `P`, each discrete world mutation produced by Naia MUST be consumable exactly once by the integration adapter.

- If the adapter drains mutations/events for a tick, and then drains again without advancing tick, the second drain MUST be empty for that mutation set.
- Duplicate deliveries MUST NOT occur in the integration API surface for the same mutation.

Notes:
- This is about the integration-facing drain semantics (the same principle as `server_events_api` / `client_events_api`), not about transport-level retransmits.

Test obligations:
- `world-integration-03.t1` (TODO → `test/tests/world_integration.rs::drain_is_one_shot_no_duplicates_server`)
- `world-integration-03.t2` (TODO → `test/tests/world_integration.rs::drain_is_one_shot_no_duplicates_client`)

---

### [world-integration-04] — Scope changes map to spawn/despawn in External World

On clients, scope governs presence. The integration adapter MUST reflect scope transitions as:

- When an entity `E` transitions OutOfScope → InScope for client `C`, the External World for `C` MUST receive a Spawn(E) (or equivalent “create entity”) and initial component inserts sufficient to form a coherent snapshot. (Snapshot semantics are defined in `specs/entity_scopes.md` and `specs/entity_replication.md`.)
- When `E` transitions InScope → OutOfScope for client `C`, the External World for `C` MUST receive a Despawn(E) (or equivalent “remove entity”).

Test obligations:
- `world-integration-04.t1` (TODO → `test/tests/world_integration.rs::scope_enter_creates_entity_and_components_as_snapshot`)
- `world-integration-04.t2` (TODO → `test/tests/world_integration.rs::scope_leave_removes_entity_no_ghosts`)

---

### [world-integration-05] — Join-in-progress and reconnect yield coherent External World

If a client joins late or reconnects, the External World MUST be reconstructed purely from current server state and current scope, not from stale client-local leftovers.

- On reconnect, the External World MUST NOT retain entities/components from the prior disconnected session.
- After initial snapshot application, the External World MUST match the client’s Naia World View.

Test obligations:
- `world-integration-05.t1` (TODO → `test/tests/world_integration.rs::late_join_builds_world_from_snapshot_only`)
- `world-integration-05.t2` (TODO → `test/tests/world_integration.rs::reconnect_clears_old_world_and_rebuilds_cleanly`)

---

### [world-integration-06] — Stable identity mapping at the integration boundary

The integration adapter MUST treat Naia’s entity identity as stable for the lifetime the entity is present in the Naia World View.

- If Naia indicates the “same entity” across ticks (same logical identity), the External World MUST keep the same external handle for that entity (or maintain an injective mapping).
- If an entity despawns and later a different entity appears, the adapter MUST NOT accidentally alias them as the same external entity.

This relies on identity semantics in `specs/entity_replication.md`; this contract ensures the adapter doesn’t break identity.

Test obligations:
- `world-integration-06.t1` (TODO → `test/tests/world_integration.rs::no_identity_aliasing_across_lifetimes`)
- `world-integration-06.t2` (TODO → `test/tests/world_integration.rs::same_logical_entity_keeps_same_external_mapping`)

---

### [world-integration-07] — Component type correctness

For every component mutation surfaced to the adapter, the component type MUST be correct and match the protocol/schema.

- The adapter MUST NOT be asked to apply a component mutation of a different type than declared.
- If a component cannot be decoded due to schema mismatch or decode failure, behavior MUST follow `specs/transport.md` / protocol contracts (e.g., reject connection or safely ignore that mutation), and the adapter MUST NOT panic.

Test obligations:
- `world-integration-07.t1` (TODO → `test/tests/world_integration.rs::component_types_are_correct_and_never_misrouted`)
- `world-integration-07.t2` (TODO → `test/tests/world_integration.rs::decode_failure_does_not_panic_external_world`)

---

### [world-integration-08] — Misuse safety: no panics, defined failures

The integration boundary MUST be robust to reasonable misuse:

- Applying a mutation for an entity not present MUST NOT panic; it MUST be a no-op or a defined error surfaced to the caller (implementation choice, but MUST be consistent).
- Applying a component update for a missing component MUST NOT panic; it MUST be a no-op or defined error.
- Re-applying the same mutation due to caller mistake MUST NOT corrupt state; it MUST be rejected/no-op deterministically.

This is about adapter-facing safety, not about hiding logic bugs inside Naia.

Test obligations:
- `world-integration-08.t1` (TODO → `test/tests/world_integration.rs::missing_entity_update_is_safe`)
- `world-integration-08.t2` (TODO → `test/tests/world_integration.rs::missing_component_update_is_safe`)
- `world-integration-08.t3` (TODO → `test/tests/world_integration.rs::double_apply_is_safe_and_deterministic`)

---

### [world-integration-09] — Zero-leak lifecycle cleanup

Across repeated connect/disconnect cycles and scope churn, the integration adapter MUST allow External World to reach a clean empty state when Naia’s view is empty.

- After disconnect, External World MUST contain no entities belonging to that connection/session.
- After all clients disconnect (or server clears its world), External World MUST be empty.

Test obligations:
- `world-integration-09.t1` (TODO → `test/tests/world_integration.rs::disconnect_cleans_world_fully`)
- `world-integration-09.t2` (TODO → `test/tests/world_integration.rs::long_running_cycles_do_not_leak_external_entities`)

---

## Notes for Implementers

- For server integration, the External World is typically updated from server-side inserts/updates/removes/despawns (see `specs/server_events_api.md`).
- For client integration, the External World is typically updated from client-side world events (see `specs/client_events_api.md`), and scope governs presence (`specs/entity_scopes.md`).
- This spec is satisfied whether the adapter is “push” (callbacks) or “pull” (drain + apply), as long as contracts above hold.

## Test obligations

TODO: Define test obligations for this specification.
