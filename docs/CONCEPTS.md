# Naia Concepts Guide

Audience: a Rust game developer who has read the README and is about to write
their first multiplayer game with naia.

---

## 1. The Shared Protocol

Both the server and the client must agree on the complete set of replicable
component types, message types, channel configurations, and protocol-level
settings. In naia this agreement is expressed as a `Protocol` value.

Conventionally you put `Protocol` construction in a shared crate:

```rust
use naia_shared::{Protocol, ChannelMode, ChannelDirection};

pub fn protocol() -> Protocol {
    Protocol::builder()
        .tick_interval(std::time::Duration::from_millis(40)) // 25 Hz
        .add_component::<Position>()
        .add_component::<Health>()
        .add_message::<ChatMessage>()
        .add_channel::<GameChannel>(
            ChannelDirection::Bidirectional,
            ChannelMode::OrderedReliable(Default::default()),
        )
        .build()
}
```

Both the server and the client call this same function and pass the result to
`Server::new` / `Client::new`. naia derives a deterministic `ProtocolId` from
the registered types and channel configuration; a client whose ID does not
match the server's will be rejected during the handshake.

**The shared crate** typically contains:
- `Protocol` construction
- All `#[derive(Replicate)]` component types
- All `#[derive(Message)]` / `#[derive(Request, Response)]` types
- Custom `#[derive(Channel)]` marker types

---

## 2. Entities and Components

naia is ECS-agnostic. An entity is any value that satisfies
`Copy + Eq + Hash + Send + Sync` — for instance a `u32`, a `bevy::Entity`, or
a custom newtype. naia never allocates entities itself; the `WorldMutType<E>`
you pass to `spawn_entity` does.

**Replicated components** must derive `Replicate`:

```rust
#[derive(Replicate)]
pub struct Position {
    pub x: Property<f32>,
    pub y: Property<f32>,
}
```

`Property<T>` is naia's change-detection wrapper. When a field inside
`Property<T>` is mutated the containing entity is marked dirty and the
diff is queued for transmission on the next `send_all_packets` call. Only
changed fields are sent — naia tracks per-field diffs for each in-scope user.

---

## 3. The Replication Loop

Every frame the server must execute these five steps in order:

```text
receive_all_packets     – read UDP/WebRTC datagrams from the OS
process_all_packets     – decode packets; apply client mutations
take_world_events       – drain connect/disconnect/spawn/update/message events
take_tick_events        – advance the tick clock; collect elapsed tick events
                          (mutate replicated components here)
send_all_packets        – serialise diffs + messages; flush to network
```

**Why this order is mandatory:**

- `receive_all_packets` fills the internal receive queue; nothing downstream
  can run until bytes are available.
- `process_all_packets` consumes that queue and converts bytes into
  `EntityEvent` objects that `take_world_events` later drains.
- `take_world_events` must come after `process_all_packets` so that events
  produced by the latest batch of packets are visible this frame.
- `take_tick_events` must come after `take_world_events` to avoid ordering
  anomalies between world-state events and tick-boundary events.
- `send_all_packets` must come last so that all mutations made during the
  current frame are included in the outbound batch.

The same five-step contract applies to the client, with the difference that
the client processes packets from a single server connection rather than from
many users.

---

## 4. Rooms and Scope

Entity replication uses a two-level scoping model.

### Room membership (coarse)

A user and an entity must share at least one room before replication is
possible. This is the broad spatial or logical partition — a game "zone", a
match instance, a lobby.

```rust
let room = server.create_room();
let room_key = room.key();
// …
server.room_mut(&room_key).add_user(&user_key);
server.room_mut(&room_key).add_entity(&entity);
```

### UserScope (fine-grained)

Within a shared room you can further restrict which entities replicate to
which users. The canonical pattern is a visibility callback:

