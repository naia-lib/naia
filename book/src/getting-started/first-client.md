# Your First Client

This chapter connects a minimal naia client to the server built in
[Your First Server](first-server.md). The client receives entity spawn and
update events from the server.

---

## Client binary

```rust
// client/src/main.rs
use naia_client::{Client, ClientConfig, transport::udp::NativeSocket};
use my_game_shared::{protocol, Position};

#[async_std::main]
async fn main() {
    let mut client = Client::new(ClientConfig::default(), protocol());
    client.connect(NativeSocket::new("127.0.0.1:14191"));

    loop {
        client.receive_all_packets();
        client.process_all_packets();

        for event in client.take_world_events(&mut world).drain() {
            match event {
                WorldEvent::Connect => {
                    println!("Connected to server!");
                }
                WorldEvent::Disconnect => {
                    println!("Disconnected.");
                }
                WorldEvent::SpawnEntity(entity) => {
                    println!("Entity spawned: {:?}", entity);
                }
                WorldEvent::UpdateComponent(entity, kind) => {
                    if let Some(pos) = world.get_component::<Position>(&entity) {
                        println!("Position updated: ({}, {})", *pos.x, *pos.y);
                    }
                }
                _ => {}
            }
        }

        for _tick in client.take_tick_events().drain() {
            // Client-side simulation step
        }

        client.send_all_packets(&mut world);
        async_std::task::sleep(std::time::Duration::from_millis(1)).await;
    }
}
```

> **Note:** This is a simplified sketch. See `demos/basic/client/` for a complete working
> example including proper world management.

---

## Running both together

```sh
# Terminal 1 — server
cargo run -p server

# Terminal 2 — client
cargo run -p client
```

You should see "Connected to server!" and position updates as the server ticks.

---

## Browser client

For a browser client, see [WebRTC (Browser Clients)](../transports/webrtc.md)
and `demos/basic/client/wasm_bindgen/`.
