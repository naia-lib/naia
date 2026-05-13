# Entity Replication

Entity replication is the core mechanism by which networked world state moves
between hosts. The default path is server-owned state replicated to clients, but
naia also supports opt-in client-owned entities and delegated authority.

> **Core API:** Not using Bevy? The bare `naia-server` / `naia-client` API is
> identical in concept but uses a direct method-call style instead of Bevy
> systems. See [Core API Overview](../adapters/overview.md).

---

## The Replication Loop

Internally naia runs these five steps in order every frame:

```
receive_all_packets     – read UDP/WebRTC datagrams from the OS
process_all_packets     – decode packets; apply client mutations
                          (Bevy messages are populated here)
[YOUR SYSTEMS]          – read messages, mutate components
send_all_packets        – serialise diffs + messages; flush to network
```

With the Bevy adapter, `NaiaServerPlugin` and `NaiaClientPlugin` own
`receive_all_packets`, `process_all_packets`, and `send_all_packets`. Your
systems run between `process_all_packets` and `send_all_packets` automatically —
you never call those methods directly.

The equivalent Bevy system ordering looks like this:

```mermaid
graph LR
    A[receive_all_packets<br/>plugin] --> B[process_all_packets<br/>plugin]
    B --> C[Your systems<br/>read events, mutate components]
    C --> D[send_all_packets<br/>plugin]
```

> **Danger:** `send_all_packets` must be the **last** step. The plugin enforces
> this. In the bare core API, calling it inside a tick loop adds a full tick of
> latency to every component update.

---

## Spawning a replicated entity

With the Bevy adapter, use `CommandsExt::enable_replication`:

```rust
use naia_bevy_server::CommandsExt;

let entity = commands
    .spawn_empty()
    .enable_replication(&mut server)   // registers entity with naia
    .insert(Position::new(0.0, 0.0))  // initial component value
    .id();
```

On the next `send_all_packets`, naia sends a `SpawnEntity` packet to every
in-scope client with the initial component values.

To despawn a replicated entity, call `commands.entity(entity).despawn()`. naia
detects the despawn and sends `DespawnEntity` to all in-scope clients.

---

## Replication state machine

```mermaid
stateDiagram-v2
    [*] --> OutOfScope : entity spawned
    OutOfScope --> InScope : scope includes entity
    InScope --> OutOfScope : scope excludes entity / Despawn
    InScope --> Frozen : scope excludes entity / Persist
    Frozen --> InScope : scope.include(entity)
    InScope --> [*] : server despawns entity

    InScope --> InScope : property mutated → diff sent
```

---

## Receiving replication messages on the client

On the client, Bevy messages are emitted as naia processes incoming packets:

```rust
use bevy::ecs::message::MessageReader;
use naia_bevy_client::{
    events::{DespawnEntityEvent, InsertComponentEvent, SpawnEntityEvent, UpdateComponentEvent},
    DefaultClientTag,
};
use my_game_shared::Position;

fn handle_replication_events(
    mut spawn_reader: MessageReader<SpawnEntityEvent<DefaultClientTag>>,
    mut despawn_reader: MessageReader<DespawnEntityEvent<DefaultClientTag>>,
    mut insert_reader: MessageReader<InsertComponentEvent<DefaultClientTag, Position>>,
    mut update_reader: MessageReader<UpdateComponentEvent<DefaultClientTag, Position>>,
    positions: Query<&Position>,
) {
    for event in spawn_reader.read() {
        println!("Entity spawned: {:?}", event.entity);
    }

    for event in despawn_reader.read() {
        println!("Entity despawned: {:?}", event.entity);
    }

    for event in insert_reader.read() {
        if let Ok(pos) = positions.get(event.entity) {
            println!("Position inserted: ({:.2}, {:.2})", *pos.x, *pos.y);
        }
    }

    for event in update_reader.read() {
        if let Ok(pos) = positions.get(event.entity) {
            println!("Position updated: ({:.2}, {:.2})", *pos.x, *pos.y);
        }
    }
}
```

The `Position` component on the client entity is a standard Bevy component. naia
writes the latest server values into it before your systems run.

---

## Static vs Dynamic Entities

**Dynamic entities** (the default) use per-field delta tracking. When any
`Property<T>` field changes, only the changed fields are sent to each in-scope
user on the next `send_all_packets` call.

**Static entities** skip delta tracking entirely. When a static entity enters
a user's scope, naia sends a full component snapshot. After that no further
updates are transmitted — static entities are assumed to be immutable for the
lifetime of the session.

Create a static entity with Bevy:

```rust
// Bevy adapter — enable static replication before inserting components.
commands
    .spawn_empty()
    .as_static()
    .insert(tile);
```

> **Tip:** Use static entities for map tiles, level geometry, or any entity written once
> and never changed. They eliminate diff tracking and save significant CPU time
> on servers with many entities.

---

## Replicated Resources

A **replicated resource** is a singleton value replicated through the same
machinery as entities. Internally naia creates a hidden one-component entity to
carry the value. Resources are usually server-owned, but they can be configured
as delegated resources so a client can request authority and mutate them through
the normal authority path.

```rust
use naia_bevy_server::ServerCommandsExt;

// Insert a dynamic (diff-tracked) resource:
commands.replicate_resource(ScoreBoard::new());

// Insert a static (immutable) resource:
commands.replicate_resource_static(MapMetadata::new());

// Remove it later:
commands.remove_replicated_resource::<ScoreBoard>();
```

On the client:

```rust
fn read_scoreboard(scoreboard: Option<Res<ScoreBoard>>) {
    if let Some(scoreboard) = scoreboard {
        // Read it like any other Bevy resource.
    }
}
```

Resources differ from ordinary entities in three ways:

- No room or scope configuration is needed.
- At most one resource per type can exist at a time (inserting a duplicate
  returns `Err(ResourceAlreadyExists)`).
- They can be delegated just like entities by calling `configure_resource` /
  `configure_replicated_resource`.

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
