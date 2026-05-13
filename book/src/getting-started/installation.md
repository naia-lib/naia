# Installation

Most naia applications use a three-crate workspace: a shared crate imported by
both sides, a server binary, and a client binary. The shared crate is where the
`Protocol`, replicated components, messages, channels, and request/response
types live.

---

## Bevy Projects

For Bevy, use the Bevy adapter crates in all three crates:

```toml
# shared/Cargo.toml
[dependencies]
naia-bevy-shared = "0.25"
bevy_ecs = { version = "0.18", default-features = false }

# server/Cargo.toml
[dependencies]
naia-bevy-server = { version = "0.25", features = ["transport_webrtc"] }
my-game-shared = { path = "../shared" }

# client/Cargo.toml
[dependencies]
naia-bevy-client = { version = "0.25", features = ["transport_webrtc"] }
my-game-shared = { path = "../shared" }
```

`naia-bevy-server` and `naia-bevy-client` re-export the shared primitives most
application code needs. Your shared crate should depend on `naia-bevy-shared`
because Bevy replicated components derive both `Component` and `Replicate`.

See [Bevy Quick Start](bevy-quickstart.md) for a complete working example.

---

## Without Bevy

For macroquad or a custom engine, use the core crates directly:

```toml
# shared/Cargo.toml
[dependencies]
naia-shared = "0.25"

# server/Cargo.toml
[dependencies]
naia-server = { version = "0.25", features = ["transport_webrtc"] }
my-game-shared = { path = "../shared" }

# client/Cargo.toml
[dependencies]
naia-client = { version = "0.25", features = ["transport_webrtc"] }
my-game-shared = { path = "../shared" }
```

There is no separate macroquad adapter crate. Macroquad clients use `naia-client`
directly and enable the `mquad` feature when building through miniquad/macroquad:

```toml
naia-client = { version = "0.25", features = ["mquad", "transport_webrtc"] }
naia-shared = { version = "0.25", features = ["mquad"] }
```

See [Core API Overview](../adapters/overview.md) and [Macroquad](../adapters/macroquad.md)
for the non-Bevy path.

---

## Browser Clients

The server still runs natively. Browser clients compile to
`wasm32-unknown-unknown` and use the same `transport_webrtc` protocol path as
native WebRTC clients.

For a core client wrapper crate:

```toml
[features]
wbindgen = ["naia-client/wbindgen", "my-game-shared/wbindgen"]

[dependencies]
naia-client = { version = "0.25", features = ["transport_webrtc"] }
my-game-shared = { path = "../shared" }
```

For a Bevy client, `naia-bevy-client` already enables the underlying wasm-bindgen
support it needs; the important transport feature is still `transport_webrtc`.
Add the Wasm target if you have not already:

```sh
rustup target add wasm32-unknown-unknown
```

Build with whichever frontend tool your app uses:

```sh
trunk build --release
wasm-pack build --target web
```

---

## Transport Features

| Crate | Feature | Use when |
|-------|---------|----------|
| `naia-server`, `naia-client` | `transport_webrtc` | Preferred native + browser transport; DTLS via WebRTC |
| `naia-bevy-server`, `naia-bevy-client` | `transport_webrtc` | Same transport through the Bevy adapter |
| `naia-server`, `naia-client` | `transport_udp` | Native plaintext UDP for local dev/trusted networks |
| `naia-server`, `naia-client` | `transport_local` | In-process tests and deterministic harnesses |
| `naia-client`, `naia-shared` | `wbindgen` | Core-client wrapper crates targeting wasm-bindgen |
| `naia-client`, `naia-shared` | `mquad` | miniquad/macroquad builds |

---

## Workspace Layout

```toml
# Cargo.toml (workspace root)
[workspace]
members = ["shared", "server", "client"]
resolver = "2"
```

Keep protocol construction and all registered replicated/message/channel types
in the shared crate. Both sides must build the exact same `Protocol`; a mismatch
rejects the handshake, which is correct behavior and also a very efficient way
to discover that one side forgot to enable a feature flag.

---

## Rust Toolchain

naia uses stable Rust. No nightly features are required.
