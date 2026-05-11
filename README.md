[![Latest Version](https://img.shields.io/crates/v/naia-server.svg)](https://crates.io/crates/naia-server)
[![API Documentation](https://docs.rs/naia-server/badge.svg)](https://docs.rs/naia-server)
[![Discord chat](https://img.shields.io/discord/764975354913619988.svg?label=discord%20chat)](https://discord.gg/fD6QCtX)
[![MIT/Apache][s3]][l3]

[s3]: https://img.shields.io/badge/license-MIT%2FApache-blue.svg
[l3]: docs/LICENSE-MIT

# naia

Server-authoritative entity replication and typed message passing for
multiplayer games in Rust, running on native (UDP) and browser (WebRTC).

---

## What naia is

naia lets you define a shared `Protocol` — a compile-time list of replicated
component types, message types, and channel configurations — that both the
server and the client agree on. Given that protocol:

- The **server** spawns entities, attaches replicated components, assigns users
  to rooms, and calls `send_all_packets` every tick. naia diffs changed fields
  and delivers them to every in-scope client automatically.
- The **client** receives entity spawn/update/despawn events and the current
  server-side field values with no extra bookkeeping.
- Either side can send typed messages over ordered-reliable, unordered-reliable,
  or unreliable channels.
- The server can **delegate authority** over a specific entity to a client,
  allowing client mutations to flow back to the server while the server retains
  final ownership.

naia is ECS-agnostic. Bevy and macroquad adapters are included; the core crate
works with any entity type that is `Copy + Eq + Hash + Send + Sync`.

The internal networking model follows the
[Tribes 2 Networking Model](https://www.gamedevs.org/uploads/tribes-networking-model.pdf).

---

## Crate map

| Crate | Role | Add as a dependency when… |
|-------|------|--------------------------|
| `naia-shared` | Protocol definition, component derives, channel types | You are writing the shared protocol crate |
| `naia-server` | Core server | Writing a server without Bevy |
| `naia-client` | Core client | Writing a client without Bevy or macroquad |
| `naia-bevy-server` | Bevy server adapter | Using Bevy on the server |
| `naia-bevy-client` | Bevy client adapter | Using Bevy on the client |

---

## Quick concepts

- **Protocol** — the shared type registry. Both server and client build from
  the same `Protocol` value; a hash mismatch during the handshake causes
  rejection.
- **Entity** — any `Copy + Eq + Hash` value your world allocates. naia tracks
  which entities are replicated and to whom, but never allocates them itself.
- **Room** — a coarse membership group. A user and an entity must share a room
  before replication is possible. Think: match, zone, lobby.
- **Channel** — a named transport lane with configurable ordering and
  reliability. Messages and entity actions travel through channels.
- **Tick** — the server's heartbeat. `take_tick_events` advances the tick
  counter. `TickBuffered` channels deliver client input at the correct server
  tick for prediction and rollback.
- **Authority delegation** — a server entity can be marked `Delegated`,
  allowing a client to request write authority. The server grants or denies
  and can revoke at any time.

For the full mental-model guide, see [docs/CONCEPTS.md](docs/CONCEPTS.md).

---

## Getting started

### Core (no ECS)

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

See [demos/basic/](demos/basic/) for a minimal working example.

### Bevy adapter

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

See [demos/bevy/](demos/bevy/) for a complete Bevy demo.

### macroquad adapter

```toml
# client/Cargo.toml
[dependencies]
naia-macroquad-client = "0.24"
naia-shared = { path = "../shared" }
```

See [demos/macroquad/](demos/macroquad/) for a macroquad demo.

---

## Channel reference

| Mode | Ordering | Reliability | Canonical use |
|------|----------|-------------|---------------|
| `UnorderedUnreliable` | None | None | High-frequency telemetry |
| `SequencedUnreliable` | Newest-wins | None | Position updates (stale ok) |
| `UnorderedReliable` | None | Guaranteed | One-off notifications |
| `OrderedReliable` | FIFO | Guaranteed | Chat, game events |
| `TickBuffered` | Per-tick | Guaranteed | Client input for prediction |
| Bidirectional + Reliable | FIFO | Guaranteed | Request / response pairs |

---

## Platform support

| Target | Transport | Notes |
|--------|-----------|-------|
| Linux / macOS / Windows | UDP | `naia-socket-native` |
| Browser (`wasm32-unknown-unknown`) | WebRTC data channel | Enable `wbindgen` feature on socket crate; build with `wasm-pack` or `trunk` |

The server always runs natively. Only the client needs WebRTC support for
browser targets.

---

## Coming from another library?

| You know… | naia equivalent | Key difference |
|-----------|----------------|----------------|
| **renet** `NetworkedEntity` / RenetServer | `Server<E>` + `#[derive(Replicate)]` | naia replicates ECS state automatically (diff + send); renet is message-only |
| **renet** `send_message` / `receive_message` | `server.send_message::<Ch, M>()` | naia wraps channels as typed Rust generics |
| **renet** `ClientId` | `UserKey` | Same concept — opaque handle to a connected client |
| **lightyear** `Replicate` component | `#[derive(Replicate)]` on a struct | naia is ECS-agnostic; lightyear is Bevy-only |
| **lightyear** `ComponentRegistry` | `Protocol::add_component::<C>()` | Same idea; naia uses a builder pattern |
| **lightyear** `InputChannel` / predicted input | `TickBuffered` channel + `receive_tick_buffer_messages` | naia delivers client input at the matching server tick for rollback |
| **lightyear** `Predicted` / `Interpolated` | not built-in | naia supplies the data; you write the prediction/interpolation logic ([see PREDICTION.md](docs/PREDICTION.md)) |
| **bevy_replicon** `Replication` marker | `server.spawn_entity()` + `#[derive(Replicate)]` | naia has fine-grained per-user scope control via rooms + `UserScopeMut` |
| **bevy_replicon** visibility filter | `server.user_scope_mut(&user)` / rooms | naia: rooms = coarse; user-scope = fine-grained |

---

## Links

- [API docs (docs.rs)](https://docs.rs/naia-server)
- [Concepts guide](docs/CONCEPTS.md)
- [Migration guide](docs/MIGRATION.md)
- [Changelog](CHANGELOG.md)
- [Discord](https://discord.gg/fD6QCtX)
- [Demos](demos/)
