# Contract 18 — SpawnWithComponents Coalesce (Phase 4)

## Context

`init_entity_send_host_commands` currently sends one `EntityCommand::Spawn` plus one
`EntityCommand::InsertComponent` per component in the entity's initial set. For 10K
tiles × 1 component, that is 20K reliable-channel messages at level load. Each
carries its own `CommandId` delta, `EntityMessageType` tag, and ack-tracking record.

Phase 4 adds `EntityCommand::SpawnWithComponents(global_entity, component_kinds)` and
`EntityMessageType::SpawnWithComponents`. The wire format is:

```
CommandId delta
EntityMessageType::SpawnWithComponents tag
HostEntity (varint)
u8 component_count
[write(...) payload]×component_count
```

One reliable message replaces (1 + K) messages. The receiver splits it back into
individual `spawn` + `insert_component×N` events; the client `EntityEvent` API is
unchanged.

The existing `EntityCommand::Spawn` path is preserved for the zero-component case.
The existing `EntityCommand::InsertComponent` path is preserved for components added
after an entity has already been spawned in scope.

---

## Obligations

### t1 — Multi-component entity: all components available after coalesced spawn

When an entity with multiple components enters scope, the client observes every
component present immediately following the spawn event. No additional delivery
cycle is required before the components are accessible.

On both paths (legacy and Phase 4), the client eventually receives all components.
The Phase 4 coalesced path delivers them in one reliable message instead of several.

### t2 — Initial component values are correct after coalesced spawn

The field values serialised into `SpawnWithComponents` must equal the server-side
values at the moment the entity enters scope. The client must reproduce them
faithfully from the coalesced payload.

### t3 — Zero-component entity: existing Spawn path preserved

When an entity with no replicated components enters scope, the legacy
`EntityCommand::Spawn` path is used. No coalesced message is emitted, and the
client spawns the entity correctly.

### t4 — Behavioral equivalence

Post-spawn component mutations replicate normally after a coalesced spawn. The
SpawnWithComponents coalesce is transparent to subsequent update replication.

Covered by: existing Contract 7 (entity_replication) scenarios. Not re-tested here.
