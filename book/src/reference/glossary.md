# Glossary

A reference for terms used throughout the naia book and game networking in
general.

---

**Authority delegation**
The mechanism by which a server-owned entity can be temporarily transferred to
client write authority. The server grants and revokes authority; the client never
holds unrevocable ownership. See [Authority Delegation](../authority/delegation.md).

**Bandwidth budget**
The per-connection outbound byte target per second (`BandwidthConfig::target_bytes_per_sec`).
The send loop accumulates a token bucket each tick and drains it against the
priority-sorted dirty entity list.

**Bit-packing**
Serializing values at the bit level (not byte level). naia's `Serde` trait
packs booleans into single bits and quantized numeric types into their exact
declared width, reducing wire size significantly compared to byte-aligned
formats.

**Channel**
A named transport lane with configurable ordering and reliability, registered
in the `Protocol`. Messages and entity actions travel through channels.
See [Messages & Channels](../concepts/messages.md).

**CommandHistory**
A client-side ring buffer that stores the last N ticks of input commands. Used
during rollback to re-simulate the predicted entity from the authoritative
server correction tick. See [Client-Side Prediction & Rollback](../advanced/prediction.md).

**Component**
A data struct that derives `Replicate`. Components carry `Property<T>` fields
that are change-tracked and delta-compressed across the network.

**Confirmed entity**
In a prediction setup, the server-replicated entity whose state reflects the
authoritative server view. Contrast with the *predicted entity* (client-local
copy). See [Client-Side Prediction & Rollback](../advanced/prediction.md).

**Delta compression**
The practice of sending only changed fields rather than the full component
state each tick. naia's `Property<T>` wrapper provides per-field change
detection. See [Delta Compression](../advanced/delta-compression.md).

**Delegated**
The `ReplicationConfig` state that marks a server entity as eligible for client
authority requests. See [Authority Delegation](../authority/delegation.md).

**Despawn**
The server removing an entity from the world. Clients receive a
`DespawnEntityEvent` for entities in their scope.

**DTLS**
Datagram Transport Layer Security. The encryption standard used by WebRTC data
channels. naia's `transport_webrtc` gets DTLS automatically from the WebRTC
spec. See [Security & Trust Model](../reference/security.md).

**ECS**
Entity-Component-System. A game architecture pattern. naia is ECS-agnostic —
its core crates work with any entity type that is `Copy + Eq + Hash`.

**Entity**
Any value satisfying `Copy + Eq + Hash + Send + Sync` that the game world
allocates. naia tracks replication metadata for entities but never allocates
them.

**Historian**
naia's rolling per-tick world snapshot buffer. Enables server-side lag
compensation by rewinding the world to the tick the client was seeing when
they fired a weapon. See [Lag Compensation with Historian](../advanced/historian.md).

**Jitter**
Random variation in one-way network delay. High jitter means packets arrive
at irregular intervals. naia's `LinkConditionerConfig` can simulate jitter
in local development.

**Lag compensation**
The server technique of rewinding its world state to the client's perspective
tick before performing authoritative hit detection. Prevents shots that were
visually accurate on the client from missing on the server due to RTT travel
time. See [Lag Compensation with Historian](../advanced/historian.md).

**local_duplicate()**
A client method that spawns a new client-local entity and copies all
`Replicate` components from a server-replicated entity. Used to create the
predicted counterpart in a client-side prediction setup.

**Misprediction**
When the client's predicted entity state diverges from the server's authoritative
state. The correction handler fires a rollback and re-simulates from the
authoritative tick.

**Packet loss**
The fraction of sent packets that are never acknowledged. naia's
`ConnectionStats::packet_loss_pct` tracks this over a rolling window.

**Predicted entity**
In a prediction setup, the client-local entity that runs ahead of the server,
applying inputs immediately before authoritative confirmation arrives. Contrast
with the *confirmed entity*.

