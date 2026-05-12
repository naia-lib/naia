# Client-Owned Entities

In addition to server-side delegated authority, naia supports client-created
entities that replicate back to the server via the `Publicity` API.

---

## `Publicity::Public`

A client can create an entity locally and mark it as `Public`, causing it to
replicate to the server:

```rust
use naia_client::Publicity;

// Client creates and publishes a local entity:
let entity = world.spawn();
client.entity_mut(&mut world, &entity)
    .insert_component(MyComponent { value: 42.into() })
    .configure_replication(Publicity::Public);
```

The server receives a `SpawnEntityEvent` for the entity and can read its
components. This is distinct from authority delegation over a server-spawned
entity — here the client is the origin.

## `Publicity::Private`

`Publicity::Private` keeps the entity purely client-local — it is never sent to
the server. This is the default for entities created by the client.

---

## Use cases

- **Client-owned projectiles** — the client spawns a bullet, marks it public,
  and the server validates the trajectory.
- **UI / local effects** — spawned as `Private` so they never touch the network.

> **Warning:** As with delegated authority, the server must validate all component values
> received from client-owned public entities. naia replicates what the client
> sends without validation.
