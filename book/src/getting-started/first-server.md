# Your First Server

This walkthrough builds a minimal naia server from scratch using the core crates
(no ECS framework). The server listens on UDP, accepts connections, spawns a
replicated entity per user, and sends tick updates.

---

## Project layout

```
my_game/
  shared/   ← protocol, components, channels
  server/   ← naia-server binary
  client/   ← naia-client binary (next chapter)
```

## 1. The shared protocol

```rust
// shared/src/lib.rs
use naia_shared::{Protocol, ChannelDirection, ChannelMode};

#[derive(Replicate, Clone)]
pub struct Position {
    pub x: Property<f32>,
    pub y: Property<f32>,
}

#[derive(Channel)]
pub struct GameChannel;

pub fn protocol() -> Protocol {
    Protocol::builder()
        .tick_interval(std::time::Duration::from_millis(50)) // 20 Hz
        .add_component::<Position>()
        .add_channel::<GameChannel>(
            ChannelDirection::ServerToClient,
            ChannelMode::OrderedReliable(Default::default()),
        )
        .build()
}
```

## 2. The server binary

```rust
// server/src/main.rs
use naia_server::{Server, ServerConfig, transport::udp::NativeSocket};
use my_game_shared::protocol;

#[async_std::main]
async fn main() {
    let mut server = Server::new(ServerConfig::default(), protocol());
    server.listen(NativeSocket::new("0.0.0.0:14191"));

    loop {
        server.receive_all_packets();
        server.process_all_packets();

        for event in server.take_world_events(&mut world).drain() {
            match event {
                WorldEvent::Connect(user_key) => {
                    // Spawn a replicated entity for this user
                    let entity = world.spawn();
                    server.spawn_entity(&mut world, entity)
                        .insert_component(Position { x: 0.0.into(), y: 0.0.into() });
                    let room = server.create_room();
                    server.room_mut(&room).add_user(&user_key);
                    server.room_mut(&room).add_entity(&entity);
                }
                WorldEvent::Disconnect(user_key) => {
                    // Clean up on disconnect
                }
                _ => {}
            }
        }

        for _tick in server.take_tick_events().drain() {
            // Mutate replicated components here
        }

        server.send_all_packets(&mut world);
        async_std::task::sleep(std::time::Duration::from_millis(1)).await;
    }
}
```

> **Note:** This is a simplified sketch. See `demos/basic/server/` for a complete working
> example including proper event handling and world management.

---

## Next steps

- [Your First Client](first-client.md) — connect to this server from a client.
- [Rooms & Scoping](../concepts/rooms.md) — fine-grained visibility control.
- [Messages & Channels](../concepts/messages.md) — sending typed messages.
