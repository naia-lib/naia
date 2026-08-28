# Your First Client

This chapter connects a Bevy client to the server built in
[Your First Server](first-server.md). The client receives entity spawn,
update, and despawn messages from the server using Bevy's message system.

> **Core API:** Not using Bevy? The bare `naia-client` API is identical in
> concept but uses a direct method-call loop. See
> [Core API Overview](../adapters/overview.md).

---

## Cargo.toml

```toml
# client/Cargo.toml
[package]
name    = "my-game-client"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "client"
path = "src/main.rs"

[dependencies]
bevy = { version = "0.18", default-features = false, features = ["bevy_core_pipeline"] }
naia-bevy-client = { version = "0.25", features = ["transport_webrtc"] }
my-game-shared   = { path = "../shared" }
```

For native or browser clients, enable the WebRTC transport:

```toml
naia-bevy-client = { version = "0.25", features = ["transport_webrtc"] }
```

---

## Plugin setup

`NaiaClientPlugin` handles the packet loop automatically. Your systems only
read messages.

```rust
use bevy::prelude::*;
use naia_bevy_client::{ClientConfig, DefaultPlugin as NaiaClientPlugin};
use my_game_shared::protocol;

fn main() {
    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(NaiaClientPlugin::new(ClientConfig::default(), protocol()))
        .add_systems(Startup, startup)
        .add_systems(
            Update,
            (
                handle_connect,
                handle_disconnect,
                handle_spawn,
                handle_despawn,
                handle_insert_position,
                handle_update_position,
                handle_tick,
            ),
        )
        .run();
}
```

---

## Startup — connect

```rust
use naia_bevy_client::{transport::webrtc, Client, DefaultClientTag};

fn startup(mut client: Client<DefaultClientTag>) {
    client.connect(webrtc::Socket::new("http://127.0.0.1:14191", client.socket_config()));
    println!("Connecting to http://127.0.0.1:14191 ...");
}
```

---

## What the client does NOT do

The Bevy client plugin owns the packet loop. You do not call:

- `receive_all_packets`
- `process_all_packets`
- `send_all_packets`

The plugin runs those before and after your systems in the Bevy schedule. Your
job is only to read the resulting messages.

---

## Connection events

```rust
use bevy::ecs::message::MessageReader;
use naia_bevy_client::{events::{ConnectEvent, DisconnectEvent}, DefaultClientTag};

fn handle_connect(mut connect_reader: MessageReader<ConnectEvent<DefaultClientTag>>) {
    for _ in connect_reader.read() {
        println!("Connected to server!");
    }
}

fn handle_disconnect(mut disconnect_reader: MessageReader<DisconnectEvent<DefaultClientTag>>) {
    for _ in disconnect_reader.read() {
        println!("Disconnected from server.");
        // Despawn stale entities here — naia does NOT do this automatically.
        // Without cleanup you will get duplicate entities on reconnect.
    }
}
```

---

## Entity lifecycle events

```rust
use bevy::ecs::message::MessageReader;
use naia_bevy_client::{events::{SpawnEntityEvent, DespawnEntityEvent}, DefaultClientTag};

fn handle_spawn(mut spawn_reader: MessageReader<SpawnEntityEvent<DefaultClientTag>>) {
    for event in spawn_reader.read() {
        println!("Entity spawned: {:?}", event.entity);
    }
}

fn handle_despawn(mut despawn_reader: MessageReader<DespawnEntityEvent<DefaultClientTag>>) {
    for event in despawn_reader.read() {
        println!("Entity despawned: {:?}", event.entity);
    }
}
```

> **Note:** When naia spawns a server entity locally, it creates a real Bevy
> `Entity`. `SpawnEntityEvent` carries that `Entity` handle — you can pass it to
> `Query`, `Commands::entity`, etc. just like any other Bevy entity.

---

## Component events

`InsertComponentEvent<C>` fires once when a component first arrives for an
entity. `UpdateComponentEvent<C>` fires whenever any field of that component
changes on the server.

