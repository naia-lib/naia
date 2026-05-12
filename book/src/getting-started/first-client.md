# Your First Client

This chapter connects a Bevy client to the server built in
[Your First Server](first-server.md). The client receives entity spawn,
update, and despawn events from the server using Bevy's standard event system.

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
bevy             = { version = "0.13", default-features = false, features = ["bevy_core_pipeline"] }
naia-bevy-client = "0.24"
my-game-shared   = { path = "../shared" }
```

For a browser (WASM) client, add the `wbindgen` feature:

```toml
naia-bevy-client = { version = "0.24", features = ["wbindgen"] }
```

---

## Plugin setup

`NaiaClientPlugin` handles the packet loop automatically. Your systems only read
events.

```rust
use bevy::prelude::*;
use naia_bevy_client::{ClientConfig, Plugin as NaiaClientPlugin};
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
use naia_bevy_client::{transport::udp::NativeSocket, Client};

fn startup(mut client: Client) {
    client.connect(NativeSocket::new("127.0.0.1:14191"));
    println!("Connecting to 127.0.0.1:14191 ...");
}
```

---

## What the client does NOT do

The Bevy client plugin owns the packet loop. You do not call:

- `receive_all_packets`
- `process_all_packets`
- `send_all_packets`

The plugin runs those before and after your systems in the Bevy schedule. Your
job is only to read the resulting events.

---

## Connection events

```rust
use naia_bevy_client::events::{ConnectEvent, DisconnectEvent};

fn handle_connect(mut connect_reader: EventReader<ConnectEvent>) {
    for ConnectEvent(_) in connect_reader.read() {
        println!("Connected to server!");
    }
}

fn handle_disconnect(mut disconnect_reader: EventReader<DisconnectEvent>) {
    for DisconnectEvent(_) in disconnect_reader.read() {
        println!("Disconnected from server.");
        // Despawn stale entities here — naia does NOT do this automatically.
        // Without cleanup you will get duplicate entities on reconnect.
    }
}
```

---

## Entity lifecycle events

```rust
use naia_bevy_client::events::{SpawnEntityEvent, DespawnEntityEvent};

fn handle_spawn(mut spawn_reader: EventReader<SpawnEntityEvent>) {
    for SpawnEntityEvent(entity) in spawn_reader.read() {
        println!("Entity spawned: {:?}", entity);
    }
}

fn handle_despawn(mut despawn_reader: EventReader<DespawnEntityEvent>) {
    for DespawnEntityEvent(entity) in despawn_reader.read() {
        println!("Entity despawned: {:?}", entity);
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
use naia_bevy_client::events::{InsertComponentEvent, UpdateComponentEvent};
use my_game_shared::Position;

fn handle_insert_position(
    mut insert_reader: EventReader<InsertComponentEvent<Position>>,
    positions: Query<&Position>,
) {
    for InsertComponentEvent(entity) in insert_reader.read() {
        if let Ok(pos) = positions.get(*entity) {
            println!("Position inserted: ({:.2}, {:.2})", *pos.x, *pos.y);
        }
    }
}

fn handle_update_position(
    mut update_reader: EventReader<UpdateComponentEvent<Position>>,
    positions: Query<&Position>,
) {
    for UpdateComponentEvent(entity) in update_reader.read() {
        if let Ok(pos) = positions.get(*entity) {
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
use naia_bevy_client::{Client, events::ClientTickEvent};
use my_game_shared::{InputChannel, PlayerInput};

fn handle_tick(
    mut client: Client,
    mut tick_reader: EventReader<ClientTickEvent>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    for ClientTickEvent(_tick) in tick_reader.read() {
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

| Event | When it fires |
|-------|---------------|
| `ConnectEvent` | Handshake complete; connection established |
| `DisconnectEvent` | Connection dropped (timeout or explicit) |
| `SpawnEntityEvent` | Server spawned an entity now in your scope |
| `DespawnEntityEvent` | Entity left your scope or server despawned it |
| `InsertComponentEvent<C>` | Component `C` first arrived for an entity |
| `UpdateComponentEvent<C>` | One or more fields of `C` changed on the server |
| `ClientTickEvent` | Client tick elapsed; send input here |
| `MessageEvent<Ch, M>` | Server sent a typed message `M` on channel `Ch` |
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

Swap only the transport. All event-handling code stays the same:

```rust
use naia_bevy_client::transport::webrtc::WebrtcSocket;

fn startup(mut client: Client) {
    client.connect(WebrtcSocket::new("https://myserver.example.com", 14192));
}
```

Enable the `wbindgen` feature, build with `wasm-pack build --target web` or
`trunk build --release`, and serve the output directory over HTTP.

See [WebRTC (Browser Clients)](../transports/webrtc.md) for the complete setup.

---

## Next steps

- [The Shared Protocol](../concepts/protocol.md) — understand `ProtocolId` and type registration.
- [Rooms & Scoping](../concepts/rooms.md) — control which entities each client sees.
- [Client-Side Prediction & Rollback](../advanced/prediction.md) — use `TickBuffered` input to predict before the server confirms.
- [Running the Demos](demos.md) — run the complete `demos/bevy/` example end-to-end.