```rust
// In your game loop — call scope_checks_pending() for the incremental set,
// or scope_checks_all() for a full re-evaluation:
for (room_key, user_key, entity) in server.scope_checks_pending() {
    let mut scope = server.user_scope_mut(&user_key);
    if is_visible(entity, user_key) {
        scope.include(&entity);
    } else {
        scope.exclude(&entity);
    }
}
server.mark_scope_checks_pending_handled();
```

The basic demo ships the `x ∈ [5, 15]` example: only entities whose `x`
coordinate is between 5 and 15 are included in a user's scope. Entities
outside that window are either despawned on the client (`ScopeExit::Despawn`,
the default) or frozen in place (`ScopeExit::Persist`).

---

## 5. Channels

All messages and entity actions are routed through typed channels. A channel
is a named type that derives `Channel` and is registered in the `Protocol`
with a `ChannelMode` and `ChannelDirection`.

### Built-in channel modes

| Mode | Ordering | Reliability | Typical use |
|------|----------|-------------|-------------|
| `UnorderedUnreliable` | None | None | Fire-and-forget telemetry |
| `SequencedUnreliable` | Newest-wins | None | Position updates (drop stale) |
| `UnorderedReliable` | None | Guaranteed | One-off notifications |
| `OrderedReliable` | FIFO | Guaranteed | Chat, game events |
| `TickBuffered` | Per tick | Guaranteed | Client input (tick-stamped) |
| Bidirectional + Reliable | FIFO | Guaranteed | Requests and responses |

### Custom channels

```rust
#[derive(Channel)]
pub struct PlayerInputChannel;

// In protocol builder:
.add_channel::<PlayerInputChannel>(
    ChannelDirection::ClientToServer,
    ChannelMode::TickBuffered(Default::default()),
)
```

### TickBuffered channels

`TickBuffered` stamps every message with the client tick at which the input
occurred. The server buffers them and delivers them via
`receive_tick_buffer_messages(tick)` when the server tick matches. This
enables tick-accurate input replay and is the foundation of client-side
prediction.

---

## 6. Static vs Dynamic Entities

**Dynamic entities** (the default) use per-field delta tracking. When any
`Property<T>` field changes, only the changed fields are sent to each in-scope
user on the next `send_all_packets` call.

**Static entities** skip delta tracking entirely. When a static entity enters
a user's scope, naia sends a full component snapshot. After that no further
updates are transmitted — static entities are assumed to be immutable for the
lifetime of the session.

Create a static entity via the `as_static()` builder method:

```rust
server.spawn_entity(&mut world)
    .as_static()           // must be called BEFORE insert_component
    .insert_component(tile);
```

Use static entities for map tiles, level geometry, or any entity that is
written once and never changes. They save significant CPU time by eliminating
diff tracking.

---

## 7. Replicated Resources

A **replicated resource** is a server-side singleton that is automatically
visible to all connected users, without room membership or scope management.
Internally naia creates a hidden one-component entity to carry the value.

```rust
// Insert a dynamic (diff-tracked) resource:
server.insert_resource(&mut world, ScoreBoard::new(), false)?;

// Insert a static (immutable) resource:
server.insert_resource(&mut world, MapMetadata::new(), true)?;

// Remove it later:
server.remove_resource::<ScoreBoard, _>(&mut world);
```

On the client:

```rust
if client.has_resource::<ScoreBoard>() {
    let entity = client.resource_entity::<ScoreBoard>().unwrap();
    // read component from world storage using entity
}
```

Resources differ from ordinary entities in three ways:
- No room or scope configuration is needed.
- At most one resource per type can exist at a time (inserting a duplicate
  returns `Err(ResourceAlreadyExists)`).
- They can be delegated just like entities by calling `configure_resource`.

---

## 8. Authority Delegation

By default the server owns all component state. **Delegation** allows a client
to take temporary write authority over a specific entity — while it holds
authority its mutations replicate back to the server instead of the other way
around.

### State machine

