# Bevy Adapter

**Crates:** `naia-bevy-shared`, `naia-bevy-server`, `naia-bevy-client`

The Bevy adapter wraps naia's core crates and exposes `Server` / `Client` as
Bevy resources, routes naia events into Bevy messages, and provides
`CommandsExt` extension methods for entity replication. If you are using Bevy,
use these crates instead of `naia-server` / `naia-client` directly.

---

## The `T` phantom type parameter

When using the Bevy client adapter, the `Client` SystemParam and `Plugin<T>`
carry a generic type parameter `T`:

```rust
use naia_bevy_client::{Client, Plugin};

#[derive(Resource)]
pub struct MyClient;

app.add_plugins(Plugin::<MyClient>::new(client_config, protocol()));

fn my_system(client: Client<MyClient>) { /* … */ }
```

**Why does `T` exist?** Bevy applications sometimes run more than one naia client
simultaneously — a split-screen game where each half is a separate session, or a
relay node bridging two servers. The `T` phantom marker lets Bevy distinguish the
two `Client` SystemParams at compile time. They become different Bevy resources
with no runtime overhead.

`T` must satisfy `Resource` (a Bevy bound) plus `Sync + Send + 'static`. A
`#[derive(Resource)]` unit struct always satisfies this.

---

## Single-client shorthand

For apps with only one naia client, use `DefaultClientTag` and `DefaultPlugin` to
skip the boilerplate:

```rust
use naia_bevy_client::{DefaultPlugin, Client, DefaultClientTag};

app.add_plugins(DefaultPlugin::new(client_config, protocol()));

fn my_system(client: Client<DefaultClientTag>) { /* … */ }
```

`DefaultClientTag` is a unit struct defined in `naia-bevy-client`. Use it
everywhere `T` appears: the plugin, the `Client<T>` SystemParam, and the event
types.

---

## Plugin registration

### Server

```rust
use naia_bevy_server::{Plugin, ServerConfig};

app.add_plugins(Plugin::new(ServerConfig::default(), protocol()));
```

### Client

```rust
use naia_bevy_client::{Plugin, ClientConfig, DefaultClientTag};

app.add_plugins(Plugin::<DefaultClientTag>::new(
    ClientConfig::default(),
    protocol(),
));
```

---

## System ordering

naia's Bevy plugins register their packet and world-sync systems internally. In
most apps you can read naia messages and mutate replicated components in normal
`Update` systems; the plugin handles receive/process/send ordering around them.
Advanced apps can order relative to the exported shared system sets when needed.

```rust
use naia_bevy_shared::{ProcessPackets, SendPackets};

app.configure_sets(Update, MyGameSet::Logic.after(ProcessPackets).before(SendPackets));
```

> **Warning:** If your simulation systems run before packet processing, they will
> see old network events. If they run after send, mutations made this frame wait
> for the next frame.

---

## Handling events

naia events are exposed through Bevy's message system, so normal systems should
read them with `MessageReader<T>`. Client-side event types carry the client tag
`T`; server-side event types do not.

```rust
use bevy_ecs::message::MessageReader;
use naia_bevy_server::{events::{ConnectEvent, DisconnectEvent}, Server};

fn handle_connections(
    mut server: Server,
    mut connect_reader: MessageReader<ConnectEvent>,
    mut disconnect_reader: MessageReader<DisconnectEvent>,
) {
    for ConnectEvent(user_key) in connect_reader.read() {
        println!("User connected: {:?}", user_key);
        // Spawn entities, add to rooms, etc.
    }

    for DisconnectEvent(user_key, address, reason) in disconnect_reader.read() {
        println!("User disconnected: {:?} {:?} {:?}", user_key, address, reason);
    }
}
```

### Client event types

```rust
use naia_bevy_client::events::{
    ConnectEvent,
    DisconnectEvent,
    SpawnEntityEvent,
    DespawnEntityEvent,
    InsertComponentEvent,
    UpdateComponentEvent,
    MessageEvents,
};
```

### Server event types

```rust
use naia_bevy_server::events::{
    ConnectEvent,
    DisconnectEvent,
    AuthEvents,
    MessageEvents,
    TickEvent,
};
```

---

## Entity replication via `CommandsExt`

The Bevy adapter provides `CommandsExt` extension methods on Bevy's `Commands`
that mirror the core naia entity API:

```rust
use naia_bevy_server::CommandsExt;

fn spawn_player(
    mut commands: Commands,
    mut server: Server,
    user_key: UserKey,
) {
    // Spawn a replicated entity.
    let entity = commands
        .spawn_empty()
        .enable_replication(&mut server)   // registers the entity with naia
        .insert(Position::new(0.0, 0.0))
        .id();

    // Place the user and entity in a shared room.
    server.room_mut(&room_key).add_user(&user_key);
    server.room_mut(&room_key).add_entity(&entity);
}
```

> **Note:** `enable_replication` must be called before `insert_component` on the entity.
> naia only diff-tracks components on entities it knows about.

---

## Sending messages

```rust
// Server → specific client:
server.send_message::<GameChannel, _>(&user_key, &ChatMessage { text: "Hello".into() })?;

// Client → server:
client.send_message::<GameChannel, _>(&ChatMessage { text: "Hi".into() });
```

---

## Multi-client setup

For games with two simultaneous naia clients:

```rust
#[derive(Resource)]
pub struct ClientA;

#[derive(Resource)]
pub struct ClientB;

app.add_plugins(Plugin::<ClientA>::new(config_a, protocol()))
   .add_plugins(Plugin::<ClientB>::new(config_b, protocol()));

fn system_a(client: Client<ClientA>) { /* … */ }
fn system_b(client: Client<ClientB>) { /* … */ }
```

Both clients are fully independent: separate connections, separate event queues,
separate entity sets. Bevy routes events to the correct client based on the `T`
type parameter.

---

## Full working example

See `demos/bevy/` for a complete Bevy demo covering entity replication, authority
delegation, and client-side prediction. The key files are:

- `demos/bevy/server/src/systems/events.rs` — server event handling, room setup,
  tick loop, and entity spawning.
- `demos/bevy/client/src/systems/events.rs` — client event handling, prediction
  loop, and rollback correction.
- `demos/bevy/shared/src/` — shared `Protocol`, components, channels, and
  movement behavior used by both sides.
