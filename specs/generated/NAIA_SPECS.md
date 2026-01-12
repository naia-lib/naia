# Naia Specifications Bundle

This document contains all normative specifications for the Naia networking engine, concatenated into a single reference.

**Generated:** 2026-01-12 02:13 UTC
**Spec Count:** 14

---

## Table of Contents

- [2. Connection Lifecycle](#connection-lifecycle)
- [3. Transport](#transport)
- [4. Messaging](#messaging)
- [5. Time, Ticks & Commands](#time-ticks--commands)
- [6. Observability Metrics](#observability-metrics)
- [7. Entity Scopes](#entity-scopes)
- [8. Entity Replication](#entity-replication)
- [9. Entity Ownership](#entity-ownership)
- [10. Entity Publication](#entity-publication)
- [11. Entity Delegation](#entity-delegation)
- [12. Entity Authority](#entity-authority)
- [13. Server Events API](#server-events-api)
- [14. Client Events API Contract](#client-events-api-contract)
- [15. World Integration Contract](#world-integration-contract)

---

<!-- ======================================================================== -->
<!-- Source: 2_connection_lifecycle.md -->
<!-- ======================================================================== -->

# Connection Lifecycle

Last updated: 2026-01-08

## Purpose

This spec defines the **connection state machine** and **observable events** for Naia connections.

It is intentionally written at the Naia core API level. Engine adapters (hecs/bevy) MUST preserve these semantics; adapter-specific plumbing is out of scope.

---

## Glossary

- **Client**: a Naia client instance attempting to establish and maintain a session with a Server.
- **Server**: a Naia server instance accepting client sessions.
- **Transport**: the underlying network mechanism (e.g. UDP, WebRTC). Transport-specific mechanics are defined in `3_transport.md`, but lifecycle semantics are defined here.
- **Session**: the period from “connected” until “disconnected”.
- **Explicit reject**: the server deliberately refuses a connection attempt in a way the client can observe as a rejection (as opposed to generic network failure).
- **Auth request**: the application-defined credential payload sent Client → Server **out-of-band** (HTTP) before the transport session is initialized.
- **Identity token**: an opaque one-time token minted by the Server and presented by the Client during the transport handshake.

---

## Publicly observable client/server signals

### Client-side observable signals

- `ConnectionStatus`:
  - MUST have no “Rejected” state.
  - Rejection MUST be represented by emitting a `RejectEvent` and then remaining / returning to a non-connected status.

- `ConnectEvent`:
  - Emitted exactly once per successful session establishment.
  - MUST only be emitted after the handshake is fully finalized, including tick sync.

- `DisconnectEvent`:
  - Emitted when the client **was connected** and later loses the connection.
  - MUST NOT be emitted for a connection attempt that never reached “connected”.

- `RejectEvent`:
  - Emitted when the server explicitly rejects the connection attempt.
  - MUST NOT be emitted for generic failures where the server did not explicitly reject (e.g. packet loss, DNS failure, server unreachable).

### Server-side observable signals

- `AuthEvent`:
  - Emitted when `require_auth = true` and the server receives an auth request (the pre-transport HTTP credential payload).

- `ConnectEvent`:
  - Emitted when a session is fully established (handshake complete, tick sync complete).

- `DisconnectEvent`:
  - Emitted when an established session ends.

---

## Lifecycle state machine

### Client states (conceptual)

- **Disconnected**
- **Connecting** (includes: “auth in progress” when applicable; transport handshake in progress; tick sync in progress)
- **Connected**

### [connection-01] —

Client behavior MUST be describable by the above conceptual states, even if the implementation uses different internal states.

### [connection-02] —

The client MUST NOT expose a public “Rejected” connection state. Rejection is an event (RejectEvent), not a persistent state.

### Server states (per-client-session conceptual)

- **NoSession**
- **Handshaking**
- **Connected**

### [connection-03] —

The server MUST NOT treat a client as “Connected” (for purposes of entity replication, message delivery, tick semantics, etc.) until the handshake is finalized including tick sync.

---

## Authentication & identity tokens

### `require_auth = false`

### [connection-04] —

If `require_auth = false`, the server MUST allow clients to attempt connection without any pre-auth step.

### [connection-05] —

Implementations MAY still support optional application-level auth, but it must not be required by Naia for connection establishment when `require_auth = false`.

### `require_auth = true`

This mode uses an out-of-band HTTP auth step and a one-time identity token.

#### Pre-transport auth request (HTTP)

### [connection-06] —

When `require_auth = true`, a client MUST obtain a server-issued identity token via an out-of-band auth request (HTTP) BEFORE initializing the transport connection attempt.

### [connection-07] —

The server MUST evaluate the auth request and return either:
- `200 OK` (accepted) with an identity token, or
- `401 Unauthorized` (rejected) with no identity token.

### [connection-08] —

When the server receives an auth request in this mode, it MUST emit exactly one `AuthEvent` for that request.

### [connection-09] —

There is no Naia-level “auth timeout” during the transport handshake, because auth is completed before the transport session begins.

#### Identity token properties

### [connection-10] —

An identity token MUST be:
- **One-time use** (cannot be used successfully more than once), and
- **Time-limited** with TTL = **1 hour** from issuance.

### [connection-11] —

If a token is expired, already-used, or invalid, the server MUST explicitly reject the connection attempt (see “Explicit rejection”).

### [connection-12] —

Identity tokens MUST be required for **all transports** when `require_auth = true` (not only WebRTC).

### [connection-13] —

On first successful validation attempt, the server MUST mark the token as used (consumed). Replays MUST fail.

---

## Transport handshake & tick sync

### [connection-14] —

A successful connection handshake MUST include a tick synchronization step. A client MUST NOT be considered “Connected” until tick sync completes.

### [connection-15] —

The client MUST emit `ConnectEvent` only at the moment the handshake is finalized (including tick sync).

### [connection-16] —

The server MUST emit `ConnectEvent` only at the moment the handshake is finalized (including tick sync).

### [connection-17] —

Naia MUST NOT deliver any entity replication “writes” as part of an established session until after `ConnectEvent` is emitted for that session (server-side), and the client MUST NOT apply any such writes until after it has emitted `ConnectEvent`.

(See `5_time_ticks_commands.md` for tick semantics and how tick sync interacts with command history.)

---

## Explicit rejection

### [connection-18] —

The server MUST explicitly reject a connection attempt when:
- `require_auth = true` and the client presents no identity token,
- the presented token is invalid/expired/already-used,
- the server otherwise chooses to deny the attempt before session establishment.

### [connection-19] —

When the server explicitly rejects:
- The client MUST emit `RejectEvent`.
- The client MUST NOT emit `ConnectEvent`.
- The client MUST NOT emit `DisconnectEvent` (because it was never connected).

### [connection-20] —

After a `RejectEvent`, the client’s public `ConnectionStatus` MUST be (or return to) a non-connected state (e.g. Disconnected), with no special “Rejected” status.

---

## Disconnect semantics

### [connection-21] —

`DisconnectEvent` (client-side) MUST only be emitted if the client previously emitted `ConnectEvent` for the session.

### [connection-22] —

`DisconnectEvent` (server-side) MUST only be emitted if the server previously emitted `ConnectEvent` for the session.

### [connection-23] —

When a client disconnects (or is disconnected) after session establishment:
- It is treated as immediately out-of-scope for all entities, and
- Any client-owned entities owned by that client MUST be despawned by the server.
(See `9_entity_ownership.md` and `7_entity_scopes.md`.)

---

## Event ordering guarantees

### Successful session (require_auth = true)

### [connection-24] —

For a single successful connection where `require_auth = true`, the server MUST observe events in this order:
1. `AuthEvent`
2. `ConnectEvent`
3. `DisconnectEvent` (eventually)

### Successful session (require_auth = false)

### [connection-25] —

For a single successful connection where `require_auth = false`, the server MUST observe:
1. `ConnectEvent`
2. `DisconnectEvent` (eventually)

### Client-side ordering (all modes)

### [connection-26] —

For a single successful session, the client MUST observe:
1. `ConnectEvent`
2. `DisconnectEvent` (eventually)

### [connection-27] —

For a rejected attempt, the client MUST observe:
1. `RejectEvent`
…and MUST NOT observe `ConnectEvent` or `DisconnectEvent` for that attempt.

---

## Non-goals / Out of scope

- The exact HTTP route(s), headers, or body format of the auth request.
- Transport-specific wire details for how the token is conveyed.
- Engine adapter (bevy/hecs) implementation details.
- Retry/backoff policies for repeated connection attempts (may be defined in a future spec if needed).

## Test obligations

TODO: Define test obligations for this specification.


---

<!-- ======================================================================== -->
<!-- Source: 3_transport.md -->
<!-- ======================================================================== -->

# Transport

Last updated: 2026-01-08

This spec defines the transport boundary contract for **Naia** (`naia_client` + `naia_server`).
It is **transport-agnostic**: Naia can run over UDP, WebRTC, or local channels. Naia assumes transports are unordered/unreliable and does not rely on stronger guarantees even if a transport happens to provide them.

Reliability, ordering, fragmentation, resend, and dedupe guarantees belong to `4_messaging.md`.

---

## Scope

This spec owns:
- Naia’s assumptions about the transport layer
- Naia’s packet-size boundary (MTU) and Naia-level error behavior
- Naia’s behavior on malformed/oversize inbound packets at the boundary

This spec does **not** own:
- Socket-crate-specific behavior (`naia_client_socket`, `naia_server_socket`)
- Message reliability/ordering/fragmentation semantics (see `4_messaging.md`)
- Entity replication semantics (see entity suite specs)
- Auth semantics (see `2_connection_lifecycle.md`)

---

## Definitions

- **Transport adapter**: the implementation used by Naia to send/receive packets (UDP/WebRTC/local).
- **Packet**: a single datagram-like unit delivered by the transport adapter.
- **Packet payload**: the bytes Naia asks the transport adapter to send in one packet.
- **MTU_SIZE_BYTES**: the maximum packet payload allowed by Naia core, exposed via `naia_shared`. :contentReference[oaicite:3]{index=3}
- **Prod vs Debug**: Debug means `debug_assertions` enabled; Prod means disabled.

---

## Contracts

### [transport-01] — Naia assumes transport is unordered & unreliable
Naia MUST assume packets may be dropped, duplicated, and reordered, and MUST NOT rely on:
- in-order delivery
- exactly-once delivery
- guaranteed delivery

(UDP/WebRTC/local are all valid so long as Naia can treat them as such.) :contentReference[oaicite:4]{index=4}

---

### [transport-02] — MTU boundary is defined by `naia_shared::MTU_SIZE_BYTES`
Naia MUST treat `MTU_SIZE_BYTES` as the maximum size of a **single packet payload**. :contentReference[oaicite:5]{index=5}

Naia MUST NOT knowingly ask a transport adapter to send a packet payload larger than `MTU_SIZE_BYTES`.

---

### [transport-03] — Oversize outbound packet attempt returns `Err` at Naia layer
If Naia is asked (directly or indirectly) to send data that would require an outbound packet payload larger than `MTU_SIZE_BYTES`, Naia MUST return `Result::Err` from the initiating Naia-layer API.

This is a Naia contract (even if a particular transport adapter would panic). Naia must validate before calling the adapter.

---

### [transport-04] — Malformed or oversize inbound packets are dropped
If Naia receives a packet that is:
- larger than `MTU_SIZE_BYTES`, or
- malformed / cannot be decoded at the packet boundary,

then:
- In **Prod**: Naia MUST drop it silently.
- In **Debug**: Naia MUST drop it and emit a warning.

(Exact warning text is not part of the contract.)

---

### [transport-05] — No transport-specific guarantees may leak upward
Naia’s higher layers (messaging/replication) MUST behave identically regardless of whether the underlying transport happens to be “better” (e.g. local channels).
Any guarantee stronger than transport-01 MUST be explicitly specified in `4_messaging.md`, not inferred from the transport adapter.

---

## Test obligations (TODO)
- transport-01: Verify Naia tolerates reorder/drop/duplicate at packet boundary (via test transport / local conditioner).
- transport-03: Verify oversize outbound attempt returns Err (and does not panic).
- transport-04: Verify malformed inbound is dropped (warn only in Debug).


---

<!-- ======================================================================== -->
<!-- Source: 4_messaging.md -->
<!-- ======================================================================== -->

# Messaging

Last updated: 2026-01-08

This spec defines Naia’s **message channel** contract for `naia_client` + `naia_server`.

It owns:
- Channel registration & configuration semantics (direction, mode)
- Delivery/ordering/duplication guarantees per ChannelMode
- Fragmentation rules for reliable channels
- Rules for messages containing `EntityProperty` (entity references) and entity-lifetime safety
- Buffering bounds & TTLs required for determinism + memory safety

It does NOT own:
- Transport adapter behavior (see `3_transport.md`)
- Entity replication semantics (see entity suite specs)
- Connection/auth handshake rules (see `2_connection_lifecycle.md`)

---

## Definitions

- **Channel**: A configured lane used to send/receive Messages (and optionally Requests/Responses).
- **ChannelKind**: A unique identifier for a channel type in a Protocol.
- **ChannelDirection**: The allowed send direction for a channel (Client→Server or Server→Client, as configured).
- **ChannelMode**: The delivery/ordering semantics of a channel. Naia exposes multiple modes. :contentReference[oaicite:3]{index=3}
- **Reliable**: Naia guarantees eventual delivery of a message while the connection remains active, and ensures the application observes the message at most once (deduped).
- **Ordered**: Application observes messages in the same order they were sent on that channel.
- **Sequenced**: Messages represent “current state”; older state MUST NOT be observed after newer state has been observed (no rollback). Intermediate states MAY be skipped.
- **TickBuffered**: Messages are grouped by tick and exposed per tick in tick order.
- **Tick**: A shared tick value used by Naia; `Tick` is `u16` and wraps. :contentReference[oaicite:4]{index=4}
- **Entity lifetime** (client-side): scope enter → scope leave, with the “≥ 1 tick out-of-scope” rule (see `7_entity_scopes.md` / `8_entity_replication.md`).

Normative keywords: MUST, MUST NOT, MAY, SHOULD.

---

## Global error-handling policy

### [messaging-01] — User-initiated errors are Results
When an error is caused by local application code or local configuration (e.g. invalid channel configuration, oversize payload send), Naia MUST return `Result::Err` from the initiating API rather than panicking.

### [messaging-02] — Remote/untrusted input MUST NOT panic
When an error is caused by remote input or the network (malformed payload, reorder, duplicates, stale ticks, unresolved entity references, spam), Naia MUST NOT panic.
- In Prod: drop silently
- In Debug: drop and emit a warning (exact text not specified)

### [messaging-03] — Framework invariant violations MUST panic
If Naia violates its own declared invariants (e.g. delivers older state after newer on a sequenced channel, attempts internal send exceeding declared bounds), Naia MUST panic.

(These conditions are considered Naia bugs and are expected to be unreachable in correct implementations.)

---

## Channel configuration

### [messaging-04] — Protocol registration must match on both sides
A given connection MUST have compatible channel registrations:
- Same ChannelKind refers to the same logical channel
- ChannelMode and ChannelDirection MUST be compatible

If channel registrations are incompatible, connection establishment MUST fail (see `2_connection_lifecycle.md` for failure surfacing).

### [messaging-05] — ChannelDirection is enforced at send-time
If local code attempts to send a message on a channel that is not configured for that direction, Naia MUST return `Result::Err`. (user-initiated)

---

## ChannelMode guarantee matrix

This table defines the observable application-level contract.

| ChannelMode | Delivery | Dedup | Ordering | Sequenced “no rollback” |
|---|---|---|---|---|
| UnorderedUnreliable | best-effort (may drop) | no | none | no |
| SequencedUnreliable | best-effort (may drop) | no | none | YES |
| UnorderedReliable | eventual while connected | YES | none | no |
| OrderedReliable | eventual while connected | YES | YES (send order) | no |
| SequencedReliable | eventual while connected (latest) | YES | none | YES |
| TickBuffered | per tick buffer (Client→Server only) | (mode-defined; see below) | tick order | n/a |

---

## UnorderedUnreliable

### [messaging-06] — Best-effort, no ordering, duplicates allowed
UnorderedUnreliable:
- MAY drop messages
- MAY deliver messages out of send order
- MAY deliver duplicates (application must tolerate)

---

## SequencedUnreliable

### [messaging-07] — Best-effort, “latest wins”, no rollback
SequencedUnreliable:
- MAY drop messages
- MAY deliver out of send order
- MUST enforce sequenced semantics:
    - Once the application has observed message M with sequence S_new, it MUST NOT later observe any message with sequence S_old where S_old is older than S_new (wrapping-safe comparison required).
    - Intermediate sequence values MAY be skipped.

Duplicates MAY occur (unreliable), and MUST NOT cause rollback.

---

## UnorderedReliable

### [messaging-08] — Reliable delivery, deduped, unordered
UnorderedReliable:
- MUST ensure eventual delivery while the connection remains active
- MUST dedupe so each message is observed at most once
- MUST NOT guarantee send-order delivery

---

## OrderedReliable

### [messaging-09] — Reliable + strict send-order delivery
OrderedReliable:
- MUST ensure eventual delivery while connected
- MUST dedupe so each message is observed at most once
- MUST deliver messages to the application in the same order they were sent on that channel
- MUST use wrap-safe ordering/indices to preserve correctness across wrap-around

---

## SequencedReliable

### [messaging-10] — Reliable + “latest wins” + no rollback
SequencedReliable is intended for “current-state streams”.

SequencedReliable:
- MUST ensure eventual delivery of the newest state while connected
- MUST dedupe (at-most-once observation for any given delivered state)
- MUST enforce sequenced semantics:
    - Once the application has observed a message with sequence S_new, it MUST NOT later observe any message with sequence older than S_new.
    - Intermediate states MAY be skipped.
- MUST NOT allow a receiver to revert to an older state due to reordering, retransmission, or delayed delivery.

---

## TickBuffered

TickBuffered is a standalone ChannelMode with TickBufferSettings. :contentReference[oaicite:5]{index=5}

### [messaging-11] — TickBuffered is Client→Server only
TickBuffered channels MUST be configurable only for Client→Server direction.
If configured for any other direction, Naia MUST return `Result::Err`. (user-initiated)

### [messaging-12] — TickBuffered groups messages by tick and exposes ticks in order
TickBuffered:
- Each message is associated with a Tick.
- The receiver MUST buffer messages grouped by Tick.
- When the receiver exposes buffered messages, it MUST expose ticks in increasing tick order (wrap-safe).
- A tick MAY have zero, one, or many messages.

### [messaging-13] — TickBuffered capacity and eviction
TickBuffered has a fixed `message_capacity`.
- The receiver MUST NOT retain more than `message_capacity` total buffered messages.
- If adding a message would exceed capacity, the receiver MUST evict the oldest buffered tick groups first (oldest ticks) until within capacity.
- Eviction is considered remote/untrusted pressure; Naia MUST NOT panic. (See messaging-02)

### [messaging-14] — TickBuffered discards very-late ticks
If a message arrives for a tick that is older than the oldest tick currently retained (i.e., it would fall behind the retained window), the receiver MUST discard it.
- Prod: discard silently
- Debug: discard with warning

---

## Fragmentation and MTU

Naia defines a maximum packet payload size `MTU_SIZE_BYTES` at the transport boundary. :contentReference[oaicite:6]{index=6}

### [messaging-15] — Unreliable channels MUST NOT fragment
For UnorderedUnreliable and SequencedUnreliable:
- If a message payload would require fragmentation, Naia MUST return `Result::Err` from the send call. (user-initiated)

### [messaging-16] — Reliable channels MAY fragment up to a hard bound
For UnorderedReliable / OrderedReliable / SequencedReliable:
- Naia MAY fragment a message across multiple packets.
- Maximum fragments per message is a fixed bound:

  `MAX_RELIABLE_MESSAGE_FRAGMENTS = 2^16`

- If a user attempts to send a message requiring more than the bound, Naia MUST return `Result::Err`. (user-initiated)
- If Naia internally attempts to exceed this bound, Naia MUST panic. (framework invariant)

---

## Wrap-around safety

Tick and (where applicable) channel indices/sequence numbers wrap and must be compared using wrap-safe logic. Naia provides explicit wrapping helpers in shared code. :contentReference[oaicite:7]{index=7}

### [messaging-17] — Wrap-around MUST NOT break ordering or sequencing contracts
All ordering/sequence comparisons (OrderedReliable ordering, Sequenced* “newer than” checks, TickBuffered tick ordering) MUST be correct across wrap-around.

---

## Messages containing EntityProperty

Messages may contain EntityProperty values which refer to entities that may or may not currently exist in the receiver’s active entity lifetime. :contentReference[oaicite:8]{index=8}

### [messaging-18] — EntityProperty must not violate entity lifetime
A message that contains an EntityProperty MUST NOT be applied to an entity outside its current active lifetime.
If the referenced entity is not currently present in the receiver’s active lifetime:
- Naia MAY buffer the message for later resolution (see TTL below), or
- Naia MAY drop the message (prod silent, debug warn)

Naia MUST NOT apply a buffered EntityProperty message after the referenced entity has completed a lifetime and despawned (no cross-lifetime leakage).

### [messaging-19] — EntityProperty resolution TTL (bounded buffering by time)
If Naia buffers messages due to unresolved EntityProperty references, it MUST enforce a TTL:

`ENTITY_PROPERTY_RESOLUTION_TTL = 60 seconds`

- The TTL MUST be measured using Naia’s monotonic time source (not wall-clock time).
- A buffered message that remains unresolved longer than TTL MUST be dropped.
  - Prod: drop silently
  - Debug: drop with warning
- TTL expiry is remote/untrusted input pressure; Naia MUST NOT panic.

Determinism requirement:
- Under a deterministic time source (test clock), identical scripted time advancement MUST produce identical TTL drop behavior.

### [messaging-20] — EntityProperty buffering hard cap
In addition to TTL, Naia MUST enforce a hard cap to prevent unbounded memory growth:

- `MAX_PENDING_ENTITY_PROPERTY_MESSAGES_PER_CONNECTION = 4096`
- `MAX_PENDING_ENTITY_PROPERTY_MESSAGES_PER_ENTITY = 128`

If the cap would be exceeded:
- Naia MUST drop buffered messages using an eviction policy that guarantees bounded memory (recommended: drop oldest first).
- Prod: silent
- Debug: warning
- MUST NOT panic

---

## Test obligations (TODO)

- messaging-06: UnorderedUnreliable can reorder/drop/duplicate; receiver does not assume otherwise.
- messaging-07: SequencedUnreliable never rolls back after newer state is observed.
- messaging-08: UnorderedReliable dedupes and eventually delivers while connected.
- messaging-09: OrderedReliable delivers in send order despite network reorder.
- messaging-10: SequencedReliable exposes only latest; never rolls back.
- messaging-11..14: TickBuffered grouping, order, capacity eviction, very-late tick discard.
- messaging-15: Unreliable oversize send returns Err (no fragmenting).
- messaging-16: Reliable fragmentation works up to 2^16 fragments; oversize returns Err.
- messaging-18..20: EntityProperty buffering obeys TTL + cap; never leaks across lifetimes.
- messaging-17: Wrap-around does not break any above guarantees.


---

<!-- ======================================================================== -->
<!-- Source: 5_time_ticks_commands.md -->
<!-- ======================================================================== -->

# Time, Ticks & Commands

Last updated: 2026-01-09

This spec defines Naia’s public contract for:
- time sources and duration measurement
- tick semantics (server tick, client tick, wrap-around ordering)
- tick synchronization and client tick-lead targeting
- command tick tagging and server acceptance rules

This spec applies to Naia (`naia_server`, `naia_client`). It is transport-agnostic.

Normative keywords: MUST, MUST NOT, MAY, SHOULD.

---

## Scope ownership

This spec owns:
- the canonical time source used for all duration-based behavior
- base tick rate definition and tick advancement rules
- wrap-safe tick ordering and comparison semantics
- the client tick-lead targeting model and how client tick relates to server tick
- command acceptance semantics

This spec does NOT own:
- transport drop/dup/reorder assumptions (see `3_transport.md`)
- message channel ordering/reliability (see `4_messaging.md`)
- entity replication/lifetime (see entity suite)
- connection admission/auth steps (see `2_connection_lifecycle.md`)

---

## Definitions

- **Time Provider**: Naia’s time abstraction used to read a monotonic “now” and measure durations. Tests MAY substitute a deterministic/fake time provider to simulate time passing.

- **Instant**: Naia’s cross-platform monotonic instant type. It MUST NOT be wall clock time.

- **Duration**: monotonic elapsed time between instants.

- **TickRate**: the configured base duration per tick, expressed in milliseconds, shared between client and server. TickRate is fixed for the lifetime of a connection.

- **Server Tick**: the authoritative tick counter maintained by the server, advancing according to TickRate.

- **Client Tick**: the client’s tick counter. The client tracks the same base TickRate, but MAY adjust its pacing to maintain a target lead ahead of the server (see “Client Tick Lead”).

- **Tick**: a `u16` tick index. Tick values wrap around.

- **Command**: client-authored input tagged to a tick.

---

## Global error-handling policy

### [time-01] — User-initiated misuse returns Result::Err
If a failure is caused by local application misuse/configuration at the Naia API layer, Naia MUST return `Result::Err` from the initiating API.

### [time-02] — Remote/untrusted anomalies MUST NOT panic
If a failure is caused by remote input or network behavior (duplicates, reordering, late arrival), Naia MUST NOT panic.
- Prod: ignore/drop silently
- Debug: ignore/drop with warning

### [time-03] — Framework invariant violations MUST panic
If Naia violates an invariant stated in this spec (e.g., tick goes backwards in public API, wrap-order is applied incorrectly, commands are applied more than once), Naia MUST panic.

---

## Canonical time source

### [time-04] — All durations use Naia’s monotonic time provider
All duration-based behavior in Naia (tick advancement, TTL expiry, lead targeting, timeouts if applicable) MUST be derived from Naia’s monotonic Time Provider (Instant/Duration), not wall-clock time.

### [time-05] — Determinism under deterministic time provider
If the Time Provider is deterministic (e.g. in tests), and the sequence of Time Provider advancements is identical, then tick advancement and time-based decisions MUST be deterministic.

---

## Tick semantics

### [time-06] — TickRate is fixed and shared
TickRate is configured as a duration per tick (milliseconds) and MUST be shared between client and server configs for a connection.
TickRate MUST NOT change during a connection’s lifetime.

### [time-07] — Server Tick advances from elapsed time
The server MUST advance its tick counter based on elapsed duration and TickRate.

- The server MUST NOT “invent” ticks without elapsed time.
- The server MAY advance by multiple ticks in one update step if enough time has elapsed.
- The server MUST NOT skip ticks that would have occurred due to elapsed time (no silent drop of tick progression).

(Best-practice note: if the host loop is delayed, processing multiple ticks to catch up is preferred over permanently slowing simulation.)

### [time-08] — Client Tick is monotonic and wrap-safe
The client tick MUST be monotonic non-decreasing in the wrap-safe sense (see time-09). It MUST NOT move backwards.

### [time-09] — Wrap-safe tick ordering rule
Tick is `u16` and wraps. Naia MUST define “newer than / older than” with a wrap-safe comparison:

Let `diff = (a - b) mod 2^16` (u16 wrapping subtraction interpreted as 0..65535).
- `a` is newer than `b` iff `diff` is in `1..32767`.
- `a` is equal to `b` iff `diff == 0`.
- `a` is older than `b` iff `diff` is in `32769..65535`.

Tie-break rule (half-range ambiguity):
- If `diff == 32768` (exactly half range apart), Naia MUST treat `a` as NOT newer than `b` and NOT older than `b` (ambiguous). Implementations MUST NOT rely on ordering in this exact case and MUST choose a deterministic behavior (recommended: treat as “not newer” for eviction / sequencing checks).

---

## Tick synchronization

### [time-10] — ConnectEvent implies tick sync complete
A successful connection handshake MUST include tick synchronization, and the client MUST NOT emit `ConnectEvent` until tick sync is complete. (See `2_connection_lifecycle.md`.)

Tick sync guarantees:
- The client knows the server’s current tick at connection time (or a tick sufficiently recent to compute lead targeting).
- The client can begin maintaining a lead relative to server tick.

---

## Client tick lead targeting (Overwatch-style)

### [time-11] — Client tick targets a lead ahead of server tick
The client MUST attempt to keep its tick ahead of the server by a target lead duration:

`target_lead = RTT + (jitter_std_dev * 3) + TickRate`

Where:
- RTT and jitter_std_dev are estimated by Naia’s connection measurement.
- TickRate is the configured duration-per-tick.

### [time-12] — Client pacing may adjust to maintain lead
To maintain the target lead:
- The client MAY slightly speed up or slow down its tick pacing relative to the base TickRate.
- The client MUST remain monotonic (time-08).
- The client MUST converge toward maintaining `client_tick_time - server_tick_time ≈ target_lead` over time.

This spec does not mandate the exact controller (PID, clamp, etc.), but it DOES mandate the target and monotonicity constraints.

---

## Commands

### [commands-01] — Every command is tagged to a tick
Every command sent by the client MUST be tagged with a tick value.

### [commands-02] — Server applies commands at most once
The server MUST apply a given logical command at most once to authoritative simulation.
Duplicates (retransmits, duplicates at network layer) MUST NOT cause double-application.

### [commands-03] — “Arrives in time” acceptance rule
A command tagged for tick `T` is considered on-time iff it is received by the server before the server begins processing tick `T`.

- If received on-time, the server MAY apply it when processing tick `T` (exact ordering among multiple commands for the same tick is implementation-defined, but MUST be deterministic).
- If received late (server has already begun or completed processing tick `T`), the server MUST ignore it.

Ignored late commands are remote/untrusted input outcomes:
- Prod: ignore silently
- Debug: ignore with warning

(There is no public “rejected command error” surfaced to the client; the contract is that late commands are ignored.)

### [commands-04] — Client lead targeting is the primary mechanism to avoid late commands
The intended mechanism to ensure commands arrive on-time is client lead targeting (time-11/time-12). The server remains authoritative and will ignore late commands regardless.

### [commands-05] — Disconnect cleans in-flight command state
On disconnect:
- any buffered/in-flight commands for that session MUST be discarded,
- no commands from that session may be applied after disconnect.

---

## Test obligations (TODO)

- time-04/time-05: deterministic time provider yields deterministic tick progression
- time-07: server tick advances exactly as implied by elapsed time and TickRate (no invented ticks)
- time-09: wrap-safe ordering holds across wrap; half-range tie is deterministic and does not corrupt ordering logic
- time-10: ConnectEvent only after tick sync complete
- time-11/time-12: client lead converges toward target_lead under changing RTT/jitter estimates
- commands-02: duplicate command deliveries do not double-apply
- commands-03: late commands are ignored deterministically
- commands-05: disconnect prevents any further command application


---

<!-- ======================================================================== -->
<!-- Source: 6_observability_metrics.md -->
<!-- ======================================================================== -->

# Observability Metrics

This spec defines the only valid semantics for *observability metrics* exposed by Naia (latency/RTT, bandwidth/throughput, and related counters).  
Normative keywords: **MUST**, **MUST NOT**, **SHOULD**, **MAY**.

---

## Glossary

- **Metric**: A numeric value exposed by Naia intended for monitoring/telemetry (not gameplay correctness).
- **Sample**: One measurement update contributing to a metric over time.
- **Window**: The time span or sample span used to aggregate a metric (moving average, EWMA, rolling sum, etc.).
- **RTT**: Round-trip time estimate (latency) derived from request/ack/heartbeat timing.
- **Throughput**: Bytes-per-second estimate (send and/or receive).
- **Steady link**: A link condition where latency/loss/jitter parameters are stable over multiple windows.
- **Fault model**: Packet loss, duplication, and reordering consistent with Naia transport simulation or real transport.

---

## References

- `3_transport.md` (fault model, heartbeats/acks, ordering/duplication behavior)
- `2_connection_lifecycle.md` (connect/disconnect lifecycle, timeouts, cleanup)
- `5_time_ticks_commands.md` (time source expectations, tick/time monotonicity)

---

## Contracts

### [observability-01] — Metric scope and non-normative gameplay impact
**Rule:** Observability metrics MUST NOT affect replicated state correctness, authority, scope, message delivery semantics, or any other gameplay-visible contract. Metrics are observational only.

**Clarifications:**
- Metrics MAY be used by applications for UI, logging, or adaptive behavior, but Naia’s core semantics MUST remain correct regardless of whether metrics are queried.

**Test obligations:**
- **observability-01.t1**: Run a representative scenario with metrics queried every tick vs never queried; externally observable replication/events MUST be identical.

---

### [observability-02] — Metric query safety and availability
**Rule:** Metrics APIs MUST be safe to query at any time after client/server object construction and MUST NOT panic. If a metric cannot be computed yet (insufficient data), it MUST return a well-defined default.

**Required defaults:**
- RTT: MUST return `None` or a documented sentinel value (e.g., 0) until enough samples exist.
- Throughput: MUST return 0 until enough samples exist.

**Test obligations:**
- **observability-02.t1**: Query metrics before connect, during handshake/auth delay, and immediately after connect; MUST not panic and MUST return defined defaults.
- **observability-02.t2**: Query metrics after disconnect; MUST not panic and MUST return defined defaults (or remain last-known if explicitly documented — choose one and enforce consistently).

---

### [observability-03] — RTT must be non-negative and bounded
**Rule:** RTT estimates MUST be non-negative. RTT MUST NOT overflow or become NaN/Infinity. Under stable link conditions, RTT SHOULD converge within a reasonable tolerance of the configured/true RTT.

**Interpretation:**
- “Reasonable tolerance” is implementation-defined but MUST be testable (e.g., within ±20% after N samples).

**Test obligations:**
- **observability-03.t1**: Under fixed-latency, low-jitter conditions, RTT converges near expected RTT and never negative.
- **observability-03.t2**: Under high jitter and moderate loss, RTT remains finite, non-negative, and bounded (no overflow/NaN).

---

### [observability-04] — RTT behavior under jitter, loss, and reordering
**Rule:** Under the transport fault model, RTT estimates MAY fluctuate but MUST remain stable in the sense that:
- It MUST NOT become negative.
- It MUST NOT oscillate wildly due to duplicate packets alone.
- Reordering MUST NOT cause RTT regression to an impossible value (e.g., negative elapsed time).

**Test obligations:**
- **observability-04.t1**: Enable packet duplication at high rate; RTT MUST not spike unboundedly solely due to duplicates.
- **observability-04.t2**: Enable reordering; RTT MUST remain finite and non-negative.

---

### [observability-05] — Throughput must be non-negative and monotonic per window semantics
**Rule:** Throughput (bytes/sec) MUST be non-negative and MUST NOT overflow or become NaN/Infinity. Throughput computations MUST be consistent with the documented windowing method.

**Clarifications:**
- If Naia uses a moving window/EWMA, then “monotonic” is not required; however values MUST update in the expected direction under sustained traffic changes:
  - Sustained higher traffic SHOULD increase reported throughput.
  - Sustained near-idle SHOULD decrease reported throughput toward 0.

**Test obligations:**
- **observability-05.t1**: Alternate between high-traffic and idle phases; throughput rises during high-traffic and decays during idle.
- **observability-05.t2**: Under constant traffic rate, throughput stabilizes near expected rate (within tolerance).

---

### [observability-06] — Bandwidth accounting includes retries/overhead if documented
**Rule:** If Naia exposes both “payload bytes” and “wire bytes” (or equivalent), then:
- Payload bytes MUST count only application payload (messages/components).
- Wire bytes MUST include protocol overhead and retransmissions.

If only one throughput metric exists, the spec MUST declare which accounting model it uses, and the implementation MUST match that model.

**Test obligations:**
- **observability-06.t1**: With reliable channel retries induced (loss), wire throughput increases relative to payload throughput (if both exist), or the single metric matches the documented accounting model.

---

### [observability-07] — Metrics reset/cleanup on connection lifecycle
**Rule:** On disconnect, Naia MUST clean up connection-scoped metric state so that:
- New connections do not inherit stale samples from prior connections.
- Metrics for a disconnected session MUST not continue accumulating samples.

**Allowed behaviors (pick one per metric and document consistently):**
- **Reset-to-default**: metrics revert to defaults immediately on disconnect.
- **Freeze-last-known**: metrics retain last-known value but do not update until reconnect; upon reconnect, metrics MUST reset or explicitly start a new session.

**Test obligations:**
- **observability-07.t1**: Connect, establish stable RTT, disconnect, reconnect; metrics MUST not “start” with prior session’s converged value unless Freeze-last-known is explicitly chosen AND reconnect resets correctly.

---

### [observability-08] — Time source assumptions
**Rule:** Metrics computations MUST rely on the same monotonic time source used by Naia’s tick/time system. Metrics MUST NOT assume wall-clock correctness. If the time source is paused (per deterministic test clock), metrics MUST behave consistently:
- No negative durations.
- No division by zero.
- Either no updates occur during pause or updates are well-defined (documented).

**Test obligations:**
- **observability-08.t1**: Pause deterministic time, keep querying metrics; MUST not panic and MUST not produce invalid values.
- **observability-08.t2**: Resume time; metrics continue updating normally.

---

### [observability-09] — Per-direction and per-transport consistency (if applicable)
**Rule:** If Naia exposes separate send/receive metrics, they MUST reflect direction correctly (send counts bytes sent, receive counts bytes received). If multiple transports exist, semantics MUST be consistent across transports (modulo known transport overhead differences).

**Test obligations:**
- **observability-09.t1**: Server sends heavy traffic, client sends minimal; send/receive metrics reflect asymmetry correctly.
- **observability-09.t2**: Run the same scenario over two transports; metrics remain within expected relative differences and do not violate invariants.

---

## Notes for implementers

- This spec does not mandate a particular estimator (EWMA vs rolling window), but it DOES mandate:
  - Non-negative, finite outputs
  - Defined behavior with insufficient samples
  - Correct lifecycle cleanup
  - Convergence under stable conditions
- Any exposed metric MUST be documented in terms of:
  - Units
  - Window/estimator
  - Reset/freeze behavior on disconnect

---

## Appendix: Test Tolerance Constants

These constants define acceptable tolerances for E2E test assertions:

| Constant | Value | Description |
|----------|-------|-------------|
| `RTT_TOLERANCE_PERCENT` | 20 | Acceptable deviation from expected RTT |
| `RTT_MIN_SAMPLES` | 10 | Minimum samples before asserting RTT convergence |
| `RTT_MAX_VALUE_MS` | 10000 | Maximum valid RTT (sanity bound) |
| `THROUGHPUT_TOLERANCE_PERCENT` | 15 | Acceptable deviation from expected throughput |
| `THROUGHPUT_MIN_SAMPLES` | 5 | Minimum samples before asserting throughput |
| `LEAD_CONVERGENCE_TICKS` | 60 | Ticks to allow client tick lead to stabilize |
| `METRIC_WINDOW_DURATION_MS` | 1000 | Default metric aggregation window |

**Usage in tests:**
```rust
// Assert RTT within tolerance
assert!(
    (measured_rtt - expected_rtt).abs() <= expected_rtt * RTT_TOLERANCE_PERCENT / 100,
    "RTT {} not within {}% of expected {}",
    measured_rtt, RTT_TOLERANCE_PERCENT, expected_rtt
);
```

## Test obligations

TODO: Define test obligations for this specification.


---

<!-- ======================================================================== -->
<!-- Source: 7_entity_scopes.md -->
<!-- ======================================================================== -->

# Entity Scopes

Entity Scopes define whether a given Entity `E` is **in-scope** or **out-of-scope** for a given User/Client `U`,
and the required observable consequences of scope transitions.

This spec defines:
- The **scope membership predicate** (Rooms + per-user include/exclude filters + required coupling).
- The **state machine** for `InScope(U,E)` / `OutOfScope(U,E)` and its client-visible effects.
- Deterministic **tick-level collapse** rules for scope changes.
- Required behavior under reordering / illegal states.

This spec does not define:
- Ownership write permissions (see `9_entity_ownership.md`)
- Publication gating for client-owned entities (see `10_entity_publication.md`)
- Delegation/authority semantics (see `11_entity_delegation.md`, `12_entity_authority.md`)
- Replication ordering/wire format (see `8_entity_replication.md`)

---

## 1) Vocabulary

- **User U**: a server-identified remote client/user (keyed by `user_key`).
- **Entity E**: a networked entity tracked by Naia replication.
- **Room**: a server-managed grouping for coarse scope gating (users and entities may be members of multiple rooms).
- **SharesRoom(U,E)**: true iff `U` and `E` share at least one common room.
- **Include(U,E)**: per-user scope inclusion filter set via `server.user_scope_mut(user_key).include(entity)`.
- **Exclude(U,E)**: per-user scope exclusion filter set via `server.user_scope_mut(user_key).exclude(entity)`.

### Diagnostics
- **Diagnostics enabled**: a build/feature/runtime mode where Naia may emit warnings for illegal/impossible states.
  When diagnostics are not enabled (production default), Naia must remain silent.

---

## 2) Core Scope Predicate

### [entity-scopes-01] — Rooms are a required coarse gate for non-owners
For any user `U` and entity `E`, `SharesRoom(U,E)` MUST be a necessary precondition for `InScope(U,E)`, except where
other specs explicitly override (e.g. owning client always in-scope for its client-owned entities; see below).

If `SharesRoom(U,E) == false`, then `OutOfScope(U,E)` MUST hold.

### [entity-scopes-02] — Per-user include/exclude is an additional filter (additive after Rooms)
Assuming `SharesRoom(U,E) == true`, the server MUST apply the per-user filter as follows:

- If `Exclude(U,E)` is active, then `OutOfScope(U,E)` MUST hold.
- Else if `Include(U,E)` is active, then `InScope(U,E)` MUST hold (subject to other gates like publication).
- Else (neither active), the default MUST be `InScope(U,E)` (subject to other gates like publication).

### [entity-scopes-03] — Include/Exclude ordering: last call wins
If both `Include(U,E)` and `Exclude(U,E)` are applied over time, the effective filter state MUST be determined by
the most recently applied call for that `(U,E)` pair (last call wins).

This rule is defined in terms of the server’s resolved mutation order (i.e. “last call” means last in the server’s
finalized application order for that tick).

### [entity-scopes-04] — Roomless entities are out-of-scope for all non-owners
If `E` is in zero rooms, then for all users `U` that are not explicitly forced in-scope by other specs,
`OutOfScope(U,E)` MUST hold, regardless of `Include(U,E)`.

(Include/exclude does not bypass the Rooms gate.)

---

## 3) Required Coupling to Ownership & Publication

### [entity-scopes-05] — Owning client is always in-scope for its client-owned entities
For a client-owned entity `E` with owning client `A`:
- `InScope(A,E)` MUST always hold while `A` is connected.
- Publication and per-user scope filters MUST NOT remove `E` from `A`’s scope.

(This restates the required coupling from `9_entity_ownership.md` / `10_entity_publication.md` as a scope invariant.)

### [entity-scopes-06] — Publication can force non-owners out-of-scope
For client-owned entities, publication state MUST be treated as an additional gate for non-owners:
- If client-owned `E` is Unpublished/Private, then for all `U != Owner(E)`, `OutOfScope(U,E)` MUST hold.

(See `10_entity_publication.md` for publication semantics; this spec defines the scope consequence.)

---

## 4) Scope State Machine & Client-Visible Effects

For each pair `(U,E)` from the server’s perspective, the scope state is exactly one of:
- `InScope(U,E)`
- `OutOfScope(U,E)`

### [entity-scopes-07] — OutOfScope ⇒ despawn on that client
When a client corresponding to user `U` becomes `OutOfScope(U,E)`:
- `E` MUST be despawned on that client (removed from the client’s networked entity pool).

### [entity-scopes-08] — Despawn destroys all components, including local-only components
When `E` despawns on a client due to leaving scope:
- all components associated with `E` in that client’s networked entity pool MUST be destroyed,
  including any local-only components the client may have attached.

### [entity-scopes-09] — OutOfScope ⇒ ignore late replication updates for that entity
If a client receives replication updates for an entity `E` that is currently `OutOfScope` on that client:
- the client MUST ignore them silently in production.
- when diagnostics are enabled, the client MAY emit a warning.

This rule exists to make the protocol tolerant to packet reordering and racey delivery.

### [entity-scopes-10] — InScope ⇒ entity exists in networked entity pool
If a client is `InScope(U,E)`, then `E` MUST exist in that client’s networked entity pool (i.e. be present as a
replicated/spawned entity), subject to normal replication delivery and eventual consistency.

---

## 5) Tick Semantics & Collapse Rules

### [entity-scopes-11] — Scope is resolved per server tick; intermediate states are not observable
The server MUST resolve the final scope state for each `(U,E)` once per server tick and emit only the delta from
the prior tick’s resolved state.

If within one server tick operations would cause `InScope(U,E)` to flip multiple times (e.g. add/remove room membership,
include/exclude toggles), the server MUST collapse to the final resolved state and MUST NOT emit intermediate
spawn/despawn transitions.

### [entity-scopes-12] — Leaving scope for ≥1 tick creates a new lifetime on re-entry
If a client transitions `InScope(U,E) → OutOfScope(U,E)` and remains OutOfScope for at least one full server tick,
then the next transition `OutOfScope(U,E) → InScope(U,E)` MUST be treated by the client as a **fresh spawn lifetime**:
- the entity MUST spawn as if new,
- the client MUST NOT rely on any prior lifetime’s state,
- the server MUST provide an authoritative snapshot baseline for the new lifetime consistent with replication rules.

If the entity leaves and re-enters within the same tick and the server collapses to “still InScope,” then no lifetime
boundary occurs (no observable spawn/despawn).

---

## 6) Disconnect Handling

### [entity-scopes-13] — Disconnect implies OutOfScope for that user for all entities
When a client disconnects (user `U` removed from the server connection set):
- `OutOfScope(U,E)` MUST be treated as holding for all entities `E` immediately.
- The server MUST cease replicating entities to that client.

Note: Separately, `9_entity_ownership.md` defines that client-owned entities are globally despawned when their owning
client disconnects. This spec does not redefine that rule; it defines per-user scope state.

---

## 7) Illegal / Misuse Cases

These cases SHOULD NOT occur in correct usage, but behavior is defined for determinism and safety.

### [entity-scopes-14] — Include/exclude without shared room cannot force scope
If `Include(U,E)` is active but `SharesRoom(U,E) == false`, then `OutOfScope(U,E)` MUST hold.

When diagnostics are enabled, the server MAY emit a warning indicating the include is ineffective due to room gating.

### [entity-scopes-15] — Unknown entity/user references
If the server receives (or internally attempts) a scope operation referencing an unknown entity or unknown user:
- in production, it MUST ignore the operation silently.
- when diagnostics are enabled, it MAY emit a warning.

---

## State Transition Table: Scope

| Current State | Trigger | Preconditions | Next State | Side Effects |
|---------------|---------|---------------|------------|--------------|
| OutOfScope(U,E) | Entity added to shared room | SharesRoom(U,E) becomes true, not Excluded | InScope(U,E) | Spawn E on U's client |
| OutOfScope(U,E) | Include(U,E) called | SharesRoom(U,E) == true | InScope(U,E) | Spawn E on U's client |
| InScope(U,E) | Entity removed from all shared rooms | SharesRoom(U,E) becomes false | OutOfScope(U,E) | Despawn E on U's client |
| InScope(U,E) | Exclude(U,E) called | - | OutOfScope(U,E) | Despawn E on U's client |
| InScope(U,E) | User disconnects | - | OutOfScope(U,E) | Session ends, no despawn event |
| InScope(U,E) | Entity despawned globally | - | (removed) | Despawn E on U's client |

---

## 8) Test obligations (TODO placeholders; not implementing yet)

- **entity-scopes-01/04**: Prove Rooms gating is necessary; roomless entities out-of-scope for non-owners.
- **entity-scopes-02/03**: Prove include/exclude filter works, last call wins, and does not bypass Rooms gate.
- **entity-scopes-05**: Prove owning client always in-scope for its client-owned entities while connected.
- **entity-scopes-06**: Prove Private/Unpublished forces OutOfScope for all non-owners.
- **entity-scopes-07/08**: Prove leaving scope despawns and destroys all components including local-only.
- **entity-scopes-09**: Prove late updates for out-of-scope entities are ignored (warn only when diagnostics enabled).
- **entity-scopes-11**: Prove same-tick flip-flops collapse to final state; no intermediate spawn/despawn.
- **entity-scopes-12**: Prove re-entry after ≥1 tick out-of-scope produces fresh spawn snapshot lifetime.
- **entity-scopes-13**: Prove disconnect implies OutOfScope for that user and replication ceases.

---

## 9) Cross-references

- Ownership: `9_entity_ownership.md`
- Publication: `10_entity_publication.md`
- Replication ordering/wire rules: `8_entity_replication.md`
- Delegation/Authority coupling: `11_entity_delegation.md`, `12_entity_authority.md`
- Events/lifetimes: `13_server_events_api.md`, `14_client_events_api.md`, `15_world_integration.md`


---

<!-- ======================================================================== -->
<!-- Source: 8_entity_replication.md -->
<!-- ======================================================================== -->

# Entity Replication

This spec defines the **client-observable behavior** of Naia’s entity/component replication over the wire:
- entity spawn/despawn as perceived by a client
- replicated component insert/update/remove ordering
- tolerance to packet **reordering**, **duplication**, and **late arrival**
- entity identity across **lifetimes** (scope enter → scope leave, with the ≥1 tick rule)

This spec does **not** define:
- RPC/message semantics (see `4_messaging.md`)
- the internal serialization format
- bandwidth/compression strategies

---

## Glossary

- **Replicated component**: a component type that is part of the Protocol and may be synced over the wire.
- **Local-only component**: a component instance present only in a local World that is not (currently) server-replicated for that entity.
- **Entity lifetime (client-side)**: `scope enter → scope leave`, where re-entering scope after being out-of-scope for **≥ 1 tick** is a **new lifetime** (fresh spawn semantics). See `7_entity_scopes.md`.
- **GlobalEntity**: global identity of an entity across the server’s lifetime (monotonically increasing u64; practical uniqueness).
- **LocalEntity (HostEntity/RemoteEntity)**: per-connection entity handle(s) that may wrap/reuse across lifetimes; must be disambiguated by lifetime rules.

---

### Entity lifetime (client)

For a given client, an entity lifetime is:
`scope enter` → `scope leave`, with the rule that re-entering scope after ≥ 1 tick out-of-scope is a fresh lifetime.

Normative:
- Entity-specific replicated writes (insert/remove/update) MUST be ignored if they refer to an entity outside its current lifetime.
- If an Update arrives before its corresponding Insert due to packet reordering, the Update MUST be buffered until the Insert arrives (or discarded if the lifetime ends first).


## Contract

### [entity-replication-01] — Global identity stability
While an entity exists on the server:
- The entity MUST have a stable **GlobalEntity**.
- The server MUST NOT change an entity’s GlobalEntity during its existence.

When the server despawns the entity:
- That entity ceases to exist. Any future entity with a different lifetime is a different entity, even if some local IDs are reused.

---

### [entity-replication-02] — Client-visible lifetime boundaries
For any given client `C` and entity `E`, Naia MUST model a client-visible **lifetime**:

- Lifetime **begins** when `E` enters `C`’s scope and Naia emits a **Spawn** to `C`.
- Lifetime **ends** when `E` leaves `C`’s scope (including unpublish) and Naia emits a **Despawn** to `C`.
- If `E` re-enters scope after being out-of-scope for **≥ 1 tick**, Naia MUST treat this as a **new lifetime** with **fresh spawn snapshot semantics**.

Cross-link:
- Scope/lifetime rules are defined in `7_entity_scopes.md` and are binding here.

---

### [entity-replication-03] — Spawn snapshot semantics (baseline state)
When `E` enters scope for client `C`, the Spawn sent to `C` MUST include:

- The set of replicated components present on `E` **at the time the Spawn is sent**
- For each included replicated component, the full replicated field state necessary to establish the baseline

Client-side requirement:
- The client MUST be able to materialize the entity’s replicated baseline solely from the Spawn snapshot.

Non-normative note:
- This allows replication to avoid requiring “insert-before-update” for initial state; Spawn is the baseline.

---

### [entity-replication-04] — No observable replication before Spawn
For a given client-visible lifetime of `(C, E)`:

- The client MUST NOT observe any replicated component Insert/Update/Remove for `E` **before** it observes the Spawn for that lifetime.
- If delivery order causes the client to receive component actions before Spawn, Naia MUST ensure those actions are **not observable early** (either by buffering or by deferring application until Spawn becomes available).

This is a hard invariant: **no update-before-spawn** observability.

---

### [entity-replication-05] — Actions outside lifetime are ignored
If the client receives any entity/component replication action referencing an entity lifetime that is not currently active (i.e. before Spawn for that lifetime, or after Despawn for that lifetime):

- Naia MUST ignore the action (it MUST NOT mutate world state).
- In production, this MUST be silent.
- When diagnostics are enabled, Naia MAY emit a warning.

This applies to:
- late packets from a prior lifetime
- reordered packets that arrive after the lifetime ended
- packets referencing entities that are out-of-scope

---

### [entity-replication-06] — Update-before-Insert buffering (within lifetime)
Within an active lifetime:

- If a replicated component **Update** is received before the corresponding replicated component **Insert** has been applied, Naia MUST buffer the Update and apply it after Insert arrives.
- Buffered updates MUST be dropped when the lifetime ends (on Despawn), if they have not been applied.
- Naia MUST NOT apply a buffered Update to an entity/component that belongs to a different lifetime.

The same rule applies symmetrically for any component action that requires the component to exist first (e.g. Remove received before Insert): Naia MUST ensure the action is not misapplied.

---

### [entity-replication-07] — Local-only component overwrite by server replication
If, at the time a replicated component Insert (or Spawn snapshot) is applied, the client already has a **local-only** component instance of the same component type on that entity:

- This overwrite MUST be surfaced as an Insert (replicated-backed component becomes present), even though a local-only instance existed.
- Naia MUST treat the replicated state as authoritative going forward.

Observability rule:
- If a local-only component existed and is overwritten by an incoming server-replicated component Insert (or Spawn snapshot),
  Naia MUST emit a client-visible **Insert** event for that component (presence becomes “replicated-backed”),
  not an Update event.

Cross-link:
- Ownership rules for local-only components vs server-backed replicated components are defined in `9_entity_ownership.md`. This contract ensures replication behavior conforms.

---

### [entity-replication-08] — Collapse to final state per tick (no intermediate transitions)
Within a single server tick, if an entity/component undergoes multiple changes that would otherwise create intermediate states (insert+remove, multiple updates, etc.):

- The server MUST collapse replication to the **final state** for that tick.
- The client MUST NOT be forced to observe intermediate states that did not persist across ticks.

This mirrors the “final state only” principle used in scope transitions.

---

### [entity-replication-09] — Duplicate delivery is idempotent
If the client receives duplicate replication actions (e.g. due to retransmission):

- Applying the same logical action more than once MUST NOT create additional observable effects.
- Naia MUST remain convergent to the server’s final replicated state.

Examples (normative intent):
- duplicate Spawn for an already-spawned active lifetime MUST NOT create a second entity
- duplicate Despawn MUST NOT error
- duplicate Insert/Remove MUST not create oscillation
- duplicate Update MUST not break determinism

---

### [entity-replication-10] — Identity reuse safety (LocalEntity wrap/reuse)
Local entity identifiers (HostEntity/RemoteEntity) may wrap/reuse over time.

Naia MUST ensure:
- Late or reordered replication actions from an old lifetime cannot corrupt a new lifetime, even if LocalEntity IDs are reused.
- Some lifetime-disambiguating information MUST gate applicability of replication actions to the correct lifetime.

Non-normative note:
- A common strategy is to gate by tick boundaries (spawn/despawn tick), but the contract is the invariant: **no cross-lifetime corruption**.

---

### [entity-replication-11] — GlobalEntity rollover is a terminal error
GlobalEntity is treated as effectively unique.

If the server’s monotonic GlobalEntity counter would roll over:
- Naia MUST NOT silently wrap/reuse GlobalEntity values.
- Naia MUST enter a **terminal error mode** (fail-fast / abort / panic), because continued operation would violate identity stability.

This is intentionally strict: rollover is astronomically unlikely and correctness beats availability here.

---

### [entity-replication-12] — Conflict resolution: server wins for replicated state
If a conflict occurs between client-local state and server-replicated state for any replicated component:

- The server’s replicated state MUST overwrite the client’s local state (convergence requirement).

Additional design constraint (to avoid conflicts by construction):
- While an entity is client-owned and not delegated, the server SHOULD NOT originate replicated component mutations for that entity except those derived from accepted owner writes and server-driven lifecycle transitions (scope/publish/delegation/despawn). If it does, the “server wins” rule still applies.

- Delegated authority refinement:
    - For delegated entities, the server’s outbound replicated state remains the canonical convergence source for all clients.
    - While a client holds authority (Granted/Releasing), the server MUST treat the authority holder’s accepted writes as the source for that canonical replicated state (plus lifecycle transitions).
    - Therefore, the server MUST NOT originate independent conflicting replicated component mutations for `E` while a client holds authority.
    - If the server revokes/resets authority, the canonical source may transition back to server-originated state after the reset boundary (see `12_entity_authority.md`).

---

## Test obligations (TODO placeholders)

For each contract above, Naia MUST eventually have at least one E2E test proving it.

- entity-replication-01 — TODO: stable GlobalEntity across lifetime
- entity-replication-02 — TODO: lifetime boundaries; fresh spawn after ≥1 tick out-of-scope
- entity-replication-03 — TODO: Spawn contains full baseline state
- entity-replication-04 — TODO: no observable update/insert/remove before Spawn
- entity-replication-05 — TODO: late/out-of-lifetime actions ignored
- entity-replication-06 — TODO: update-before-insert buffered then applied
- entity-replication-07 — TODO: local-only overwritten by server replication
- entity-replication-08 — TODO: collapse to final per tick; no intermediate states
- entity-replication-09 — TODO: duplicates idempotent
- entity-replication-10 — TODO: LocalEntity reuse cannot corrupt new lifetime
- entity-replication-11 — TODO: GlobalEntity rollover fail-fast (unit-level)
- entity-replication-12 — TODO: server-wins convergence for replicated state

---

## Cross-references

- `7_entity_scopes.md` — defines scope enter/leave semantics and the ≥1 tick lifetime rule
- `10_entity_publication.md` — defines publish/unpublish interactions with scope
- `9_entity_ownership.md` — defines local-only mutation rules and ownership write constraints
- `11_entity_delegation.md` / `12_entity_authority.md` — define delegation and authority semantics
- `14_client_events_api.md` — defines client-observable event ordering/meaning
- `5_time_ticks_commands.md` — defines tick semantics (including wrap considerations)


---

<!-- ======================================================================== -->
<!-- Source: 9_entity_ownership.md -->
<!-- ======================================================================== -->

# Entity Ownership

This spec defines **Entity Ownership**: which actor is permitted to **write** replicated state for an Entity.

Ownership is **not** Delegation, and ownership is **not** Authority. Those are specified elsewhere. Ownership is the coarse, per-entity “who may write replicated updates” rule; Delegation/Authority describe finer-grained permission flows and events.

---

## Definitions

### Mutate vs Write

- **Mutate**: change the local world state by inserting/removing/updating components and/or despawning an entity.
- **Write**: cause a mutation to be **replicated over the wire** (serialized into outbound replication and sent to the remote host).

A mutation may be allowed locally (mutate) while still being forbidden to replicate (write).

### Replicated component vs local-only component

- A **replicated component** is a component type registered for replication in the Protocol.
- A **local-only component** is any component instance that exists only in a local world view and is not currently backed by replicated authority for that entity on that host (even if its type is a replicated type).

Local-only components may exist on entities a host does not own.

### Owner

Ownership is per-entity and exclusive. It is queryable via `entity(...).owner()` on both server and client.

### EntityOwner (ownership-only)

`EntityOwner` is a statement of **who owns the entity**, and it MUST be independent of publication / scope / replication configuration.

- `EntityOwner::Server` — server-owned entity.
- `EntityOwner::Client(UserKey)` — client-owned entity (owned by the specified user).
- `EntityOwner::Local` — local-only entity (never networked; MUST NOT participate in Naia replication, publication, scopes, delegation, or authority).

**Normative:**
- `server.entity(entity).owner()` MUST return only: `Server | Client(UserKey) | Local`.
- `client.entity(entity).owner()` MUST return:
  - `Client(<this client’s UserKey>)` for client-owned entities owned by this client.
  - `Server` for all entities not owned by this client (including entities owned by other clients).
  - `Local` only for local-only entities (which MUST NOT interact with Naia networking).

---

## Core Contracts

### Ownership is per-entity, exclusive, and not per-component
- **entity-ownership-01**: Ownership MUST be defined per-Entity and MUST NOT be defined per-Component.
- **entity-ownership-01**: An Entity MUST have exactly one owner at any moment (exclusive ownership).

### Client-owned entities (server view)
- **entity-ownership-02**: For a **client-owned Entity E**, the server MUST accept **writes** for E only from the owning client and MUST NOT apply writes from any other client.
- **entity-ownership-02**: The server MAY ignore unauthorized writes silently and/or record a metric/log, but MUST NOT apply them.

### Server-owned entities (server view)
- **entity-ownership-03**: For any server-owned entity `E` that is NOT delegated (`replication_config(E) != Some(Delegated)`), the server MUST NOT accept replicated writes from any client for `E`. Such writes MUST be ignored/dropped.
- **entity-ownership-03**: For delegated entities, client writes are governed by `11_entity_delegation.md` / `12_entity_authority.md` (authority holder may write; others must not).
- **entity-ownership-03**: The server MAY ignore unauthorized writes silently and/or record a metric/log, but MUST NOT apply them.

### Ownership does not emit authority events for client-owned entities
- **entity-ownership-04**: Ownership alone MUST NOT emit Authority events for client-owned entities. Authority events are part of Delegation/Authority, not Ownership.

---

## Client-side Safety Rules (Panic Contracts)

### Client must never write without permission
- **entity-ownership-05**: A client MUST NOT write/replicate entity updates unless it is a permitted writer for that entity.
- **entity-ownership-05**: A client is a permitted writer for entity `E` iff:
    - `owner(E) == EntityOwner::Client(this_client)`, OR
    - `replication_config(E) == Some(Delegated)` AND `authority(E) ∈ {Granted, Releasing}`.

- **entity-ownership-05**: If Naia would enqueue/serialize/send a replication write from a client that is not a permitted writer: Naia MUST panic.

Cross-link:
- Delegated authority write permission is defined in `11_entity_delegation.md` / `12_entity_authority.md`.

### Mutate vs Write (ownership gate)

Definitions:
- **Mutate**: local ECS changes (insert/remove/update components, or despawn) that may be purely local.
- **Write**: producing *replication writes* that are sent over the wire (component field updates, insert/remove replication messages, despawn replication messages).

Normative:
- A client MUST be able to **mutate** unowned entities locally (subject to the rules below).
- A client MUST NEVER **write** replication updates for an entity it does not own.
  - If Naia attempts to write for an unowned entity, it MUST panic (this is a framework-internal invariant violation).

Local-only components on unowned entities:
- If a client inserts any component (replicated or non-replicated) onto an unowned entity, and the server never supplies that component for that entity, the component MUST persist locally until:
  - the client removes it (allowed), or
  - the entity despawns (scope-leave/unpublish/etc), which destroys all local-only components.

Unauthorized removal:
- If a client attempts to remove a **replicated component instance** that was originally supplied by the server (i.e., it exists in the entity due to replication), that removal MUST panic.
- If a client removes a component that exists only locally (never supplied by the server for that entity lifetime), that removal MUST be allowed.

Overwrite by later replication:
- If a local-only component exists and later the server begins replicating that component for the entity, the incoming replicated component MUST overwrite the local-only instance.
- This overwrite MUST be treated as a **component Insert** in client events/observability (not an Update).

### Ownership visibility on the client is intentionally coarse
- **entity-ownership-06**: On the client, `entity(...).owner()` MUST return an `EntityOwner` enum.
- **entity-ownership-06**: For the client, any entity not owned by that client MUST be reported as `EntityOwner::Server` (i.e., the client MUST NOT observe “owned by another client”).
- **entity-ownership-06**: Client-owned entities visible to the owning client MUST be reported as `EntityOwner::Client`.
- **entity-ownership-06**: Local-only entities MUST be reported as `EntityOwner::Local`.

 

---

## Mutate vs Write Behavior on Clients (Local Prediction & Local-Only State)

### Non-owners may mutate locally, but must never write
- **entity-ownership-07**: A client MAY mutate entities it does not own (insert/remove/update components), but such mutations MUST NOT write/replicate to the server.
- **entity-ownership-07**: Any replicated updates received from the server for that entity MUST overwrite the client’s local state for the relevant replicated components.

### Local-only components persist until despawn (even if the type is replicated)
- **entity-ownership-08**: If a client inserts a component (replicated or non-replicated type) onto an entity it does not own, and the server never replicates that component for that entity, the component MUST persist locally until removed locally or the entity is despawned/unpublished.
- **entity-ownership-08**: If the server later begins replicating that component for that entity, the newly replicated “official” component state MUST overwrite the existing local-only component state.

### Removing components from unowned entities: allowed only for local-only components
- **entity-ownership-09**: A client MAY remove a component from an unowned entity only if that component instance is local-only on that client.
- **entity-ownership-09**: If a client attempts to remove a component from an unowned entity where that component instance is currently backed by server replication (i.e., it was inserted/maintained by server replication for that entity), Naia MUST panic.

Rationale: removing a server-replicated component locally creates a misleading “phantom delete” that cannot be written, and would be immediately contradicted by subsequent replication.

---

## Ownership Transitions

### Server-owned entities never migrate to client-owned
- **entity-ownership-10**: An entity that is server-owned MUST NOT transition to client-owned at any time.

### Client-owned entities may migrate to server-owned delegated via enabling delegation
- **entity-ownership-11**: A client-owned entity MAY transition to server-owned (delegated) only when delegation is enabled for that entity by:
  - the owning client, or
  - the server (server authority takes priority).
- **entity-ownership-11**: When delegation is enabled for a client-owned entity, ownership MUST transfer from client → server as part of that action.
- **entity-ownership-11**: Once a client-owned entity transfers to server ownership via delegation enabling, it MUST NOT revert back to client ownership.

Note: “delegated” here describes the downstream Authority/permission model; ownership itself is simply “server-owned” after the transfer.

### Ownership and scope

- A client MUST always see its own client-owned entities as in-scope (they MUST NOT be despawned due to scope changes on that owning client).
- For non-owner clients, when an entity leaves scope (unpublish/room divergence/exclude/etc), the entity MUST despawn client-side.

---

## Disconnect Handling

### Owner disconnect despawns all client-owned entities
- **entity-ownership-13**: When a client disconnects, the server MUST despawn all entities owned by that client.
- **entity-ownership-13**: There are no exceptions (delegation/authority do not change this ownership rule).

---

## Out-of-scope / unpublished write attempts

- **entity-ownership-14**: A client MUST NOT write/replicate updates for any entity that it is not a permitted writer for (see `entity-ownership-05`).
- **entity-ownership-14**: Naia MUST guarantee it never attempts to write/replicate for entities that are out-of-scope on that client; if such a write would occur, Naia MUST panic.

Exception note: `EntityProperty` may refer to entities as data (identity/reference semantics). This is a read/reference mechanism and MUST NOT be treated as “writing an entity the client does not own.”

---

## Test Obligations (TODO)

(We are not implementing tests yet; these are placeholders.)

- **entity-ownership-02/03**: Unauthorized client writes MUST NOT affect server state.
- **entity-ownership-05**: Client MUST panic if it would write an unowned entity.
- **entity-ownership-08**: Local-only component persists until despawn; server replication overwrites if it begins replicating later.
- **entity-ownership-09**: Client MUST panic on unauthorized removal of a server-replicated component from an unowned entity.
- **entity-ownership-13**: Owner disconnect despawns all client-owned entities.

---

<!-- ======================================================================== -->
<!-- Source: 10_entity_publication.md -->
<!-- ======================================================================== -->

# Entity Publication

Entity Publication defines the **only valid semantics** for whether a *client-owned* entity may be replicated (spawned/updated) to **non-owning clients**.

Publication is a **gate** layered on top of scoping:
- **Scoping** decides *which* clients are in-scope.
- **Publication** decides whether non-owners are even *eligible* to be in-scope for a client-owned entity.

This spec is intentionally narrow:
- It defines publication as a closed, normative contract.
- It does **not** redefine ownership, scopes, replication, or delegation; it cross-references them.

---

## 1) Scope

### In scope
- Publication states and transitions for **client-owned** entities.
- Required effect of publication on **non-owner scope eligibility**.
- Observable publication state via `replication_config()` on server/client entities.

### Out of scope (defined elsewhere)
- Ownership write acceptance / panics (`9_entity_ownership.md`)
- Scope computation & in-scope/out-of-scope mechanics (`7_entity_scopes.md`)
- Replication ordering / wire semantics (`8_entity_replication.md`)
- Delegation migration & delegated authority (`11_entity_delegation.md`, `12_entity_authority.md`)

---

## 2) Vocabulary

- **Owner(E)**: The owner of entity `E` (see `9_entity_ownership.md`).
- **Owning client A**: A client `A` such that `Owner(E) == A`.
- **Non-owner client C**: A client `C` such that `C != Owner(E)`.
- **InScope(C,E)** / **OutOfScope(C,E)**: defined in `7_entity_scopes.md`.
- **Despawn (client-side)**: `E` is removed from the client’s networked entity pool (and all of its components in that pool are destroyed).
- **Publication state (client-owned only)**:
  - **Published**: the server MAY scope `E` to non-owners (subject to scope policy).
  - **Unpublished**: the server MUST NOT scope `E` to any non-owner.

### Observable: ReplicationConfig
Naia exposes an observable replication configuration via `replication_config() -> Option<ReplicationConfig>` and a setter `configure_replication(ReplicationConfig)` on server & client entity handles.

This spec defines how `ReplicationConfig::{Private,Public,Delegated}` maps onto publication semantics **only for client-owned entities**.

---

## 3) Contract (Rules)

### [entity-publication-01] — Publication gates only client-owned visibility to non-owners
Publication semantics apply only to **client-owned** entities as a gate for **non-owner** visibility.
This spec does not impose additional constraints on server-owned entities beyond what `7_entity_scopes.md` / `8_entity_replication.md` specify.

### [entity-publication-02] — Unpublished client-owned entities are never in-scope for non-owners
If `E` is client-owned and **Unpublished** with owner `A`:
- for all clients `C != A`, `OutOfScope(C,E)` MUST hold.

### [entity-publication-03] — Published client-owned entities may be in-scope for non-owners
If `E` is client-owned and **Published** with owner `A`:
- the server MAY place `E` into scope of clients `C != A` per normal scope policy.

### [entity-publication-04] — Only the server or owning client may change publication; server wins conflicts
Only the server OR the owning client MAY cause `E` to transition:
- Unpublished ↔ Published

If the server and owning client produce conflicting publication changes “in the same effective replication window”
(e.g. within one server tick / one resolved change-set), the server’s final resolved publication state MUST win.

Notes:
- There is no requirement that publication transitions are exposed as a public API; they MAY be system-driven.
- This rule defines *authority to cause the transition*, not how the API is shaped.

### [entity-publication-05] — Unpublish forces immediate OutOfScope for all non-owners
When client-owned `E` transitions **Published → Unpublished**:
- all non-owner clients MUST become `OutOfScope(C,E)` for `C != Owner(E)` as part of the next resolved scope update.

### [entity-publication-06] — Publish enables later scoping; does not guarantee scoping
When client-owned `E` transitions **Unpublished → Published**:
- the server MAY later scope `E` to non-owners per policy;
- publication does not itself guarantee that any particular non-owner becomes in-scope.

### [entity-publication-07] — Owning client is always in-scope for its owned entities
For any client-owned entity `E` with owner `A`:
- `InScope(A,E)` MUST always hold.
- Publication MUST NOT remove `E` from the owning client’s scope.

(If the entity ceases to exist—e.g. it is despawned—this rule no longer applies.)

### [entity-publication-08] — Non-owner unpublish/out-of-scope implies despawn and destroys local-only components
If a non-owner client `C != Owner(E)` transitions to `OutOfScope(C,E)` due to publication becoming Unpublished:
- `E` MUST despawn on that client (be removed from the client’s networked entity pool).
- All components attached to `E` in that client’s pool (including any “local-only” components) MUST be destroyed.

This is intentionally aligned with the general “OutOfScope ⇒ despawn” rule in `7_entity_scopes.md`;
publication is just one cause of OutOfScope.

### [entity-publication-09] — Publication MUST be observable via replication_config
For a client-owned entity `E` that exists on the server:
- `Published` MUST correspond to `replication_config(E) == Some(Public)`
- `Unpublished` MUST correspond to `replication_config(E) == Some(Private)`

For a non-owner client `C != Owner(E)`:
- If `E` exists in the client’s networked entity pool, then `replication_config(E)` MUST NOT be `Some(Private)`.
  (Because `Some(Private)` would mean Unpublished, which must be OutOfScope for non-owners.)

### [entity-publication-10] — Delegation migration ends “client-owned publication” semantics
If a client-owned entity `E` migrates into a **delegated server-owned entity** (see `11_entity_delegation.md`):
- `E` is no longer client-owned, and publication semantics in this spec no longer apply.
- Non-owners are no longer gated by “Published/Unpublished client-owned rules”; the entity is now governed by
  server-owned scoping + delegated rules.

Cross-constraint (restated for coherence; the detailed rule lives in `11_entity_delegation.md`):
- A client-owned entity MUST be Published before it may migrate into delegated server-owned form.

---

## 4) Illegal cases & required behavior

This section exists to prevent “undefined behavior pockets.” These situations MUST NOT occur in correct Naia usage,
but if they do occur due to a bug or misuse, behavior is still defined.

### [entity-publication-11] — If a non-owner observes a client-owned Private entity, it MUST be treated as OutOfScope
If a non-owner client `C != Owner(E)` ever reaches a state where:
- `E` exists in the client’s networked entity pool AND `replication_config(E) == Some(Private)`

then the client MUST immediately treat `E` as `OutOfScope(C,E)` and despawn it.

Rationale: this restores the invariant required by entity-publication-02/09 without relying on perfect server behavior.

---

## State Transition Table: Publication (Client-Owned Entities)

| Current State | Trigger | Who Can Trigger | Next State | Effect on Non-Owners |
|---------------|---------|-----------------|------------|----------------------|
| Unpublished | configure_replication(Public) | Owner or Server | Published | MAY enter scope per policy |
| Published | configure_replication(Private) | Owner or Server | Unpublished | MUST exit scope immediately |
| Published | configure_replication(Delegated) | Owner or Server | (Delegated) | Ownership transfers to server |
| (any) | Owner disconnects | (automatic) | (despawned) | Entity despawned globally |

---

## 5) Test obligations (TODO placeholders; not implementing yet)

- **entity-publication-02**: Prove unpublished client-owned entities never appear for non-owners.
- **entity-publication-05/08**: Prove Published→Unpublished forces non-owner despawn, destroying local-only components.
- **entity-publication-06**: Prove Unpublished→Published does not guarantee any non-owner in-scope.
- **entity-publication-07**: Prove owning client always retains in-scope visibility across publication toggles.
- **entity-publication-09**: Prove `replication_config` accurately reflects Published/Public and Unpublished/Private.
- **entity-publication-10**: Prove delegated migration requires Published first and then switches to delegated semantics.
- **entity-publication-11**: Prove the client self-heals by despawning if it ever sees `Private` on a non-owned entity.

---

## 6) Cross-references

- Ownership: `9_entity_ownership.md`
- Scopes: `7_entity_scopes.md`
- Replication ordering/wire behavior: `8_entity_replication.md`
- Delegation & authority: `11_entity_delegation.md`, `12_entity_authority.md`


---

<!-- ======================================================================== -->
<!-- Source: 11_entity_delegation.md -->
<!-- ======================================================================== -->

# Entity Delegation

Entity Delegation defines how a **server-owned delegated entity** grants temporary **Authority** to clients so that
exactly one client at a time may **write** replicated updates for that entity.

Delegation is distinct from:
- **Ownership**: who ultimately owns the entity (see `9_entity_ownership.md`).
- **Publication**: whether client-owned entities are visible to non-owners (see `10_entity_publication.md`).
- **Scope**: whether an entity exists on a client at all (see `7_entity_scopes.md`).
- **Replication**: spawn/update ordering and lifetime rules (see `8_entity_replication.md`).

This spec defines:
- the meaning of the `Delegated` replication configuration
- authority arbitration (request/grant/deny/release)
- required client/server behavior and observability

---

## 1) Glossary

- **Delegated entity**: a server-owned entity configured as `ReplicationConfig::Delegated`.
- **Authority holder**: the single actor currently allowed to **write** replicated updates for the delegated entity.
  The authority holder is either:
    - the server, or
    - exactly one client.
- **Authority status (client view)**: an `EntityAuthStatus` value that expresses a client’s current permission state
  with respect to writing:
    - `Available`: no one is currently holding authority; client may request.
    - `Requested`: client requested authority; not yet granted.
    - `Granted`: client currently holds authority and may write.
    - `Releasing`: authority is in the process of being released; writing may still be permitted until release finalizes.
    - `Denied`: authority is currently held by another client or by the server, so this client cannot request/grab it.

Non-normative note:
- The shared delegation state machine includes `can_mutate()` and `can_write()` distinctions; clients can mutate earlier
  than they can write. This spec defines the observable meaning of those states.

---

## 2) Core Model

### [entity-delegation-01] — Delegation applies only to server-owned delegated entities
Authority delegation semantics apply only when:
- the entity is server-owned, and
- `replication_config(E) == Some(Delegated)`.

If an entity is not delegated, this spec’s authority arbitration does not apply.

### [entity-delegation-02] — Single-writer invariant
For any delegated entity `E`, at any time:
- at most one client MAY be the authority holder for `E`.
- the server MAY reset/revoke authority at any time (see `12_entity_authority.md`).
- the server MAY hold authority (server-as-holder) which forces all clients to observe `Denied`.
- while a client holds authority (Granted/Releasing), the server MUST NOT originate independent replicated component writes for `E`; the server’s replicated state MUST be derived from the current authority holder’s accepted writes plus server-driven lifecycle transitions.

Client-visible implication:
- exactly one client can have `EntityAuthStatus::Granted` at a time for a given delegated entity.

### [entity-delegation-03] — Authority is scoped: only in-scope clients participate
Only clients for which `InScope(U,E)` holds MAY request authority for `E`.

If a client is out-of-scope for `E`, it MUST NOT request authority for `E` and MUST NOT be granted authority for `E`.

---

## 3) Entering Delegation (Migration)

### [entity-delegation-04] — Client-owned → server-owned delegated migration requires Published
A client-owned entity MUST be Published/`Public` before it may migrate into a server-owned delegated entity.

(Ownership/publication constraints are defined in `9_entity_ownership.md` and `10_entity_publication.md`;
this rule is restated here as a delegation precondition.)

### [entity-delegation-05] — Migration grants authority to previous owner
When a client-owned, Published entity `E` migrates into a server-owned delegated entity:
- ownership transfers to the server (per `9_entity_ownership.md`).
- the previous owner client MUST immediately become the authority holder.
- on that previous owner client, `EntityAuthStatus(E)` MUST be `Granted`.

When a client-owned, published entity migrates to server-owned delegated, the previous owner client MUST immediately start with `EntityAuthStatus::Granted` for that entity.

Rationale:
- delegation migration should not create a behavior cliff for the former owner.

---

## 4) Authority Arbitration (Request/Grant/Deny/Release)

### [entity-delegation-06] — First request wins
If `E` is delegated and currently has no client authority holder (i.e., authority is `Available`):
- the first in-scope client to request authority MUST be granted authority.
- while a client holds authority, no other client may be granted authority until it is released or reset.

### Authority requests are NOT queued

Normative:
- If a delegated entity’s authority is currently held by some holder (client or server),
  then **the server MUST NOT transfer authority to a different client** just because that client requests authority.
- Requests made while authority is held MUST resolve as `Denied` for the requester (i.e., “someone else holds it”).
- When the current holder releases authority (or the server revokes/releases it), the entity becomes `Available`.
  - Other clients do NOT automatically receive authority.
  - A client MUST call `request_authority()` again while `Available` to obtain authority.

### [entity-delegation-07] — Meaning of Denied
For a client `C` and delegated entity `E`:
- `EntityAuthStatus(C,E) == Denied` MUST mean: authority is currently held by another client OR by the server.
- A client in `Denied` status MUST remain denied until authority is released or reset by the holder or the server,
  at which point the status MUST transition back to `Available`.

This is not a “request rejection” outcome; it is a “currently unavailable” outcome.

### [entity-delegation-08] — Requested means pending; no writes allowed
When a client requests authority and is in `Requested`:
- the client MAY mutate locally (prediction/local prep) but MUST NOT write replicated updates.
- if Naia would attempt to write while in `Requested`, it MUST panic.

### [entity-delegation-09] — Granted means writes allowed; single writer enforced
When a client is in `Granted` for delegated entity `E`:
  - that client MAY write replicated updates for `E`.
  - all other clients MUST be in `Denied` for `E` (or `Available` only if not tracking the entity’s status explicitly).
  - While a client is `Granted`/`Releasing` for `E`, the authority holder is the sole origin of replicated component updates for `E`.
  - The server MUST NOT attempt to ‘override’ by sending conflicting component writes while the client holds authority.
  - If the server needs to override, it MUST first reset/revoke authority (`entity-authority-10`), optionally become the holder (`entity-authority-09`), and then replicate its authoritative state.

### [entity-delegation-10] — Releasing means writes may still occur until release finalizes
When a client enters `Releasing`:
- the client MAY continue to write replicated updates until the release is finalized,
  after which it MUST become `Available`.
- other clients MUST remain `Denied` until the release finalizes and authority becomes `Available`.

### [entity-delegation-11] — Release transitions authority back to Available
If the authority holder releases authority (or the server releases/resets it):
- the authority state MUST become `Available`.
- all clients that were `Denied` due to another holder MUST transition to `Available`.

---

## 5) Client Safety (Panic Contracts)

### [entity-delegation-12] — Client must never write without permission
If Naia would enqueue/serialize/send a replication write for a delegated entity `E` from a client that is not permitted
to write (`EntityAuthStatus != Granted/Releasing`):
- Naia MUST panic.

This is a hard invariant: Naia framework controls writing and must enforce this strictly.

---

## 6) Scope/Disconnect Interactions

### [entity-delegation-13] — Losing scope ends client authority
If a client that holds authority for `E` becomes out-of-scope for `E`:
- authority MUST be released/reset by the server.
- other in-scope clients MUST transition to `Available` (subject to first-request wins on new requests).

Cross-link:
- Scope transitions and despawn semantics are defined in `7_entity_scopes.md`.

### [entity-delegation-14] — Disconnect releases authority
If the authority-holding client disconnects:
- the server MUST release/reset authority for `E`.
- other in-scope clients MUST transition to `Available`.

If the disconnected client also owned client-owned entities, those are despawned globally per `9_entity_ownership.md`.
This rule concerns only delegated server-owned entities.

---

## 7) Observability (Events & Queryability)

### [entity-delegation-17] — Delegation observability

Delegation MUST be observable through:
- `replication_config(E) == Some(Delegated)` (server + client observable)
- authority status and events (defined in `12_entity_authority.md` and the events API specs)

This spec defines the required semantics; the concrete event types and delivery guarantees are specified in:
- `13_server_events_api.md`
- `14_client_events_api.md`
- `12_entity_authority.md`

---

## 8) Illegal / Misuse Cases

### [entity-delegation-15] — Requesting authority while out-of-scope is ignored (warn in diagnostics)
If a client requests authority for `E` while out-of-scope:
- server MUST ignore the request silently in production.
- server MAY emit a warning when diagnostics are enabled.

### [entity-delegation-16] — Conflicting reconfiguration is resolved by server final state
If configuration changes (e.g., toggling Delegated on/off) would produce conflicting intermediate states within a tick:
- the server MUST collapse to the final resolved state per tick, consistent with `8_entity_replication.md` and
  `7_entity_scopes.md`.
- clients MUST observe only the final state transitions (no intermediate oscillations).

---

## 9) Test obligations (TODO placeholders; not implementing yet)

- entity-delegation-04: migration requires Published
- entity-delegation-05: migration grants authority to previous owner client
- entity-delegation-06: first request wins; others denied
- entity-delegation-07/11: denied persists until release; release returns others to Available
- entity-delegation-08/12: write while not allowed panics
- entity-delegation-10: releasing allows writes until finalized
- entity-delegation-13/14: losing scope/disconnect releases authority and unblocks others
- entity-delegation-15: out-of-scope requests ignored (warn only in diagnostics)
- entity-delegation-16: same-tick collapse yields final-only observability

---

## 10) Cross-references

- Ownership: `9_entity_ownership.md`
- Publication: `10_entity_publication.md`
- Scopes: `7_entity_scopes.md`
- Replication: `8_entity_replication.md`
- Authority & events: `12_entity_authority.md`, `13_server_events_api.md`, `14_client_events_api.md`


---

<!-- ======================================================================== -->
<!-- Source: 12_entity_authority.md -->
<!-- ======================================================================== -->

# Entity Authority

Entity Authority defines how a client can acquire and release the right to **write replicated updates** for a
**server-owned delegated** entity, and what each side can observe about that right.

Authority is distinct from:
- **Ownership** (see `9_entity_ownership.md`): who ultimately owns the entity
- **Delegation** (see `11_entity_delegation.md`): how delegated entities arbitrate authority (first-request wins)
- **Scope** (see `7_entity_scopes.md`): whether the entity exists on the client
- **Replication** (see `8_entity_replication.md`): ordering/lifetime/reordering semantics

This spec defines:
- the authority state machine (`EntityAuthStatus`)
- client request/release semantics (including optimistic `Requested`)
- server-controlled authority (server as a holder; server override/reset)
- required behavior for illegal cases, duplicates, and out-of-scope conditions

---

## 1) Definitions

### Authority applies only to delegated entities
Authority exists only for entities where:
- `replication_config(E) == Some(Delegated)` (see `11_entity_delegation.md` / `10_entity_publication.md`)

### EntityAuthStatus (client-visible)

Client-visible authority statuses for a delegated entity:
- `Available`: no client holds authority; a client may request it.
- `Requested`: this client has requested authority and is awaiting the server’s decision (optimistic pending).
- `Granted`: this client currently holds authority.
- `Releasing`: this client has initiated release and is awaiting server confirmation.
- `Denied`: authority is currently held by some other client or by the server.

Derived capabilities (per endpoint, per entity):
- `can_write` (replication writes) is true iff:
  - the endpoint owns the entity (client-owned), OR
  - the entity is delegated AND this endpoint is the active authority holder (`Granted` or `Releasing`), OR
  - the endpoint is the server AND it is explicitly holding authority (server-forced denial mode).
- `can_read` (apply incoming replicated updates from the peer) is true iff the endpoint is NOT the active authority holder.

Therefore:
- If status is `Granted` or `Releasing`: `can_read = false`.
- If status is `Available`, `Requested`, or `Denied`: `can_read = true`.

`can_read = false` means the authority holder MUST NOT apply incoming *replicated component updates* from the peer for `E`; it does NOT prevent observing authority-control transitions (`Granted`/`Denied`/`Reset`) or lifecycle events (spawn/despawn), which must still be processed.

Normative safety:
- If Naia attempts to write while `can_write = false`, it MUST panic.

### Diagnostics enabled
When diagnostics are enabled, Naia MAY emit warnings on illegal/bug states; otherwise it MUST remain silent
in production.

---

## 2) Core Contracts

### [entity-authority-01] — Authority is defined only for delegated entities
For any entity `E`:
- If `replication_config(E) != Some(Delegated)`, then `authority(E)` MUST be `None` on clients (no authority state).
- Any attempt to request or release authority on a non-delegated entity MUST return an error (see below).

### [entity-authority-02] — Single-writer rule (client-side)
For any delegated entity `E` and a given client `C`:
- `C` MUST only be permitted to **write** replicated updates for `E` when `EntityAuthStatus(C,E)` is:
    - `Granted`, or
    - `Releasing` (until release finalizes)

For all other statuses (`Available`, `Requested`, `Denied`):
- if Naia would enqueue/serialize/send a replication write for `E`, it MUST panic.

This is a hard invariant: Naia controls writing and must enforce it strictly.

### [entity-authority-03] — Meaning of Denied
For a delegated entity `E` as observed by a client `C`:
- `Denied` MUST mean: authority is currently held by another client OR by the server.
- While `Denied`, the client MUST NOT be granted authority until the current holder releases or the server resets.
- When authority becomes available again, `Denied` MUST transition to `Available`.

This is not “you asked and were rejected”; it is “currently unavailable.”

---

## 3) Client API Semantics (Request / Release)

### [entity-authority-04] — request_authority() is optimistic: Available → Requested immediately
If a client calls `request_authority(E)` for a delegated entity `E` and the client is eligible (in-scope, etc.):
- the client MUST transition locally from `Available` → `Requested` immediately (optimistic pending),
  without waiting for a server round-trip.

### Request resolution

- Calling `request_authority()` MUST immediately set local status to `Requested` (optimistic pending).
- The server MUST resolve the request:
  - If authority is `Available`, the first request received wins and becomes `Granted`.
  - If authority is held by someone else (client or server), the requester MUST become `Denied` (no queue).

### [entity-authority-05] — request_authority() completion transitions
After `Requested`, the client MUST eventually observe one of:

- `Requested → Granted` if the server grants authority
- `Requested → Denied` if authority is held by another client or by the server (first-request-wins arbitration)
- `Requested → Available` if the server resets authority globally (e.g., server override) before granting

The client MUST NOT remain permanently in `Requested` unless the entity/lifetime ends (see scope/lifetime rules).

### [entity-authority-06] — release_authority() transitions: Granted → Releasing → Available
If the client currently holds authority:
- `release_authority(E)` MUST transition `Granted → Releasing` immediately (local optimistic),
- and MUST eventually finalize to `Available` after the server processes release.

If the client is `Requested` and calls `release_authority(E)`:
- it MUST cancel its request locally and transition to `Available`.
- the server MAY ignore the cancellation if it never observed the request; behavior must remain convergent.

### [entity-authority-07] — Client-side error returns (Result semantics)
`request_authority(E)` and `release_authority(E)` MAY return errors. At minimum:

- If `replication_config(E) != Some(Delegated)`: MUST return an error (e.g., `ErrNotDelegated`)
- If the entity is out-of-scope on this client: MUST return an error (e.g., `ErrNotInScope`)
- If the entity does not exist in the client’s current lifetime: MUST return an error (e.g., `ErrNoSuchEntity`)

Errors MUST NOT mutate authority status except where explicitly specified (e.g., cancel Requested on release).

Non-normative note:
- Even with client-side errors, the server must still be safe if it receives invalid requests; see §6.

---

## 4) Server Semantics (Grant / Reset / Server as Holder)

### [entity-authority-08] — First-request wins arbitration (delegation law)
Authority arbitration MUST follow the rules defined in `11_entity_delegation.md`:
- first eligible request wins
- others remain denied until release/reset

Authority spec defines the client-observable status transitions and events resulting from that law.

### [entity-authority-09] — Server may hold authority and block clients
The server MAY act as an authority holder for a delegated entity.

If the server is holding authority for `E`:
- all clients observing `E` MUST be in `Denied` for `E` (except a client currently in `Requested`, which must
  transition to `Denied` once the server state is observed/applied).

### [entity-authority-10] — Server override/reset
The server MAY reset authority for a delegated entity at any time.

When the server resets authority for `E`:
- any client in `Granted` or `Releasing` MUST transition to `Available` (authority revoked)
- any client in `Denied` MUST transition to `Available`
- any client in `Requested` MUST transition to `Available` (request cleared)

This is the server’s “break glass” control.

---

## 5) Scope, Lifetime, and Disconnect Interactions

### [entity-authority-11] — Out-of-scope ends authority for that client
If a client becomes out-of-scope for delegated entity `E` (or the entity despawns due to publication/scope):
- the client MUST treat the entity’s lifetime as ended
- any authority status for that entity MUST be cleared (entity no longer exists locally)
- any pending buffered actions for that entity MUST be discarded (see `8_entity_replication.md`)

### [entity-authority-12] — Authority holder losing scope forces global release/reset
If the authority-holding client loses scope for `E` (or disconnects):
- the server MUST release/reset authority for `E`
- other in-scope clients MUST transition from `Denied` to `Available`

(Exact timing is per replication tick semantics; clients must converge.)

### [entity-authority-13] — Delegation disable clears authority
If an entity stops being delegated (`replication_config` changes away from `Delegated`):
- authority MUST become `None` on all clients for that entity
- any pending `Requested` MUST be cleared
- any `Granted/Releasing` MUST be revoked (server wins)

---

## 6) Illegal / Misuse Cases (Robustness)

### [entity-authority-14] — Out-of-scope requests are ignored server-side
If the server receives an authority request for `(U,E)` while `OutOfScope(U,E)`:
- in production, it MUST ignore it silently
- when diagnostics are enabled, it MAY emit a warning

This complements client-side `ErrNotInScope`. The system must remain safe even if invalid requests occur.

### [entity-authority-15] — Duplicate/late authority signals are idempotent
Authority grant/reset signals may be duplicated or reordered.

Clients MUST:
- not emit duplicate observable “grant” effects for the same lifetime
- converge to the server’s final resolved authority state
- ignore authority signals for entities not in the active lifetime (see `8_entity_replication.md`)

---

## 7) Observability (Events)

### [entity-authority-16] — Authority observability

Authority changes MUST be observable via:
- `authority()` (status) while the entity is delegated and in the client’s lifetime
- client/server events as defined in `14_client_events_api.md` and `13_server_events_api.md`

This spec defines semantics, not exact event names. At minimum, the event layer MUST be able to represent:
- "authority granted to this client for entity E"
- "authority reset/revoked for entity E"

---

## State Transition Table: EntityAuthStatus

| Current State | Trigger | Preconditions | Next State | can_write | can_read |
|---------------|---------|---------------|------------|-----------|----------|
| Available | request_authority() | InScope(C,E) | Requested | false | true |
| Requested | Server grants | First request wins | Granted | true | false |
| Requested | Server denies | Another holds | Denied | false | true |
| Requested | Server resets | - | Available | false | true |
| Granted | release_authority() | - | Releasing | true | false |
| Granted | Server resets/revokes | - | Available | false | true |
| Granted | Lose scope | - | (cleared) | - | - |
| Releasing | Server confirms | - | Available | false | true |
| Denied | Holder releases | - | Available | false | true |
| Denied | Server resets | - | Available | false | true |

---

## 8) Test obligations (TODO placeholders; not implementing yet)

- entity-authority-01: authority exists only for delegated entities (None otherwise)
- entity-authority-02: writing without permission panics
- entity-authority-04/05: Available→Requested optimistic; Requested resolves to Granted/Denied/Available appropriately
- entity-authority-06: release transitions Granted→Releasing→Available; cancels Requested
- entity-authority-07: request/release return errors for not delegated / not in scope / no entity
- entity-authority-09/10: server can hold authority; server reset clears all client states
- entity-authority-12: holder scope-loss/disconnect releases authority and unblocks others
- entity-authority-13: delegation disable clears authority and revokes grants
- entity-authority-14: out-of-scope requests ignored server-side (warn only in diagnostics)
- entity-authority-15: duplicates/reordering are idempotent and lifetime-safe

---

## 9) Cross-references

- Delegation: `11_entity_delegation.md`
- Ownership: `9_entity_ownership.md`
- Scopes & lifetimes: `7_entity_scopes.md`
- Replication ordering/lifetime gating: `8_entity_replication.md`
- Events: `13_server_events_api.md`, `14_client_events_api.md`, `15_world_integration.md`

---

<!-- ======================================================================== -->
<!-- Source: 13_server_events_api.md -->
<!-- ======================================================================== -->

# Server Events API

This spec defines the **only** valid semantics for the server-side Events API surface: what is collected, when it becomes observable, how it is drained, and what ordering/duplication guarantees exist.

Normative keywords: **MUST**, **MUST NOT**, **MAY**, **SHOULD**.

Related specs:
- `8_entity_replication.md` (spawn/update/remove/despawn semantics)
- `7_entity_scopes.md` (in-scope vs out-of-scope and snapshot behavior)
- `4_messaging.md` (message ordering, reliability, request/response semantics)
- `5_time_ticks_commands.md` (tick definition, wrap ordering, command timing model)
- `2_connection_lifecycle.md` (connect/disconnect/auth ordering + cleanup)

---

## Glossary

- **Events API**: The server-facing interface that buffers and exposes observable happenings (auth/connect/disconnect, world mutations, messages, requests).
- **World events**: Events that describe replicated-world changes and inbound app-level messages (spawn/despawn, insert/update/remove, message/request/response).
- **Tick events**: Events that describe connection/tick/session-level happenings (auth/connect/disconnect, tick-related meta events if any).
- **Receive step**: The act of ingesting available packets from the transport into Naia’s internal packet buffer.
- **Process step**: The act of processing all buffered packets, applying protocol semantics, and producing new pending events.
- **Drain**: Reading events from the API such that they are removed from the pending queue (pure read+remove).
- **In scope**: A user is considered a recipient for an entity only if `InScope(user, entity)` per `entity_scopes`.
- **Tick**: Server simulation tick as defined in `5_time_ticks_commands.md`. (Wrap-safe ordering applies.)

---

## API boundary model (normative)

This spec standardizes the server loop boundary as:

1) `receive_all_packets()`  (Receive step)
2) `process_all_packets()`  (Process step)
3) `take_tick_events()` and/or `take_world_events()` (Drain steps)

The *names* above reflect the current API. The **semantics** below are the contract.

### [server-events-00] — Receive step is ingestion only
- The Receive step MUST only ingest packets into an internal buffer.
- The Receive step MUST NOT advance tick, mutate the world, or produce observable events directly.

### [server-events-01] — Process step is the only event-production boundary
- New events MUST become pending/observable only as a result of the Process step.
- If no Process step occurs, drains MUST NOT “discover” new events.

### [server-events-02] — Drains are pure read+remove
- `take_world_events()` and `take_tick_events()` MUST be pure drains:
  - MUST NOT receive packets
  - MUST NOT process packets
  - MUST NOT advance tick
  - MUST have no side effects other than removing the drained events from the pending queue

---

## Contracts

### [server-events-03] — Drain operations are destructive and idempotent (no replay without new Process step)
**Rule**
- Each drain call MUST remove the returned events from the pending buffer.
- Repeating the same drain call again **without any intervening Process step that produced new pending events** MUST return empty.
- This MUST hold even if drains are called multiple times within the same server tick.

**Notes**
- “Idempotent” here means “subsequent drains see nothing,” not “same payload returned.”

**Test obligations**
- `server-events-03.t1` (TODO) Given one insert+update+remove becomes pending, When draining twice without another Process step, Then first drain returns expected events and second drain returns none.
- `server-events-03.t2` (TODO) Given no new pending events, When calling all drains, Then all are empty.

---

### [server-events-04] — Event types are partitioned; no cross-contamination
**Rule**
- World mutation events MUST NOT appear in message/request streams.
- Message/request streams MUST NOT appear in world mutation streams.
- Tick/session events (auth/connect/disconnect) MUST NOT appear in world mutation streams.

**Test obligations**
- `server-events-04.t1` (TODO) Given mixed activity (spawn + message + request + connect), When draining each category, Then each appears only in the correct stream.

---

### [server-events-05] — Auth/connect/disconnect ordering is stable and exactly-once per session transition
**Rule**
- For each connection attempt when auth is enabled:
  - exactly one auth decision event MUST be exposed
  - if accepted, exactly one connect event MUST be exposed after auth for that session
  - if rejected, a connect event MUST NOT occur for that attempt
- For each session termination:
  - exactly one disconnect event MUST be exposed
  - duplicate lower-level disconnect signals MUST NOT duplicate the disconnect event

**Test obligations**
- `server-events-05.t1` (TODO) `require_auth=true`, valid credentials → auth event occurs before connect.
- `server-events-05.t2` (TODO) invalid credentials → auth event occurs, connect does not.
- `server-events-05.t3` (TODO) duplicate disconnect signals → exactly one disconnect event.

---

### [server-events-06] — Disconnect cleanup is consistent with scope + ownership contracts
**Rule**
- After a disconnect is observed, the server MUST have cleaned up all per-connection scoped state attributable solely to that session (no “ghost” scoped entities for that user).
- Additionally, ownership cleanup MUST follow `9_entity_ownership.md` (client-owned entities despawn when owner disconnects).

**Test obligations**
- `server-events-06.t1` (TODO) Disconnect while scoped → scope membership removed.
- `server-events-06.t2` (TODO) Disconnect owner → owned entities are despawned (ownership contract).

---

### [server-events-07] — Entity spawn/enter events: per user, in-scope only, exactly-once
**Rule**
- When an entity `E` enters scope for user `U` (including initial join snapshot), the World events stream MUST expose exactly one spawn/enter event for `(U, E)`.
- Spawn/enter events MUST be emitted only for users for which `InScope(U, E)` becomes true.
- Spawn/enter events MUST NOT be emitted for out-of-scope users.

**Test obligations**
- `server-events-07.t1` (TODO) E becomes in-scope for A but not B → only A gets spawn/enter.
- `server-events-07.t2` (TODO) Late join snapshot → spawn/enter for all in-scope entities exactly once.

---

### [server-events-08] — Component insert/update/remove: per user and per component, no duplicates
**Rule**
- For each user `U` with `InScope(U, E)` at the time the change becomes observable:
  - inserting component `C` on `E` MUST produce exactly one insert event for `(U, E, C)`
  - updating MUST produce exactly one update event per underlying applied update
  - removing MUST produce exactly one remove event per underlying removal
- Duplicate packets/retries MUST NOT create duplicate events unless they cause a new applied transition.

**Test obligations**
- `server-events-08.t1` (TODO) One update replicated to two users → two update events, no duplicates.
- `server-events-08.t2` (TODO) Insert then update then remove in same tick → each appears exactly once.

---

### [server-events-09] — Despawn/leave-scope events are exactly-once and end that user’s lifecycle
**Rule**
- When `E` leaves scope for `U` (scope change or true despawn), the World events stream MUST expose exactly one despawn/exit event for `(U, E)`.
- After `(U, E)` has exited, the server MUST NOT surface further insert/update/remove events for `(U, E, *)` unless `E` re-enters scope for `U` as a new lifecycle (per `7_entity_scopes.md` + `8_entity_replication.md`).

**Test obligations**
- `server-events-09.t1` (TODO) Despawn while in scope → exit once; no further component events for that lifecycle.
- `server-events-09.t2` (TODO) Leave scope then re-enter after ≥1 tick → fresh spawn/enter event.

---

### [server-events-10] — No “component events before spawn/enter” for any user
**Rule**
- For any user `U`, the World events stream MUST NOT surface insert/update/remove events for entity `E` before `U` has observed spawn/enter for `E`.
- Under reordering/duplication, internal buffering is allowed, but the API-visible ordering MUST respect this invariant.

**Test obligations**
- `server-events-10.t1` (TODO) Under simulated reorder, assert no insert/update/remove for `(U, E)` is observed before spawn/enter for `(U, E)`.

---

### [server-events-11] — Message events: grouped by channel and message type; each yields sender + payload; drain once
**Rule**
- Inbound messages MUST be exposed via typed message events grouped by:
  - **channel type** and
  - **message type**
- Iteration MUST yield the sender user key and the decoded message payload.

(Example shape: `world_events.read::<MessageEvent<Channel, Msg>>() -> (user_key, msg)`.)

Additional requirements:
- Each inbound delivered message MUST appear exactly once to the application across drains.
- Messages MUST be decoded to the correct message type per protocol configuration and MUST NOT be misrouted to the wrong channel/type.

**Test obligations**
- `server-events-11.t1` (TODO) Multiple senders + channels → correct channel/type grouping; each yields correct sender; each appears once.
- `server-events-11.t2` (TODO) Mixed message types → decoded to correct type and not misrouted.

---

### [server-events-12] — Request/response events: exactly-once surfacing, correct matching, drain once
**Rule**
- For each incoming request accepted by the protocol layer, the server MUST surface exactly one corresponding request event/handle to the application.
- Any response matching MUST be correct per `4_messaging.md` and MUST NOT surface duplicates under retransmit/duplication.
- Draining request/response events MUST be destructive and MUST NOT replay already-drained items.

**Test obligations**
- `server-events-12.t1` (TODO) One request → exactly one server-visible request event.
- `server-events-12.t2` (TODO) Duplicate packets → still exactly one request event.

---

### [server-events-13] — API misuse safety: drains MUST NOT panic
**Rule**
- Calling any drain method at any time (including when empty) MUST NOT panic.
- Empty drains MUST return empty.

**Test obligations**
- `server-events-13.t1` (TODO) Call drains repeatedly in an empty world; assert empties and no panic.

---

## Forbidden behaviors

- Producing new observable events during drains (drains must be pure).
- Replaying already-drained events without an intervening Process step producing new pending events.
- Emitting component events for `(U, E)` before spawn/enter for `(U, E)`.
- Emitting entity/component events for out-of-scope users.
- Duplicating auth/connect/disconnect events for a single session transition.
- Misrouting messages to the wrong channel/type or losing sender attribution.
- Panicking on empty drains or repeated drains.

## Test obligations

TODO: Define test obligations for this specification.


---

<!-- ======================================================================== -->
<!-- Source: 14_client_events_api.md -->
<!-- ======================================================================== -->

# Client Events API Contract

This document defines the **only** valid semantics for the client-side Events API: what events exist, when they become observable, how they are drained, ordering guarantees, and behavior under reordering/duplication/scope changes/disconnects.

Normative keywords: **MUST**, **MUST NOT**, **MAY**, **SHOULD**.

---

## Glossary

- **Client Events API**: The public interface by which a client drains replicated-world events (spawns, despawns, component changes, messages, request/response).
- **World events**: Events describing the client’s replicated world changes and inbound app-level messages.
- **Tick events**: Events describing connection/tick/session-level happenings (if any are exposed to the client).
- **Receive step**: Ingesting packets from the transport into Naia’s internal packet buffer.
- **Process step**: Processing all buffered packets, applying protocol semantics, and producing new pending events / applying replicated state changes.
- **Drain**: Reading events such that they are removed from the pending queue (pure read+remove).
- **Tick**: Client tick as defined in `5_time_ticks_commands.md`. (Wrap-safe ordering applies.)
- **InScope(C,E)** / **OutOfScope(C,E)**: Whether entity `E` exists in client `C`’s local world (see `7_entity_scopes.md`).
- **Entity lifetime**: scope enter → scope leave, with the ≥1 tick out-of-scope rule (see entity suite).

---

## Cross-References

- Tick + time model: `5_time_ticks_commands.md`
- Identity, replication legality, and "no updates before spawn / none after despawn": `8_entity_replication.md`
- Scope transitions, join snapshots, and scope leave/re-enter semantics: `7_entity_scopes.md`
- Messaging ordering/reliability: `4_messaging.md`
- Ownership/delegation/authority semantics (not defined here): `9_entity_ownership.md`, `11_entity_delegation.md`, `12_entity_authority.md`

---

## API boundary model (normative)

This spec standardizes the client loop boundary as:

1) `receive_all_packets()`  (Receive step)
2) `process_all_packets()`  (Process step)
3) `take_tick_events()` and/or `take_world_events()` (Drain steps)

The *names* above reflect the current API. The **semantics** below are the contract.

### [client-events-00] — Receive step is ingestion only
- The Receive step MUST only ingest packets into an internal buffer.
- The Receive step MUST NOT directly mutate the client world or produce observable events.

### [client-events-01] — Process step is the only event-production / world-application boundary
- Replicated state application and new pending events MUST occur only as a result of the Process step.
- Drains MUST NOT “discover” new events unless a prior Process step produced them.

### [client-events-02] — Drains are pure read+remove
- `take_world_events()` and `take_tick_events()` MUST be pure drains:
  - MUST NOT receive packets
  - MUST NOT process packets
  - MUST NOT advance tick
  - MUST have no side effects besides removing drained events from the pending queue

---

## Contracts

### [client-events-03] — Drain is destructive and idempotent (no replay without new Process step)
**Rule:** Draining a given event stream MUST remove those events from the pending queue, and subsequent drains without an intervening Process step producing new pending events MUST return empty.

- Draining twice “back-to-back” MUST NOT return the same event twice.
- Draining does not advance time/tick and does not trigger receive/process.

**Test obligations:**
- `TODO: client_events_api::drain_is_destructive_and_idempotent_no_replay`

---

### [client-events-04] — Spawn is the first event for an entity lifetime on that client
**Rule:** For any entity `E` that becomes present on client `C`, the first observable entity-lifetime event for that lifetime MUST be `Spawn(E)` (or an equivalent spawn event). The client MUST NOT observe component Update/Remove events for `E` before Spawn for that lifetime.

- Initial component presence delivered with the spawn snapshot MAY be represented as:
  - (a) Spawn + a batch of Insert events, or
  - (b) Spawn carrying a snapshot, with zero inserts,
    as long as the model is consistent and tests assert the chosen model.
- Under packet reordering/duplication, the API MUST still prevent “update-before-spawn” observability.

**Test obligations:**
- `TODO: client_events_api::no_update_or_remove_before_spawn_under_reordering`

---

### [client-events-05] — No events for entities that were never in scope
**Rule:** If `E` is never `InScope(C,E)` for client `C` during a connection lifetime, the client Events API MUST not emit any entity events for `E` (no spawn/insert/update/remove/despawn).

This includes entities created and destroyed entirely while `C` is out of scope.

**Test obligations:**
- `TODO: client_events_api::no_events_for_never_in_scope_entities`

---

### [client-events-06] — Despawn ends the entity lifetime; no further events for that lifetime
**Rule:** After `Despawn(E)` is emitted for client `C`, the Events API MUST NOT emit any further entity-related events for that lifetime of `E` on `C`.

- Late packets referencing the despawned lifetime MUST be ignored safely (see `8_entity_replication.md`).
- If `E` later re-enters scope as a new lifetime under the scope model, that is a new Spawn and a new lifetime.

**Test obligations:**
- `TODO: client_events_api::no_events_after_despawn_under_reordering`

---

### [client-events-07] — Component insert/update/remove are one-shot per applied change
**Rule:** When a component change is applied to an entity `E` on client `C`, the Events API MUST surface exactly one corresponding event for that applied change.

- Insert: exactly once when a component becomes present on `E`
  - If a replicated-backed component replaces a local-only component instance of the same type, the Events API MUST emit an Insert event (not Update) for that transition.
- Update: exactly once per distinct applied update
- Remove: exactly once when a component is removed from `E`

Duplicate packets or retries MUST NOT cause duplicate events if they do not cause a new applied state transition.

**Test obligations:**
- `TODO: client_events_api::component_insert_update_remove_are_one_shot`

---

### [client-events-08] — Per-entity ordering: spawn → (inserts/updates/removes)* → despawn
**Rule:** For a given entity lifetime on client `C`, the API-visible ordering MUST respect:

`Spawn(E)` happens before any component events for that lifetime, and `Despawn(E)` happens after all component events for that lifetime.

This is an observability constraint: internal buffering/reordering is allowed, but the Events API must never violate this ordering.

**Test obligations:**
- `TODO: client_events_api::per_entity_ordering_is_never_violated`

---

### [client-events-09] — Scope transitions are reflected as spawn/despawn (with the defined model)
**Rule:** When an entity `E` transitions between OutOfScope and InScope on client `C`, the client Events API MUST reflect that transition using spawn/despawn semantics consistent with `7_entity_scopes.md`.

- Leaving scope MUST cause Despawn(E) (entity removed from client world).
- Re-entering scope MUST cause Spawn(E) with a coherent snapshot, consistent with the identity/lifetime model.

**Test obligations:**
- `TODO: client_events_api::scope_leave_reenter_emits_spawn_despawn_consistently`

---

### [client-events-10] — Message events are typed, correctly routed, and drain once
**Rule:** Client message events:
- MUST be exposed via typed message events grouped by:
  - channel type, and
  - message type
- Iteration MUST yield the sender identity (server or user depending on channel direction semantics) and the decoded payload.

(Example shape: `world_events.read::<MessageEvent<Channel, Msg>>() -> (sender, msg)`.)

Additional requirements:
- MUST be drained exactly once (no duplicates on repeated drains).
- MUST NOT be emitted for messages not actually delivered (e.g., dropped unreliable traffic).
- Ordering/reliability constraints are defined in `4_messaging.md`; this contract covers API surfacing correctness + drain semantics.

**Test obligations:**
- `TODO: client_events_api::message_events_are_typed_routed_and_one_shot`

---

### [client-events-11] — Request/response events are matched, one-shot, and cleaned up on disconnect
**Rule:** If the client exposes request/response events via its Events API:
- Each delivered request/response MUST be surfaced exactly once and drain cleanly.
- Responses MUST be matchable to the originating request handle/ID per the public API.
- On disconnect with in-flight requests, the client MUST follow the defined failure behavior and MUST NOT leak request tracking state (see `4_messaging.md`).

**Test obligations:**
- `TODO: client_events_api::request_response_events_are_one_shot_and_matched`
- `TODO: client_events_api::in_flight_requests_fail_cleanly_on_disconnect`

---

### [client-events-12] — Authority events are out of scope for this spec
**Rule:** Authority-related events MUST follow `12_entity_authority.md`. This spec does not define them, except:

- If authority events are surfaced through the same drain mechanism, they MUST obey drain semantics (no duplicates) as per this spec.

**Test obligations:**
- `TODO: client_events_api::authority_events_obey_drain_semantics_without_duplicates`

---

## Forbidden behaviors

- Producing new observable events during drains (drains must be pure).
- Replaying already-drained events without an intervening Process step producing new pending events.
- Emitting Update or Remove before Spawn for an entity lifetime.
- Emitting entity events for an entity never in scope.
- Emitting entity events after Despawn for that lifetime.
- Misrouting message events to the wrong channel/type.
- Panicking on empty drains or repeated drains.

## Test obligations

TODO: Define test obligations for this specification.


---

<!-- ======================================================================== -->
<!-- Source: 15_world_integration.md -->
<!-- ======================================================================== -->

# World Integration Contract

This spec defines the only valid semantics for integrating Naia’s replicated state into an external “game world” (engine ECS, custom world, adapter layer), on both server and client.

Normative keywords: **MUST**, **MUST NOT**, **SHOULD**, **MAY**.

---

## Scope

This spec covers:
- How Naia delivers world mutations (spawn/despawn, component insert/update/remove) to an external world implementation.
- Ordering and “exactly-once” expectations per tick/drain.
- Integration lifecycle: connect, disconnect, scope in/out, join-in-progress, reconnect.
- Misuse safety requirements at the integration boundary (no panics, defined no-ops/errors).

This spec does **not** define:
- The replication rules themselves (see `8_entity_replication.md`).
- Scope policy semantics (see `7_entity_scopes.md`).
- Ownership/delegation/authority rules (see `9_entity_ownership.md`, `11_entity_delegation.md`, `12_entity_authority.md`).
- Messaging and request/response (see `4_messaging.md`).
- Transport behavior (see `3_transport.md`).

Related specs:
- `8_entity_replication.md`
- `7_entity_scopes.md`
- `13_server_events_api.md`
- `14_client_events_api.md`

---

## Terms

- **External World**: The user/engine-owned state container that mirrors Naia’s view (ECS, scene graph, entity-component store).
- **Integration Adapter**: Code that takes Naia events/mutations and applies them to the External World.
- **Naia World View**: The authoritative state Naia believes exists (server world; or client local world scoped per-client).
- **World Mutation**: One of: Spawn, Despawn, ComponentInsert, ComponentUpdate, ComponentRemove.
- **Tick**: The discrete step at which Naia advances and produces mutations/events.
- **Drain**: A single pass where the integration adapter consumes the available Naia events/mutations for a tick (or for a poll loop iteration).
- **In Scope**: An entity is present in the client's Naia World View (see `7_entity_scopes.md`).

---

## Contracts

### [world-integration-01] — World mirrors Naia view

For any participant `P` (server or client), if an External World is integrated, it MUST converge to exactly the Naia World View for `P` as mutations are drained and applied.

- Entities present in Naia view MUST exist in External World after applying all mutations through that tick.
- Entities absent in Naia view MUST NOT exist in External World after applying all mutations through that tick.
- For each entity, the set of components and their values MUST match Naia view after applying all mutations through that tick.

Test obligations:
- `world-integration-01.t1` (TODO → `test/tests/world_integration.rs::server_world_integration_stays_in_lockstep`)
  - Given a fake server External World wired to the integration adapter; when server spawns/inserts/updates/removes/despawns across ticks; then fake world matches Naia server view each tick.
- `world-integration-01.t2` (TODO → `test/tests/world_integration.rs::client_world_integration_stays_in_lockstep_with_scope`)
  - Given two clients with scope changes; when entities enter/leave scope and update; then each client External World matches that client’s Naia local view.

---

### [world-integration-02] — Mutation ordering is deterministic per tick

Within a single tick and for a single entity `E`, the integration adapter MUST apply mutations in a deterministic, valid order:

1) Spawn(E) (if E becomes present this tick)
2) ComponentInsert(E, X) (initial or newly added components)
3) ComponentUpdate(E, X) (updates to existing components)
4) ComponentRemove(E, X)
5) Despawn(E) (if E becomes absent this tick)

Constraints:
- ComponentInsert/Update/Remove MUST NOT be applied to an entity that is not present in External World at that moment.
- Despawn MUST occur after all other mutations for that entity in that tick.

This contract concerns integration application order; Naia’s event production rules are defined elsewhere.

Test obligations:
- `world-integration-02.t1` (TODO → `test/tests/world_integration.rs::per_tick_order_spawn_then_components_then_despawn`)
  - Given a tick where E spawns and receives inserts/updates; then the integration adapter can apply in the valid order without needing retries or panics.
- `world-integration-02.t2` (TODO → `test/tests/world_integration.rs::remove_before_despawn_in_same_tick_is_safe_and_deterministic`)
  - Given E has a component removed and E despawns in the same tick; then adapter applies remove then despawn deterministically.

---

### [world-integration-03] — Exactly-once delivery per drain

For a given participant `P`, each discrete world mutation produced by Naia MUST be consumable exactly once by the integration adapter.

- If the adapter drains mutations/events for a tick, and then drains again without advancing tick, the second drain MUST be empty for that mutation set.
- Duplicate deliveries MUST NOT occur in the integration API surface for the same mutation.

Notes:
- This is about the integration-facing drain semantics (the same principle as `server_events_api` / `client_events_api`), not about transport-level retransmits.

Test obligations:
- `world-integration-03.t1` (TODO → `test/tests/world_integration.rs::drain_is_one_shot_no_duplicates_server`)
- `world-integration-03.t2` (TODO → `test/tests/world_integration.rs::drain_is_one_shot_no_duplicates_client`)

---

### [world-integration-04] — Scope changes map to spawn/despawn in External World

On clients, scope governs presence. The integration adapter MUST reflect scope transitions as:

- When an entity `E` transitions OutOfScope → InScope for client `C`, the External World for `C` MUST receive a Spawn(E) (or equivalent "create entity") and initial component inserts sufficient to form a coherent snapshot. (Snapshot semantics are defined in `7_entity_scopes.md` and `8_entity_replication.md`.)
- When `E` transitions InScope → OutOfScope for client `C`, the External World for `C` MUST receive a Despawn(E) (or equivalent “remove entity”).

Test obligations:
- `world-integration-04.t1` (TODO → `test/tests/world_integration.rs::scope_enter_creates_entity_and_components_as_snapshot`)
- `world-integration-04.t2` (TODO → `test/tests/world_integration.rs::scope_leave_removes_entity_no_ghosts`)

---

### [world-integration-05] — Join-in-progress and reconnect yield coherent External World

If a client joins late or reconnects, the External World MUST be reconstructed purely from current server state and current scope, not from stale client-local leftovers.

- On reconnect, the External World MUST NOT retain entities/components from the prior disconnected session.
- After initial snapshot application, the External World MUST match the client’s Naia World View.

Test obligations:
- `world-integration-05.t1` (TODO → `test/tests/world_integration.rs::late_join_builds_world_from_snapshot_only`)
- `world-integration-05.t2` (TODO → `test/tests/world_integration.rs::reconnect_clears_old_world_and_rebuilds_cleanly`)

---

### [world-integration-06] — Stable identity mapping at the integration boundary

The integration adapter MUST treat Naia’s entity identity as stable for the lifetime the entity is present in the Naia World View.

- If Naia indicates the “same entity” across ticks (same logical identity), the External World MUST keep the same external handle for that entity (or maintain an injective mapping).
- If an entity despawns and later a different entity appears, the adapter MUST NOT accidentally alias them as the same external entity.

This relies on identity semantics in `8_entity_replication.md`; this contract ensures the adapter doesn't break identity.

Test obligations:
- `world-integration-06.t1` (TODO → `test/tests/world_integration.rs::no_identity_aliasing_across_lifetimes`)
- `world-integration-06.t2` (TODO → `test/tests/world_integration.rs::same_logical_entity_keeps_same_external_mapping`)

---

### [world-integration-07] — Component type correctness

For every component mutation surfaced to the adapter, the component type MUST be correct and match the protocol/schema.

- The adapter MUST NOT be asked to apply a component mutation of a different type than declared.
- If a component cannot be decoded due to schema mismatch or decode failure, behavior MUST follow `3_transport.md` / protocol contracts (e.g., reject connection or safely ignore that mutation), and the adapter MUST NOT panic.

Test obligations:
- `world-integration-07.t1` (TODO → `test/tests/world_integration.rs::component_types_are_correct_and_never_misrouted`)
- `world-integration-07.t2` (TODO → `test/tests/world_integration.rs::decode_failure_does_not_panic_external_world`)

---

### [world-integration-08] — Misuse safety: no panics, defined failures

The integration boundary MUST be robust to reasonable misuse:

- Applying a mutation for an entity not present MUST NOT panic; it MUST be a no-op or a defined error surfaced to the caller (implementation choice, but MUST be consistent).
- Applying a component update for a missing component MUST NOT panic; it MUST be a no-op or defined error.
- Re-applying the same mutation due to caller mistake MUST NOT corrupt state; it MUST be rejected/no-op deterministically.

This is about adapter-facing safety, not about hiding logic bugs inside Naia.

Test obligations:
- `world-integration-08.t1` (TODO → `test/tests/world_integration.rs::missing_entity_update_is_safe`)
- `world-integration-08.t2` (TODO → `test/tests/world_integration.rs::missing_component_update_is_safe`)
- `world-integration-08.t3` (TODO → `test/tests/world_integration.rs::double_apply_is_safe_and_deterministic`)

---

### [world-integration-09] — Zero-leak lifecycle cleanup

Across repeated connect/disconnect cycles and scope churn, the integration adapter MUST allow External World to reach a clean empty state when Naia’s view is empty.

- After disconnect, External World MUST contain no entities belonging to that connection/session.
- After all clients disconnect (or server clears its world), External World MUST be empty.

Test obligations:
- `world-integration-09.t1` (TODO → `test/tests/world_integration.rs::disconnect_cleans_world_fully`)
- `world-integration-09.t2` (TODO → `test/tests/world_integration.rs::long_running_cycles_do_not_leak_external_entities`)

---

## Notes for Implementers

- For server integration, the External World is typically updated from server-side inserts/updates/removes/despawns (see `13_server_events_api.md`).
- For client integration, the External World is typically updated from client-side world events (see `14_client_events_api.md`), and scope governs presence (`7_entity_scopes.md`).
- This spec is satisfied whether the adapter is “push” (callbacks) or “pull” (drain + apply), as long as contracts above hold.

## Test obligations

TODO: Define test obligations for this specification.


---

