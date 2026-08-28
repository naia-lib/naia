# Macroquad

Macroquad clients use the core `naia-client` crate directly. There is no
separate macroquad adapter crate.

The macroquad demo in `demos/macroquad/` is the canonical reference: it pairs a
native `naia-server` with a macroquad/miniquad client and a shared
`naia-shared` protocol crate.

---

## Cargo Setup

```toml
# shared/Cargo.toml
[dependencies]
naia-shared = { version = "0.25", features = ["mquad"] }

# client/Cargo.toml
[dependencies]
naia-client = { version = "0.25", features = ["mquad", "transport_webrtc"] }
macroquad = "0.3"
my-game-shared = { path = "../shared" }

# server/Cargo.toml
[dependencies]
naia-server = { version = "0.25", features = ["transport_webrtc"] }
my-game-shared = { path = "../shared" }
```

---

## Loop Shape

In macroquad, naia's client loop lives inside your frame loop:

```rust
#[macroquad::main("My Game")]
async fn main() {
    let mut client = Client::new(ClientConfig::default(), protocol());
    let socket = naia_client::transport::webrtc::Socket::new(
        "http://127.0.0.1:14191",
        client.socket_config(),
    );
    client.connect(socket);

    loop {
        client.receive_all_packets();
        client.process_all_packets(&mut world);

        // Read connection/entity/component/message events.
        // Mutate your local world and render with macroquad.

        client.send_all_packets(&mut world);
        next_frame().await;
    }
}
```

The exact world implementation is up to your game. The demo uses
`naia-demo-world`, a small world wrapper that implements the core world traits.
For a production game, you can keep that shape or implement the traits for your
own storage.
