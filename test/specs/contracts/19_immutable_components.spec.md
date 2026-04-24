# Contract 19 — Immutable Replicated Components (Phase 5)

## Context

Components marked `#[replicate(immutable)]` (trait method `is_immutable() → true`) are written
once at spawn time and never mutate. There is no need to register them in the
`GlobalDiffHandler` or `UserDiffHandler`, allocate `MutChannel` pairs for them, or include
them in the dirty-candidate set.

Phase 5 eliminates diff-tracking for immutable components:
- `insert_component_diff_handler` returns early when `component.is_immutable()` is true.
- `init_entity_send_host_commands` skips `entity_update_manager.register_component` for
  immutable component kinds.

Immutable components are still written to the wire via the normal `write()` path on spawn
(either via `SpawnWithComponents` or a trailing `InsertComponent`). Clients receive them
exactly as they would a mutable component.

---

## Obligations

### t1 — Immutable component replicates to client

An entity with an immutable component (`ImmutableLabel`) enters scope for a client.
The client must receive and hold that component exactly as with any normal component.

### t2 — No diff-handler receivers for immutable components

After an entity with only `ImmutableLabel` enters scope, the global diff handler must
contain **zero** receiver registrations. No `MutChannel` is allocated.

### t3 — Mixed entity: mutable component is still diff-tracked

An entity with both `Position` (mutable) and `ImmutableLabel` (immutable) enters scope.
The global diff handler must contain exactly **one** registration — for `Position`.
`ImmutableLabel` must not create a registration.