```text
Server marks entity Delegated  (ReplicationConfig::delegated())
        │
        ▼
  Status: Available   ←──────────────────────────────────────────┐
        │                                                         │
        │  client calls entity_request_authority                  │
        ▼                                                         │
  Status: Requested                                              │
        │                                                         │
        ├─── server denies ──────────────────► Status: Denied    │
        │                                              │          │
        │  server grants                               │ release  │
        ▼                                              └──────────┤
  Status: Granted   (client mutations replicate to server)       │
        │                                                         │
        │  client calls entity_release_authority                  │
        ▼                                                         │
  Status: Releasing ──────────────────────────────────────────►──┘
```

### Trust model

- The server may revoke authority at any time by calling
  `entity_take_authority`.
- The client never holds unrevocable ownership.
- Mutations from a client-authoritative entity should still be validated
  server-side before applying to authoritative game state. naia replicates
  what the client sends without validation.

### Example

```rust
// Server: mark entity as delegatable
server.spawn_entity(&mut world)
    .insert_component(position)
    .configure_replication(ReplicationConfig::delegated());

// Client: request authority (requires Granted EntityAuthStatus to mutate)
client.entity_mut(&mut world, &entity)
    .request_authority();

// Server event loop — handle grant/deny:
for (user_key, entity) in events.read::<EntityAuthGrantEvent>() {
    // The requesting client now has write authority.
}
```

---

## 9. Tick Synchronisation

### Server ticks

The server tick interval is configured in the `Protocol`:

```rust
Protocol::builder()
    .tick_interval(Duration::from_millis(50)) // 20 Hz
```

`take_tick_events` advances the tick counter. Each elapsed server tick
produces a `TickEvent` that triggers the game simulation step.

### Client ticks

The client maintains two tick streams:

- **Client tick** (`client_tick`) — the tick at which the client is sending,
  running slightly ahead of the server to account for travel time.
- **Server tick** (`server_tick`) — the server tick currently arriving at the
  client, behind the server's actual tick by RTT/2 + jitter.

Use `client_interpolation()` and `server_interpolation()` to compute the
sub-tick interpolation fraction `[0.0, 1.0)` for smooth rendering.

### Prediction and rollback

`TickBuffered` channels carry client input timestamped with the client tick.
The server delivers them via `receive_tick_buffer_messages(tick)`, enabling
rollback-and-replay: apply the server's authoritative update, then replay
buffered client inputs on top. `CommandHistory<M>` stores the input history
for this purpose.

For a complete step-by-step walkthrough of the full prediction loop, see
**[docs/PREDICTION.md](PREDICTION.md)**.

---

## 10. Bevy adapter — the client tag type `T`

When using the Bevy adapter (`naia-bevy-client`, `naia-bevy-server`), the
`Client` SystemParam and `NaiaClientPlugin` carry a generic type parameter `T`:

```rust
use naia_bevy_client::{Client, NaiaClientPlugin};

// Your marker type — a zero-sized struct, nothing more.
#[derive(Resource)]
pub struct MyClient;

app.add_plugins(NaiaClientPlugin::<MyClient>::new(client_config, protocol()));

fn my_system(client: Client<MyClient>) { … }
```

**Why does `T` exist?**

Bevy applications sometimes run more than one naia client simultaneously (for
example, a split-screen game where each half is a separate session, or a relay
node that bridges two servers). The `T` phantom marker lets Bevy distinguish
the two `Client` SystemParams at compile time — they are different types and
therefore different Bevy resources, with no runtime overhead.

**How to use it**

1. Declare a zero-sized marker struct, typically in your game crate:
   ```rust
   #[derive(Resource)]
   pub struct GameClient;
   ```
2. Pass `GameClient` as the type parameter everywhere:
   ```rust
   NaiaClientPlugin::<GameClient>::new(…)
   Client<GameClient>        // SystemParam
   NaiaClientConfig::<GameClient>::default()
   ```
