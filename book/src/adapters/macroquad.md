# Macroquad Adapter

The macroquad adapter (`naia-macroquad-client`) provides a client integration
for the macroquad game framework. Only the client is supported — the server
always runs natively using `naia-server` directly.

---

## Setup

```toml
# client/Cargo.toml
[dependencies]
naia-macroquad-client = "0.24"
naia-shared = { path = "../shared" }
```

---

## Game loop integration

```rust
use naia_macroquad_client::{Client, ClientConfig};
use my_game_shared::protocol;

#[macroquad::main("My Game")]
async fn main() {
    let mut client = Client::new(ClientConfig::default(), protocol());
    client.connect(Socket::new("127.0.0.1:14191", None));

    loop {
        client.receive_all_packets();
        client.process_all_packets();

        for event in client.take_world_events(&mut world).drain() {
            // handle connect, spawn, update, despawn events
        }

        for _tick in client.take_tick_events().drain() {
            // simulation step
        }

        client.send_all_packets(&mut world);

        // macroquad rendering
        clear_background(BLACK);
        // draw_* calls
        next_frame().await;
    }
}
```

See `demos/macroquad/` for a complete working example.