**Priority accumulator**
The per-entity per-user internal counter that determines which entities get
bandwidth in the current tick. Higher gain accumulates priority faster.
See [Priority-Weighted Bandwidth](../advanced/bandwidth.md).

**`Property<T>`**
naia's change-detection wrapper for component fields. Mutation via `DerefMut`
marks the component dirty; only dirty fields are included in the next
`send_all_packets` call. See [Delta Compression](../advanced/delta-compression.md).

**Protocol**
The compile-time registry of all replicated component types, message types,
and channel configurations. Both server and client must build from the same
`Protocol`; a hash mismatch during the handshake causes rejection.
See [The Shared Protocol](../concepts/protocol.md).

**ProtocolId**
A deterministic hash derived from the registered types and channel
configuration in a `Protocol`. Mismatched IDs cause handshake rejection.

**Replication**
The automatic process by which the server's entity state is transmitted to
connected clients. naia handles spawn, update, and despawn events with per-field
delta compression.

**Rollback**
Snapping the predicted entity to the server's authoritative state at a past
tick, then re-simulating forward using buffered commands. The mechanism that
hides mispredictions without visible popping (when combined with error
interpolation). See [Client-Side Prediction & Rollback](../advanced/prediction.md).

**Room**
A coarse membership group. A user and an entity must share a room before
replication is possible. See [Rooms & Scoping](../concepts/rooms.md).

**RTT**
Round-trip time. The time for a packet to travel from sender to receiver and
back. naia tracks RTT as an EWMA and P50/P99 percentiles via `ConnectionStats`.

**Scope**
Whether an entity is currently being replicated to a specific user. Scope is
controlled by room membership (coarse) and `UserScope` (fine-grained).
See [Rooms & Scoping](../concepts/rooms.md).

**ScopeExit**
The behavior when an entity leaves a user's scope: `Despawn` (default — send a
despawn event to the client) or `Persist` (freeze the entity's last known state
without despawning).

**send_all_packets**
The server (and client) API call that serializes all accumulated diffs and
messages and flushes them to the network. Must be called exactly once per frame,
after all mutations for the frame are complete.

**Sequenced unreliable**
A channel mode where only the newest message is kept. Stale packets (arriving
out of order) are silently discarded. Ideal for position updates where only the
latest value matters.

**Static entity**
An entity created with `.as_static()` that skips delta tracking. A full snapshot
is sent when the entity enters scope; no further updates are sent after that.
See [Entity Replication](../concepts/replication.md).

**Tick**
The server's simulation heartbeat. `take_tick_events` advances the tick counter.
Each elapsed tick produces a `TickEvent` that drives the game simulation step.
See [Tick Synchronization](../concepts/ticks.md).

**TickBuffered**
A channel mode that stamps every message with the client tick at which the
input occurred. The server delivers them via `receive_tick_buffer_messages(tick)`
at the matching tick, enabling tick-accurate input replay.

**Token bucket**
The mechanism underlying naia's per-connection bandwidth budget. Each tick
`target_bytes_per_sec × dt` bytes of budget are added to the bucket; the send
loop drains the bucket against the priority-sorted dirty entity list.

**Transport**
The network layer below naia's protocol. naia ships `transport_udp` (native),
`transport_webrtc` (browser), and `transport_local` (in-process testing).
Custom transports implement the `Socket` trait.

**UserKey**
An opaque handle to a connected client on the server. Stable for the duration
of a connection; a new `UserKey` is issued on reconnect.

**UserScope**
Fine-grained per-user visibility control within a room. `include` / `exclude`
calls determine which entities replicate to which users.
See [Rooms & Scoping](../concepts/rooms.md).

**WebRTC**
Web Real-Time Communication. A browser standard providing encrypted UDP-like
data channels (DTLS). naia's `transport_webrtc` uses WebRTC to reach browser
clients from a native server.

**zstd**
Zstandard, a fast lossless compression algorithm. naia supports optional zstd
packet compression with default, custom-dictionary, and dictionary-training
modes. See [zstd Compression & Dictionary Training](../advanced/compression.md).
