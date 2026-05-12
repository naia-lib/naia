# Installation

naia follows a three-crate workspace pattern: a `shared` crate imported by both
the server and client, a `server` binary, and a `client` binary.

---

## Bevy (recommended)

```toml
# shared/Cargo.toml
[dependencies]
naia-shared = "0.24"

# server/Cargo.toml
[dependencies]
naia-bevy-server = "0.24"
naia-shared       = { path = "../shared" }

# client/Cargo.toml
[dependencies]
naia-bevy-client = "0.24"
naia-shared      = { path = "../shared" }
```

See [Bevy Quick Start](bevy-quickstart.md) for a complete working example.

---

## Core (no ECS)

For macroquad or custom engines, use the core crates directly:

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

See [Core API Overview](../adapters/overview.md) for the five-step loop.

---

## macroquad adapter

```toml
# client/Cargo.toml
[dependencies]
naia-macroquad-client = "0.24"
naia-shared           = { path = "../shared" }
```

---

## Browser (WASM) target

For browser clients, enable the `wbindgen` feature and build with `wasm-pack`
or `trunk`:

```toml
# client/Cargo.toml — WASM build
[dependencies]
naia-client = { version = "0.24", features = ["wbindgen"] }
naia-shared = { path = "../shared" }
```

Install the WASM target if you haven't already:

```sh
rustup target add wasm32-unknown-unknown
```

Build with trunk:

```sh
trunk build --release
```

Or with wasm-pack:

```sh
wasm-pack build --target web
```

> **Note:** The server always runs natively. Only the client needs the `wbindgen`
> feature for browser targets. The shared crate requires no feature-flag changes.

---

## Feature flags

| Crate | Flag | Effect |
|-------|------|--------|
| `naia-client` | `wbindgen` | Enable WebRTC transport for `wasm32-unknown-unknown` targets |
| `naia-bevy-client` | `wbindgen` | Same, for the Bevy adapter |
| `naia-server` | `metrics` | Enable `naia-metrics` integration (opt-in observability) |
| `naia-bevy-server` | `metrics` | Same, for the Bevy server adapter |

---

## Workspace layout

For a real project, a minimal `Cargo.toml` workspace looks like:

```toml
# Cargo.toml (workspace root)
[workspace]
members = ["shared", "server", "client"]
resolver = "2"
```

Each member crate then has its own `Cargo.toml` as shown above.

> **Tip:** Put the `Protocol` builder, all `#[derive(Replicate)]` component types,
> all `#[derive(Message)]` types, and all `#[derive(Channel)]` types in the
> `shared` crate. Import it from both the server and client. This ensures both
> sides always build the exact same protocol hash — a mismatch causes
> handshake rejection.

---

## Rust toolchain

naia requires stable Rust. No nightly features are used. The minimum supported
Rust version (MSRV) is listed in the root `Cargo.toml` of the naia repository.
