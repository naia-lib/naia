# Running the Demos

naia ships several working demos in the `demos/` directory. Each demo
demonstrates a different integration pattern.

---

## Basic demo (core, no ECS)

The simplest demo — server and client using `naia-server` / `naia-client`
directly, no ECS framework.

```sh
# Server
cargo run -p naia-demo-basic-server

# Client (native)
cargo run -p naia-demo-basic-client

# Client (browser)
cd demos/basic/client/wasm_bindgen
wasm-pack build --target web
```

## Bevy demo (entity replication + prediction)

The flagship demo: entity replication, authority delegation, and client-side
prediction all working together.

```sh
# Server
cargo run -p naia-demo-bevy-server

# Client
cargo run -p naia-demo-bevy-client
```

## macroquad demo

A lightweight alternative to Bevy using the macroquad game framework.

```sh
# Server
cargo run -p naia-demo-macroquad-server

# Client
cargo run -p naia-demo-macroquad-client
```

## Socket demo

Demonstrates the raw `transport_webrtc` socket layer without the
higher-level Server / Client APIs.

```sh
# Server
cargo run -p naia-demo-socket-server

# Client (browser)
cd demos/socket/client/wasm_bindgen
wasm-pack build --target web
```

---

> **Tip:** Start with the **basic** demo to understand the five-step server/client loop,
> then move to the **Bevy** demo to see entity replication and prediction
> working together.