3. If you only ever have one client, the marker is still required — just pick
   any unit struct. The name does not matter to naia.

**Single-client shorthand**

For single-client apps the demo crates use a local `struct Client;` or simply
alias the plugin:

```rust
type AppPlugin = NaiaClientPlugin<MyClient>;
```

`T` must satisfy `Resource` (a Bevy bound) plus `Sync + Send + 'static` (which
`Resource` already implies). A `#[derive(Resource)]` unit struct always
satisfies this.

---

## 11. Transport and Wasm

naia's transport layer is pluggable. Two implementations ship out of the box:

| Target | Implementation | Socket type | Encryption |
|--------|----------------|-------------|------------|
| Native (Linux/macOS/Windows) | UDP datagram socket | `transport_udp` | **None** — dev / trusted LAN only |
| Browser (Wasm) | WebRTC data channel | `transport_webrtc` | DTLS (WebRTC spec) |

> **Security note:** `transport_udp` sends all packets as unencrypted plaintext.
> Use it for local development and trusted private networks only. For production
> native deployments on the internet, use `transport_quic` (TLS 1.3, planned)
> or place the server behind a TLS proxy. See `SECURITY.md` for details.

The `Server` and `Client` APIs are identical for both — only the `Socket`
value passed to `listen` / `connect` differs:

```rust
// Native server:
server.listen(NativeSocket::new("0.0.0.0:14191"));

// Native client:
client.connect(NativeSocket::new("127.0.0.1:14191"));

// Browser client (wasm32-unknown-unknown):
client.connect(WebrtcSocket::new("https://myserver.example.com", 14192));
```

For Wasm builds, enable the `wbindgen` feature on the socket crate and build
with `wasm-pack` or `trunk`. The protocol, channel config, and all game logic
are identical — only the entry point and socket type change.

---

## 12. Network Condition Simulation

`LinkConditionerConfig` simulates packet loss, latency, and jitter — useful
for testing replication robustness and prediction/rollback in a local dev loop
without a real bad network.

```rust
use naia_shared::LinkConditionerConfig;

// Build a custom profile:
let lag = LinkConditionerConfig::new(
    100,   // incoming_latency ms
    25,    // incoming_jitter ms  (added or subtracted at random)
    0.02,  // incoming_loss  (2% packet drop)
);

// Or use a named preset:
let lag = LinkConditionerConfig::poor_condition();

// Apply on the server socket (conditions inbound packets from clients):
server.listen(NativeSocket::new(&addrs, Some(lag.clone())));

// Apply on the client socket (conditions inbound packets from the server):
client.connect(Socket::new(server_url, Some(lag)));
```

Named presets — all values are one-way (applied to the receiving side):

| Preset | Latency (ms) | Jitter (ms) | Loss |
|--------|-------------|-------------|------|
| `perfect_condition()` | 1 | 0 | 0% |
| `very_good_condition()` | 12 | 3 | 0.1% |
| `good_condition()` | 40 | 10 | 0.2% |
| `average_condition()` | 100 | 25 | 2% |
| `poor_condition()` | 200 | 50 | 4% |
| `very_poor_condition()` | 300 | 75 | 6% |

The conditioner applies to **incoming** packets on whichever socket you pass it
to. To simulate a bidirectional bad link, pass the same config (or different
configs for asymmetric paths) to both the server and client sockets.

The local in-process transport (`transport_local`) used in the test harness
accepts the same config via `hub.configure_link_conditioner()`, enabling
loss/latency injection without a real UDP socket.

---

## 13. Bandwidth-Optimized Properties

`Property<T>` is generic over any `T: Serde`. naia ships a set of compact
numeric types in `naia_shared` that reduce wire size compared to raw `f32`/`u32`:

