# Client-Owned Entities

In addition to server-owned and delegated entities, naia supports client-created
entities that replicate back to the server via the `Publicity` API.

Client-authoritative entities are opt-in. The shared protocol must call
`enable_client_authoritative_entities()` before clients may spawn, publish, or
mutate client-owned replicated entities.

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
components. If other clients share the right room/scope, the server may also
replicate that public entity onward to them. This is distinct from authority
delegation over a server-spawned entity: here the client is the origin.

## `Publicity::Private`

`Publicity::Private` means the client-owned entity replicates to the server but
is not published to other clients. This is the default for entities created by
the client.

Use this for state the server must validate or react to, but that other clients
do not need to see directly.

---

## Use cases

- **Client-owned projectiles** — the client spawns a bullet, marks it public,
  and the server validates the trajectory.
- **Private client intent/state** — replicated to the server but not fanned out
  to other clients.
- **Pure UI / local effects** — do not enable naia replication at all.

> **Warning:** As with delegated authority, the server must validate all component values
> received from client-owned public entities. naia replicates what the client
> sends without validation.
