# Installation

## Core (no ECS)

```toml
# shared/Cargo.toml
[dependencies]
naia-shared = "0.24"

# server/Cargo.toml
[dependencies]
naia-server = "0.24"
naia-shared = { path = "../shared" }

# client/Cargo.toml
[dependencies]
naia-client = "0.24"
naia-shared = { path = "../shared" }
```

## Bevy adapter

```toml
# shared/Cargo.toml
[dependencies]
naia-shared = "0.24"

# server/Cargo.toml
[dependencies]
naia-bevy-server = "0.24"
naia-shared = { path = "../shared" }

# client/Cargo.toml
[dependencies]
naia-bevy-client = "0.24"
naia-shared = { path = "../shared" }
```

## macroquad adapter

```toml
# client/Cargo.toml
[dependencies]
naia-macroquad-client = "0.24"
naia-shared = { path = "../shared" }
```

---

## Browser (WASM) target

For browser clients, add the `wbindgen` feature and build with `wasm-pack` or `trunk`:

```toml
# client/Cargo.toml
[dependencies]
naia-client = { version = "0.24", features = ["wbindgen"] }
```

Install the WASM target if you haven't already:

```sh
rustup target add wasm32-unknown-unknown
```

Build with trunk:

```sh
trunk build --release
```

---

## Feature flags

| Flag | Effect |
|------|--------|
| `wbindgen` | Enable WebRTC transport for browser targets |
| `metrics` | Enable `naia-metrics` integration |

> **Note:** The server always runs natively. Only the client needs the `wbindgen` feature
> for browser targets. Your server/client can share the same `shared` crate
> without any feature-flag divergence.
