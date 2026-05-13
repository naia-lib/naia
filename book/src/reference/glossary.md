# Glossary

## Protocol And Transport

**Protocol** — Shared registry of components, resources, messages, requests, and
channels. Both sides must build the same protocol.

**ProtocolId** — Deterministic hash of the protocol. A mismatch rejects the
handshake.

**Channel** — Named lane for messages with reliability/ordering settings.

**Transport** — Network layer below naia. Shipped features are WebRTC, UDP, and
local in-process transport.

**WebRTC** — Encrypted data-channel transport used by naia for native and Wasm
clients.

**RTT** — Round-trip time.

**Jitter** — Variation in packet delay.

## Replication

**Entity** — A world object registered with naia for replication.

**Component** — Replicated data attached to an entity.

**`Property<T>`** — Change-detection wrapper for replicated component fields.

**Replicated resource** — Singleton value replicated through a hidden
one-component entity.

**Delta compression** — Sending changed fields instead of full component state.

**Static entity** — Entity that sends one full snapshot on scope entry and does
not diff-track after that.

**Room** — Coarse interest group. A user and entity must share a room before the
entity can replicate to that user.

**UserScope** — Per-user visibility filter applied after rooms.

**ScopeExit** — Despawn-or-persist behavior when an entity leaves a user's
scope.

## Authority

**Server-owned** — Ordinary server-spawned replicated state; clients observe but
do not directly write.

**Client-authoritative entity** — Opt-in entity created/owned by a client and
replicated to the server.

**Publicity** — Client-owned entity state: `Private`, `Public`, or `Delegated`.

**Authority delegation** — Server-owned entity/resource mode where a client may
temporarily hold write authority.

**Granted / Denied / Available** — Client-visible authority states for delegated
objects.

## Time And Prediction

**Tick** — Simulation heartbeat.

**TickBuffered** — Channel mode that stamps messages with the client tick and
delivers them at the matching server tick.

**Confirmed entity** — Server-replicated entity used as the authoritative local
copy in prediction.

**Predicted entity** — Client-local duplicate that runs ahead using local input.

**`local_duplicate()`** — Helper that creates a local predicted copy of a
replicated entity.

**Rollback** — Reset to confirmed state at a past tick, then replay buffered
inputs.

**Historian** — Server-side rolling snapshot buffer used for lag compensation.

## Bandwidth And Serialization

**Bandwidth budget** — Target outbound bytes per second for a connection.

**Priority accumulator** — Per-entity/per-user priority state used to decide
what fits in the current budget.

**Token bucket** — Budget mechanism that accumulates send capacity over time.

**Bit-packing** — Serializing at bit granularity instead of byte granularity.

**zstd** — Optional packet compression algorithm with dictionary support.