```rust
use bevy::ecs::message::MessageReader;
use naia_bevy_client::{events::{InsertComponentEvent, UpdateComponentEvent}, DefaultClientTag};
use my_game_shared::Position;

fn handle_insert_position(
    mut insert_reader: MessageReader<InsertComponentEvent<DefaultClientTag, Position>>,
    positions: Query<&Position>,
) {
    for event in insert_reader.read() {
        if let Ok(pos) = positions.get(event.entity) {
            println!("Position inserted: ({:.2}, {:.2})", *pos.x, *pos.y);
        }
    }
}

fn handle_update_position(
    mut update_reader: MessageReader<UpdateComponentEvent<DefaultClientTag, Position>>,
    positions: Query<&Position>,
) {
    for event in update_reader.read() {
        if let Ok(pos) = positions.get(event.entity) {
            println!("Position updated:  ({:.2}, {:.2})", *pos.x, *pos.y);
        }
    }
}
```

The `Position` component is a standard Bevy component on the client entity — you
read it with an ordinary `Query`. naia writes the latest server values into it
before your systems run.

---

## Tick event and sending input

```rust
use bevy::ecs::message::MessageReader;
use naia_bevy_client::{Client, DefaultClientTag, events::ClientTickEvent};
use my_game_shared::{InputChannel, PlayerInput};

fn handle_tick(
    mut client: Client<DefaultClientTag>,
    mut tick_reader: MessageReader<ClientTickEvent<DefaultClientTag>>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    for _ in tick_reader.read() {
        let input = PlayerInput {
            up:    keyboard.pressed(KeyCode::KeyW),
            down:  keyboard.pressed(KeyCode::KeyS),
            left:  keyboard.pressed(KeyCode::KeyA),
            right: keyboard.pressed(KeyCode::KeyD),
        };
        // send_tick_buffer_message stamps the message with the current
        // client tick so the server delivers it at the matching simulation step.
        client.send_tick_buffer_message::<InputChannel, _>(&input);
    }
}
```

---

## Full client event reference

| Message | When it is emitted |
|-------|---------------|
| `ConnectEvent` | Handshake complete; connection established |
| `DisconnectEvent` | Connection dropped (timeout or explicit) |
| `SpawnEntityEvent` | Server spawned an entity now in your scope |
| `DespawnEntityEvent` | Entity left your scope or server despawned it |
| `InsertComponentEvent<C>` | Component `C` first arrived for an entity |
| `UpdateComponentEvent<C>` | One or more fields of `C` changed on the server |
| `ClientTickEvent` | Client tick elapsed; send input here |
| `MessageEvents` | Server sent typed messages; call `events.read::<Channel, Message>()` |
| `PublishEntityEvent` | A delegated entity was published to the server |
| `UnpublishEntityEvent` | A delegated entity was unpublished |

---

## Running both sides

```sh
# Terminal 1 — server first
cargo run -p my-game-server

# Terminal 2 — client
cargo run -p my-game-client
```

Expected output:

```
Connecting to 127.0.0.1:14191 ...
Connected to server!
Entity spawned: Entity(0v1)
Position inserted: (0.00, 0.00)
Position updated:  (0.10, 0.00)
Position updated:  (0.20, 0.00)
…
```

---

## Browser client

Use the same `transport_webrtc` module. All event-handling code stays the same:

```rust
use naia_bevy_client::{transport::webrtc, Client, DefaultClientTag};

fn startup(mut client: Client<DefaultClientTag>) {
    let socket = webrtc::Socket::new("https://myserver.example.com", client.socket_config());
    client.connect(socket);
}
```

Build with `wasm-pack build --target web` or `trunk build --release`, and serve
the output directory over HTTP.

See [WebRTC (Native + Browser)](../transports/webrtc.md) for the complete setup.

---

## Next steps

- [The Shared Protocol](../concepts/protocol.md) — understand `ProtocolId` and type registration.
- [Rooms & Scoping](../concepts/rooms.md) — control which entities each client sees.
- [Client-Side Prediction & Rollback](../advanced/prediction.md) — use `TickBuffered` input to predict before the server confirms.
- [Running the Demos](demos.md) — run the complete `demos/bevy/` example end-to-end.
