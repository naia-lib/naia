# Migration Guide

Before/after snippets for every breaking change in the post-cleanup API.
Each block shows the old code (marked `// BEFORE`) and the new code
(marked `// AFTER`).

---

## `spawn_static_entity` → `spawn_entity().as_static()`

```rust
// BEFORE
server.spawn_static_entity(&mut world)
    .insert_component(tile);

// AFTER
server.spawn_entity(&mut world)
    .as_static()
    .insert_component(tile);
```

> **Warning:** `.as_static()` must be called **before** the first `.insert_component()`.

---

## `insert_static_resource` → `insert_resource(..., true)`

```rust
// BEFORE
server.insert_static_resource(&mut world, map_metadata)?;

// AFTER
server.insert_resource(&mut world, map_metadata, true)?;
```

---

## `insert_resource` now requires `is_static: bool`

```rust
// BEFORE
server.insert_resource(&mut world, scoreboard)?;

// AFTER
server.insert_resource(&mut world, scoreboard, false)?;
```

---

## `WorldEvents<E>` → `Events<E>` (client)

```rust
// BEFORE
use naia_client::WorldEvents;
let events: WorldEvents<E> = client.take_world_events();

// AFTER
use naia_client::Events;
let events: Events<E> = client.take_world_events();
```

---

## `make_room` → `create_room`

```rust
// BEFORE
let room = server.make_room();

// AFTER
let room = server.create_room();
```

---

## `resource_count` → `resources_count`

```rust
// BEFORE
let n = server.resource_count();

// AFTER
let n = server.resources_count();
```

---

## `room_count` → `rooms_count` (on `UserRef`)

```rust
// BEFORE
let n = server.user(&user_key).room_count();

// AFTER
let n = server.user(&user_key).rooms_count();
```

---

## Client `ReplicationConfig` → `Publicity`

```rust
// BEFORE
use naia_client::ReplicationConfig;
client.entity_mut(&mut world, &entity)
    .configure_replication(ReplicationConfig::Public);

// AFTER
use naia_client::Publicity;
client.entity_mut(&mut world, &entity)
    .configure_replication(Publicity::Public);
```

---

## `send_message` returns `Result` on the server

```rust
// BEFORE
server.send_message::<GameChannel, _>(&user_key, &msg);

// AFTER
server.send_message::<GameChannel, _>(&user_key, &msg)?;
// or handle the error explicitly:
if let Err(e) = server.send_message::<GameChannel, _>(&user_key, &msg) {
    // user disconnected between taking events and sending — safe to ignore
}
```

---

## `insert_components` removed from server `EntityMut`

```rust
// BEFORE
entity_mut.insert_components(vec![Box::new(position), Box::new(health)]);

// AFTER
entity_mut.insert_component(position);
entity_mut.insert_component(health);
```
