# Authority Delegation

By default, server-spawned replicated entities are server-owned. **Delegation**
allows a client to take temporary write authority over a specific entity or
resource. While the client holds authority, its mutations replicate back to the
server instead of the other way around.

Delegation is related to, but separate from, client-authoritative entities. A
client-owned published entity can be migrated into delegated state; after that
migration it is server-owned and follows the same grant/deny/revoke rules as
any other delegated entity.

---

## Authority state machine

```mermaid
stateDiagram-v2
    [*] --> Available : server marks entity Delegated
    Available --> Requested : client calls request_authority()
    Requested --> Granted : server grants
    Requested --> Denied : server denies
    Denied --> Available : client releases
    Granted --> Releasing : client calls release_authority()
    Releasing --> Available : server acknowledges
    Granted --> Available : server calls take_authority()
```

---

## Trust model

- The server may **revoke** authority at any time by calling `take_authority`
  through the Bevy `CommandsExt` API.
- The client **never** holds unrevocable ownership.
- Mutations from a client-held delegated entity should still be validated
  server-side before applying to authoritative game state. naia replicates what
  the client sends; it does not validate or clamp values.

> **Danger:** naia does not validate client mutations. If a client has authority over a
> `Position` component, it can send any coordinate it likes. Always range-check
> and sanity-validate delegated values on the server before applying them to
> authoritative game state.

---

## Server setup

```rust
use naia_bevy_server::{CommandsExt, ReplicationConfig};

commands
    .spawn_empty()
    .enable_replication(&mut server)
    .configure_replication(ReplicationConfig::delegated())
    .insert(position);
```

If you skip `enable_replication()`, you have created a perfectly normal Bevy
entity. naia will politely ignore it, as requested.

---

## Client request flow

```rust
use bevy::ecs::message::MessageReader;
use naia_bevy_client::{
    events::{EntityAuthDeniedEvent, EntityAuthGrantedEvent},
    Client, CommandsExt,
};

// Client: request authority over a delegated entity.
commands.entity(entity).request_authority(&mut client);

// Client: observe the server's grant/deny response.
fn handle_authority_response(
    mut granted_reader: MessageReader<EntityAuthGrantedEvent<Main>>,
    mut denied_reader: MessageReader<EntityAuthDeniedEvent<Main>>,
) {
    for event in granted_reader.read() {
        println!("Authority granted for {:?}", event.entity);
    }

    for event in denied_reader.read() {
        println!("Authority denied for {:?}", event.entity);
    }
}
```

---

## Per-user authority

Only one client can hold authority over a given entity at a time. The server
controls who may request and who is granted authority. Treat request handling as
game logic: check the requesting user, current state, anti-cheat constraints, and
whether the entity is currently in that user's scope before granting.

---

## Delegated resources

Resources can also be delegated using `configure_replicated_resource` in Bevy:

```rust
use naia_bevy_server::{ReplicationConfig, ServerCommandsExt};

commands.configure_replicated_resource::<ScoreBoard>(ReplicationConfig::delegated());
```

This lets a client request authority over singleton state through the same
authority-channel flow used for entities. On Bevy clients, use
`commands.request_resource_authority::<MyClientTag, ScoreBoard>()` after the
resource is present locally.

---

## Relationship to `Publicity`

On the client side, the `Publicity` enum controls how a locally created entity
is visible to the server:

```rust
use naia_bevy_client::{CommandsExt, Publicity};

commands
    .entity(entity)
    .configure_replication::<Main>(Publicity::Public);
```

`Publicity::Private` and `Publicity::Public` are both client-owned replicated
states: private reaches the server only, public may also be fanned out to other
in-scope clients. `Publicity::Delegated` migrates the entity into the delegated
authority model, where the server owns the entity and authority can be granted
or revoked.
