# Frequently Asked Questions

## Table of contents

<!-- TOC -->
  * [General questions](#general-questions)
    * [What is the difference between the different crates?](#what-is-the-difference-between-the-different-crates)
    * [Is naia compatible with other transport layers?](#is-naia-compatible-with-other-transport-layers)
    * [What game networking concepts does naia provide?](#what-game-networking-concepts-does-naia-provide)
  * [ECS replication system questions](#ecs-replication-system-questions)
    * [How can I have different replication frequencies per entity?](#how-can-i-have-different-replication-frequencies-per-entity)
    * [Does naia provide lag compensation?](#does-naia-provide-lag-compensation)
    * [What is the tick for?](#what-is-the-tick-for)
    * [What is the difference between `duplicate_entity` and `mirror_entity`?](#what-is-the-difference-between-duplicate_entity-and-mirror_entity)
    * [How can I know the type of entity that I am replicating?](#how-can-i-know-the-type-of-entity-that-i-am-replicating)
  * [Message/Event passing](#messageevent-passing)
    * [Can any message be passed through a channel?](#can-any-message-be-passed-through-a-channel)
    * [What is `Property<>`?](#what-is-property)
<!-- TOC -->

---

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
  extension methods for entity replication. The core crate does all the work.
- **`naia-macroquad-client`** — equivalent adapter for macroquad (client only).

### Is naia compatible with other transport layers?

Yes. naia's transport layer is defined by a pluggable `Socket` trait. Two
implementations ship out of the box:

| Socket | Target | Encryption |
|--------|--------|-----------|
| `NativeSocket` (`transport_udp`) | Linux / macOS / Windows | None (dev / trusted LAN) |
| `WebrtcSocket` (`transport_webrtc`) | Browser (`wasm32-unknown-unknown`) | DTLS via WebRTC |

A third implementation (`transport_local`) is used by the test harness for
in-process testing without a real UDP socket. You can implement the `Socket`
trait yourself to plug in any other transport — the server and client APIs
are identical regardless of which socket they receive.

> **Security note:** `transport_udp` is plaintext. For native production
> deployments on untrusted networks, place the server behind a TLS terminating
> proxy (e.g. stunnel, NGINX stream proxy) until `transport_quic` (TLS 1.3) is
> available. See `SECURITY.md` for details.

### What game networking concepts does naia provide?

**naia provides:**

- Client/Server networking that compiles to both native and
  `wasm32-unknown-unknown` (browser via WebRTC).
- Typed message passing over configurable channels (unreliable, sequenced
  unreliable, ordered reliable, unordered reliable, tick-buffered for
  prediction).
- Efficient serialization: bit-packing, per-field delta compression, quantized
  numeric types, and optional zstd packet compression with custom dictionary
  training.
- ECS world replication from server → client with fine-grained per-user
  interest management (rooms + `UserScope`).
- Authority delegation: the server can grant a client write authority over a
  specific entity and revoke it at any time.
- **Client-side prediction primitives.** `TickBuffered` channels deliver
  client input at the correct server tick; `CommandHistory` stores input for
  rollback replay; `local_duplicate()` creates a predicted entity copy. The
  application assembles the prediction loop using these building blocks. See
  `docs/PREDICTION.md` for a full walkthrough.
- **Lag compensation** via the `Historian`. See
  [Does naia provide lag compensation?](#does-naia-provide-lag-compensation)
  below.
- Per-entity priority and bandwidth allocation. See
  [How can I have different replication frequencies per entity?](#how-can-i-have-different-replication-frequencies-per-entity)

**naia does not provide:**

- A built-in snapshot interpolation framework. The Bevy and macroquad demos
  implement interpolation via an `Interp` component — the pattern is
  demonstrated, not built in.
- Spatial / automatic interest management. The `scope_checks_pending()` hook
  gives you the set of entities that may need scope re-evaluation; what you do
  with it (distance check, frustum test, etc.) is application logic.
- P2P / NAT hole-punching. naia is server-authoritative by design.

---

## ECS replication system questions

### How can I have different replication frequencies per entity?

Use the priority accumulator API. Every entity has a **gain** value (default
`1.0`) that controls how fast its priority accumulates relative to other
entities. Higher gain → more bandwidth allocated → higher effective replication
rate.

```rust
// On the server, after spawning an entity:

// Give this entity 2× the bandwidth of a normal entity.
server.global_entity_priority_mut(entity).set_gain(2.0);

// Give this entity 25% of normal bandwidth (replicated ~4× less often).
server.global_entity_priority_mut(entity).set_gain(0.25);

// Per-user priority: this entity replicates faster to user A than to user B.
server.user_entity_priority_mut(&user_a, entity).set_gain(3.0);
server.user_entity_priority_mut(&user_b, entity).set_gain(0.5);
```

naia's send loop sorts all dirty entities by their accumulated priority each
tick and drains them against a per-connection bandwidth budget
(`BandwidthConfig::target_bytes_per_sec`, default 512 kbps). Entities with
higher gain accumulate priority faster and therefore tend to send more
frequently. Combined with the bandwidth budget, this replaces a separate
per-entity "tick rate" with a continuous priority-weighted allocation.

A gain of `0.0` prevents the entity from ever being selected by the send loop
— effectively pausing replication for that entity without removing it from
scope.

### Does naia provide lag compensation?

Yes, via the **`Historian`**. The `Historian` keeps a rolling buffer of
per-tick world snapshots on the server (up to a configurable number of ticks
back). When a client fires a weapon it sends the client tick at which it fired;
the server can look up the world state at that tick to perform hit detection
against where enemies actually were from the client's perspective, rather than
where they are now.

```rust
// Server startup — opt in and set the buffer depth.
server.enable_historian(64); // keep 64 ticks of history

// Inside the server tick handler, after mutating game state:
server.record_historian_tick(&world, current_tick);

// When processing a fire command from a client:
if let Some(historian) = server.historian() {
    if let Some(snapshot) = historian.snapshot_at_tick(fire_tick) {
        // snapshot: HashMap<GlobalEntity, EntitySnapshot>
        // EntitySnapshot: HashMap<ComponentKind, Box<dyn Replicate>>
        // Iterate, read positions, run your hit-detection logic.
    }
}
```

Notes:
- Components are cloned at snapshot time via `Replicate::copy_to_box()`.
  Snapshots are cheap for small structs (position, health) and heavier for
  large component sets.
- The buffer auto-evicts snapshots older than `max_ticks`.
- Call `record_historian_tick` **after** mutating game state but **before**
  `send_all_packets` so each snapshot reflects the state clients will see.

### What is the tick for?

**On the server:** the tick is the simulation heartbeat. `take_tick_events`
advances the tick counter; each elapsed tick produces a `TickEvent` that drives
your game simulation step. `send_all_packets` is typically called once per
tick, batching all component diffs and messages accumulated that tick into a
single outbound packet per client.

**On the client:** the client maintains two tick streams — a *client tick*
(running slightly ahead of the server by ~RTT/2 so that inputs arrive at the
server on time) and a *server tick* (the server tick that is currently arriving
at the client). Use `client_interpolation()` and `server_interpolation()` to
get the sub-tick interpolation fraction `[0.0, 1.0)` for smooth rendering
between ticks.

`TickBuffered` channels stamp every message with the *client tick* at which
the input occurred. The server delivers them via
`receive_tick_buffer_messages(&server_tick)` at the matching tick, enabling
tick-accurate input replay for client-side prediction and rollback. See
`docs/PREDICTION.md`.

### What is the difference between `duplicate_entity` and `mirror_entity`?

- **`local_duplicate()`** (Bevy: `commands.entity(e).local_duplicate()`) —
  spawns a brand-new client-local entity and copies all `Replicate` components
  from `e` into it. Use this to create the *predicted* counterpart of a
  server-replicated entity at the moment the server assigns it to the local
  player.

- **`mirror_entity`** — copies component values from a source entity into an
  already-existing target entity (for components they share). Use this when you
  want to sync two existing entities rather than spawn a new one.

In the prediction pattern the typical flow is:
1. Receive `EntityAssignment` — the server tells the client which entity is
   the local player.
2. Call `local_duplicate()` on the confirmed entity to create the predicted
   entity that runs ahead of the server.
3. On each correction, call `mirror` (or snap fields manually) to resync the
   predicted entity to the server's authoritative state before replaying
   buffered commands.

### How can I know the type of entity that I am replicating?

A common need: the server replicates several entity types (players, items,
enemies) and the client wants to insert extra components based on type (e.g.
the correct texture or AI marker).

Two approaches:

1. **Marker component.** Add a zero-sized `#[derive(Replicate)]` marker
   component (e.g. `PlayerMarker`, `ItemMarker`) to each entity. When the
   `InsertComponentEvent` for the marker fires on the client, insert the
   additional local components you need.

2. **Typed messages.** Instead of — or in addition to — ECS replication, send
   an `OrderedReliable` message (e.g. `PlayerSpawnedMsg { entity, position }`,
   `ItemSpawnedMsg { entity, kind }`) on spawn. The client handles each message
   type separately and inserts the appropriate components.

---

## Message/Event passing

### Can any message be passed through a channel?

Any struct that derives `Message` can be sent through a message channel:

```rust
#[derive(Message)]
pub struct ChatMessage {
    pub text: String,
    pub sender: u32,
}
```

`Message` types do **not** use `Property<>` wrappers — that wrapper is only
for `Replicate` components that participate in per-field delta tracking.
Message fields are serialized in full each time the message is sent.

For the message to be sendable, its field types must implement `Serde` (naia's
serialization trait). Primitive types, `String`, arrays, and tuples of `Serde`
types all work out of the box. Use naia's quantized numeric types
(`UnsignedInteger<N>`, `SignedVariableFloat<BITS, FRAC>`, etc.) to reduce wire
size for numeric fields.

Register the message type in your `Protocol` builder:

```rust
Protocol::builder()
    .add_message::<ChatMessage>()
    // …
    .build()
```

Then send and receive:

```rust
// Server → specific client:
server.send_message::<GameChannel, ChatMessage>(&user_key, &msg)?;

// Client → server:
client.send_message::<GameChannel, ChatMessage>(&msg);

// Client receive:
for event in events.read::<MessageEvent<GameChannel, ChatMessage>>() {
    println!("{}", *event.message.text);
}
```

### What is `Property<>`?

`Property<T>` is a change-detection wrapper used exclusively inside
`#[derive(Replicate)]` component structs. When any `Property<T>` field is
mutated the containing component is marked dirty, and naia includes only the
changed fields in the next `send_all_packets` call (per-field delta
compression). This means a component with ten fields only sends the one or two
fields that actually changed each tick — a significant bandwidth saving.

```rust
#[derive(Replicate)]
pub struct Position {
    pub x: Property<f32>,
    pub y: Property<f32>,
}

// Mutating through DerefMut marks the component dirty:
position.x.set(42.0);
```

`Property<T>` is **not** needed in `Message` types. Messages are serialized in
full every time they are sent and have no change-detection mechanism.
