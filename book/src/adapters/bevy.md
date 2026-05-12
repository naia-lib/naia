# Bevy Adapter

**Crates:** `naia-bevy-server` (server), `naia-bevy-client` (client)

The Bevy adapter wraps naia's core crates and exposes `Server` / `Client` as
Bevy resources, routes naia events into Bevy's event system, and provides
`CommandsExt` extension methods for entity replication. If you are using Bevy,
use these crates instead of `naia-server` / `naia-client` directly.

---

## The `T` phantom type parameter

When using the Bevy adapter, the `Client` SystemParam and `NaiaClientPlugin`
carry a generic type parameter `T`:

```rust
use naia_bevy_client::{Client, NaiaClientPlugin};

#[derive(Resource)]
pub struct MyClient;

app.add_plugins(NaiaClientPlugin::<MyClient>::new(client_config, protocol()));

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
use naia_bevy_server::{NaiaServerPlugin, ServerConfig};

app.add_plugins(NaiaServerPlugin::new(ServerConfig::default(), protocol()));
```

### Client

```rust
use naia_bevy_client::{NaiaClientPlugin, ClientConfig, DefaultClientTag};

app.add_plugins(NaiaClientPlugin::<DefaultClientTag>::new(
    ClientConfig::default(),
    protocol(),
));
```

---

## System ordering

naia's Bevy plugins register systems internally. You must order your game systems
relative to naia's system sets to avoid processing stale events:

```rust
use naia_bevy_server::SystemSet as NaiaServerSet;
use naia_bevy_client::SystemSet as NaiaClientSet;

// Server ordering:
app.configure_sets(
    Update,
    (
        NaiaServerSet::ReceiveEvents,   // naia drains inbound packets
        MyGameSet::Logic,               // your simulation step
        NaiaServerSet::Send,            // naia flushes outbound packets
    ).chain(),
);

// Client ordering:
app.configure_sets(
    Update,
    (
        NaiaClientSet::ReceiveEvents,
        MyGameSet::Logic,
        NaiaClientSet::Send,
    ).chain(),
);
```

> **Warning:** If your simulation systems run before `ReceiveEvents`, they will see
> last frame's events. If they run after `Send`, mutations made this frame won't
> be flushed until next frame. Both orderings add one frame of latency.

---

## Handling events

naia events are routed into Bevy's `EventReader` system. Each event type carries
the client tag `T` as a type parameter:

```rust
use naia_bevy_server::events::{ConnectEvent, DisconnectEvent, MessageEvent};

fn handle_connections(
    mut server: Server<DefaultClientTag>,
    mut connect_reader: EventReader<ConnectEvent>,
    mut disconnect_reader: EventReader<DisconnectEvent>,
) {
    for event in connect_reader.read() {
        let user_key = event.user_key;
        println!("User connected: {:?}", user_key);
        // Spawn entities, add to rooms, etc.
    }

    for event in disconnect_reader.read() {
        println!("User disconnected: {:?}", event.user_key);
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
    MessageEvent,
};
```

### Server event types

```rust
use naia_bevy_server::events::{
    ConnectEvent,
    DisconnectEvent,
    AuthEvent,
    MessageEvent,
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

app.add_plugins(NaiaClientPlugin::<ClientA>::new(config_a, protocol()))
   .add_plugins(NaiaClientPlugin::<ClientB>::new(config_b, protocol()));

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