| Type | Wire size | Use case |
|------|-----------|----------|
| `UnsignedInteger<N>` | exactly N bits | health (0–255 → 8 bits), flags |
| `SignedInteger<N>` | exactly N bits | relative offsets |
| `UnsignedVariableInteger<N>` | 1–N bits (varint) | counts that are usually small |
| `SignedVariableInteger<N>` | 1–N bits (varint) | deltas that are usually near zero |
| `UnsignedFloat<BITS, FRAC>` | exactly BITS bits | positive position, speed |
| `SignedFloat<BITS, FRAC>` | exactly BITS bits | signed angle, velocity axis |
| `SignedVariableFloat<BITS, FRAC>` | 1–BITS bits | per-tick deltas (often tiny) |

`BITS` is the total bit width; `FRAC` is the number of decimal digits of
precision retained.

**Example — a quantized game unit:**

```rust
use naia_shared::{Property, Replicate, SignedVariableFloat, UnsignedInteger};

// Tile position: i16 tile coords + sub-tile delta (variable-width float)
#[derive(Clone, PartialEq, Serde)]
pub struct PositionState {
    pub tile_x: i16,               // already compact at i16
    pub tile_y: i16,
    pub dx: SignedVariableFloat<14, 2>,  // 14-bit max, 2 decimal digits
    pub dy: SignedVariableFloat<14, 2>,  // encodes near-zero deltas in ~3 bits
}

#[derive(Replicate)]
pub struct Position {
    pub state: Property<PositionState>,
}
```

Wrapping multi-axis state in a single `Property<State>` means one dirty-bit
covers all axes — the whole struct is sent or nothing is, which is correct for
coupled state and avoids partial-update edge cases.

Compared to `Property<f32> × 4` (128 bits/tick), `PositionState` costs roughly
32 bits (2 × i16) + ~6–28 bits (variable delta) = **38–60 bits/tick** when
typical sub-tile movement is small — a 2–3× wire reduction.

See `benches/src/bench_protocol.rs` for working examples of `PositionQ`,
`VelocityQ`, and `RotationQ` using these types in a real benchmark scenario.

---

## 14. NAT Traversal and P2P

naia is **server-authoritative by design** — a publicly reachable server
holds all authoritative state, and clients connect to it. NAT traversal and
peer-to-peer hole-punching are intentionally out of scope.

If you need P2P networking (e.g. browser-to-browser direct connections for
a rollback-netcode fighting game), the recommended Rust/Wasm ecosystem tools
are:

- **[matchbox_socket](https://github.com/johanhelsing/matchbox)** — async WebRTC
  data-channel signaling for P2P connections in native and Wasm targets.
- **[GGRS / bevy_ggrs](https://github.com/gschup/ggrs)** — GGPO-style rollback
  netcode on top of matchbox; well-suited to deterministic simulations.

These libraries are complementary to naia: a game can use naia for
server→client replication (lobby, leaderboard, world state) and GGRS for the
fast-path P2P match simulation in parallel.

---

## 15. Multi-Server / Zone Architecture

naia is a single-process authority. One server owns all entities it replicates;
there is no built-in mechanism for multiple server instances to share state.

For games that need horizontal scaling (e.g. an open world split across
geographic zones), the standard pattern is **zone sharding at the application
layer**:

```
Zone A server (naia process)          Zone B server (naia process)
  owns entities in region A             owns entities in region B
        │                                       │
        └───── coordination service ────────────┘
                 (your code: entity hand-off,
                  cross-zone messages, matchmaking)
```

Each zone server runs an independent naia instance. When a player moves between
zones the application:

1. Serializes the player's replicated state (your `Replicate` components) on
   the source server.
2. Sends the serialized state to the destination server via your coordination
   channel (Redis, gRPC, direct TCP — your choice).
3. Despawns the entity on the source server (client gets a despawn event).
4. Spawns the entity on the destination server and places the player's
   connection in the new room.

naia provides the per-process primitive (`spawn_entity`, rooms, scopes,
authority). Zone coordination is an application concern — all the information
you need to implement it is available through the public API.
