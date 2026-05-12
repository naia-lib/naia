# Entity Replication

Entity replication is the core mechanism by which the server's game state is
delivered to connected clients. naia tracks which entities are in each user's
scope and automatically sends only the changed fields each tick.

---

## The Replication Loop

Every frame the server must execute these five steps in order:

```
receive_all_packets     – read UDP/WebRTC datagrams from the OS
process_all_packets     – decode packets; apply client mutations
take_world_events       – drain connect/disconnect/spawn/update/message events
take_tick_events        – advance the tick clock; collect elapsed tick events
                          (mutate replicated components here)
send_all_packets        – serialise diffs + messages; flush to network
```

> **Danger:** `send_all_packets` must be the **last** step. Calling it inside the
> `TickEvent` loop adds a full tick of latency to every component update.

**Why this order is mandatory:**

- `receive_all_packets` fills the internal receive queue; nothing downstream
  can run until bytes are available.
- `process_all_packets` consumes that queue and converts bytes into
  `EntityEvent` objects that `take_world_events` later drains.
- `take_world_events` must come after `process_all_packets` so that events
  produced by the latest batch of packets are visible this frame.
- `take_tick_events` must come after `take_world_events` to avoid ordering
  anomalies between world-state events and tick-boundary events.
- `send_all_packets` must come last so that all mutations made during the
  current frame are included in the outbound batch.

The same five-step contract applies to the client, with the difference that
the client processes packets from a single server connection rather than from
many users.

---

## Replication state machine

```mermaid
stateDiagram-v2
    [*] --> OutOfScope : entity spawned
    OutOfScope --> InScope : scope.include(entity)
    InScope --> OutOfScope : scope.exclude(entity) [ScopeExit::Despawn]
    InScope --> Frozen : scope.exclude(entity) [ScopeExit::Persist]
    Frozen --> InScope : scope.include(entity)
    InScope --> [*] : server despawns entity

    InScope --> InScope : property mutated → diff sent
```

---

## Static vs Dynamic Entities

**Dynamic entities** (the default) use per-field delta tracking. When any
`Property<T>` field changes, only the changed fields are sent to each in-scope
user on the next `send_all_packets` call.

**Static entities** skip delta tracking entirely. When a static entity enters
a user's scope, naia sends a full component snapshot. After that no further
updates are transmitted — static entities are assumed to be immutable for the
lifetime of the session.

Create a static entity via the `as_static()` builder method:

```rust
server.spawn_entity(&mut world)
    .as_static()           // must be called BEFORE insert_component
    .insert_component(tile);
```

> **Tip:** Use static entities for map tiles, level geometry, or any entity written once
> and never changed. They eliminate diff tracking and save significant CPU time
> on servers with many entities.

---

## Replicated Resources

A **replicated resource** is a server-side singleton that is automatically
visible to all connected users, without room membership or scope management.
Internally naia creates a hidden one-component entity to carry the value.

```rust
// Insert a dynamic (diff-tracked) resource:
server.insert_resource(&mut world, ScoreBoard::new(), false)?;

// Insert a static (immutable) resource:
server.insert_resource(&mut world, MapMetadata::new(), true)?;

// Remove it later:
server.remove_resource::<ScoreBoard, _>(&mut world);
```

On the client:

```rust
if client.has_resource::<ScoreBoard>() {
    let entity = client.resource_entity::<ScoreBoard>().unwrap();
    // read component from world storage using entity
}
```

Resources differ from ordinary entities in three ways:

- No room or scope configuration is needed.
- At most one resource per type can exist at a time (inserting a duplicate
  returns `Err(ResourceAlreadyExists)`).
- They can be delegated just like entities by calling `configure_resource`.

---

## Multi-Server / Zone Architecture

naia is a single-process authority. For games that need horizontal scaling
(e.g. an open world split across geographic zones), the standard pattern is
**zone sharding at the application layer**:

```
Zone A server (naia process)          Zone B server (naia process)
  owns entities in region A             owns entities in region B
        │                                       │
        └───── coordination service ────────────┘
                 (entity hand-off, cross-zone messages, matchmaking)
```

When a player moves between zones the application:

1. Serializes the player's replicated state on the source server.
2. Sends the state to the destination server via your coordination channel.
3. Despawns the entity on the source server (client gets a despawn event).
4. Spawns the entity on the destination server and places the player's
   connection in the new room.

> **Note:** naia provides the per-process primitive (`spawn_entity`, rooms, scopes,
> authority). Zone coordination is an application concern.
