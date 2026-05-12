# FAQ

## General questions

### What is the difference between the different crates?

- **`naia-shared`** — protocol definition, channel configuration, serialization
  primitives (bit-packing, quantized numeric types, zstd compression), and the
  `#[derive(Replicate)]` / `#[derive(Message)]` / `#[derive(Channel)]` proc-macros.
- **`naia-server`** / **`naia-client`** — the core game-networking implementation
  (entity replication, rooms, user-scope, authority delegation, tick
  synchronisation, priority-based bandwidth management). These crates are
  ECS-agnostic; they work with any entity type that is `Copy + Eq + Hash`.
- **`naia-bevy-server`** / **`naia-bevy-client`** — thin Bevy adapter layers.
  They wrap the core crates and expose `Server` / `Client` as Bevy resources,
  route naia events into Bevy's event system, and provide `CommandsExt`
  extension methods for entity replication.
- **`naia-macroquad-client`** — equivalent adapter for macroquad (client only).

---

### Is naia compatible with other transport layers?

Yes. naia's transport layer is defined by a pluggable `Socket` trait. Two
implementations ship out of the box:

| Socket | Target | Encryption |
|--------|--------|-----------|
| `NativeSocket` (`transport_udp`) | Linux / macOS / Windows | None (dev / trusted LAN) |
| `WebrtcSocket` (`transport_webrtc`) | Browser (`wasm32-unknown-unknown`) | DTLS via WebRTC |

A third implementation (`transport_local`) is used by the test harness for
in-process testing. You can implement the `Socket` trait yourself to plug in any
other transport.

---

### What game networking concepts does naia provide?

**naia provides:**

- Native and browser client support from a single codebase.
- Typed message passing over configurable channels.
- Efficient serialization: bit-packing, per-field delta compression, quantized
  numeric types, optional zstd packet compression with custom dictionary training.
- ECS world replication with fine-grained per-user interest management.
- Authority delegation: server grants/revokes client write authority over entities.
- Client-side prediction primitives: `TickBuffered` channels, `CommandHistory`,
  `local_duplicate()`.
- Lag compensation via the `Historian`.
- Per-entity priority and bandwidth allocation.

**naia does not provide:**

- A built-in snapshot interpolation framework.
- Spatial / automatic interest management.
- P2P / NAT hole-punching.

---

## ECS replication system questions

### How can I have different replication frequencies per entity?

Use the priority accumulator API. See [Priority-Weighted Bandwidth](../advanced/bandwidth.md).

---

### Does naia provide lag compensation?

Yes, via the **`Historian`**. See [Lag Compensation with Historian](../advanced/historian.md).

---

### What is the tick for?

**On the server:** the tick is the simulation heartbeat. `take_tick_events`
advances the tick counter; each elapsed tick produces a `TickEvent` that drives
your game simulation step.

**On the client:** the client maintains two tick streams — a *client tick*
(running slightly ahead of the server by ~RTT/2) and a *server tick* (the server
tick currently arriving at the client). Use `client_interpolation()` and
`server_interpolation()` to get the sub-tick interpolation fraction `[0.0, 1.0)`
for smooth rendering.

See [Tick Synchronization](../concepts/ticks.md) for details.

---

### What is the difference between `local_duplicate()` and `mirror_entity`?

- **`local_duplicate()`** — spawns a brand-new client-local entity and copies all
  `Replicate` components from the source entity. Use this to create the *predicted*
  counterpart of a server-replicated entity at the moment the server assigns it.

- **`mirror_entity`** — copies component values from a source entity into an
  already-existing target entity. Use this to sync two existing entities rather
  than spawning a new one.

In the prediction pattern the typical flow is:
1. Receive `EntityAssignment` — the server tells the client which entity is the local player.
2. Call `local_duplicate()` on the confirmed entity to create the predicted entity.
3. On each correction, call `mirror` to resync the predicted entity to the server's
   authoritative state before replaying buffered commands.

---

### How can I know the type of entity that I am replicating?

Two approaches:

1. **Marker component.** Add a zero-sized `#[derive(Replicate)]` marker component
   (e.g. `PlayerMarker`, `ItemMarker`) to each entity. When the
   `InsertComponentEvent` for the marker fires on the client, insert the additional
   local components you need.

2. **Typed messages.** Send an `OrderedReliable` message on spawn. The client
   handles each message type separately and inserts the appropriate components.

---

### What is `DefaultClientTag`?

`DefaultClientTag` and `DefaultPlugin` are provided by `naia-bevy-client` to
reduce boilerplate for single-client Bevy applications. Instead of defining your
own phantom marker type, you can write:

```rust
use naia_bevy_client::{DefaultPlugin, Client, DefaultClientTag};

app.add_plugins(DefaultPlugin::new(client_config, protocol()));

fn my_system(client: Client<DefaultClientTag>) { /* … */ }
```

If you have two simultaneous naia clients in the same Bevy app (split-screen,
relay node), you must define separate marker types — `DefaultClientTag` is only
for single-client setups. See [Bevy Adapter — Multi-client setup](../adapters/bevy.md#multi-client-setup).

---

## Message/Event passing

### Can any message be passed through a channel?

Any struct that derives `Message` can be sent through a message channel. Field
types must implement `Serde`. Primitive types, `String`, arrays, and tuples of
`Serde` types all work out of the box. See [Messages & Channels](../concepts/messages.md).

---

### What is `Property<>`?

`Property<T>` is a change-detection wrapper used exclusively inside
`#[derive(Replicate)]` component structs. When any `Property<T>` field is
mutated, the containing component is marked dirty, and naia includes only the
changed fields in the next `send_all_packets` call.

`Property<T>` is **not** needed in `Message` types — messages are serialized in
full every time they are sent.

See [Delta Compression](../advanced/delta-compression.md) for details.
