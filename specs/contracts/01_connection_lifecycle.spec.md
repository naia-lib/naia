# Connection Lifecycle

Last updated: 2026-01-08

## Purpose

This spec defines the **connection state machine** and **observable events** for Naia connections.

It is intentionally written at the Naia core API level. Engine adapters (hecs/bevy) MUST preserve these semantics; adapter-specific plumbing is out of scope.

---

## Glossary

- **Client**: a Naia client instance attempting to establish and maintain a session with a Server.
- **Server**: a Naia server instance accepting client sessions.
- **Transport**: the underlying network mechanism (e.g. UDP, WebRTC). Transport-specific mechanics are defined in `02_transport.spec.md`, but lifecycle semantics are defined here.
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

**Obligations:**
- **t1**: Contract behavior is correct

Client behavior MUST be describable by the above conceptual states, even if the implementation uses different internal states.

### [connection-02] —

**Obligations:**
- **t1**: Contract behavior is correct

The client MUST NOT expose a public “Rejected” connection state. Rejection is an event (RejectEvent), not a persistent state.

### Server states (per-client-session conceptual)

- **NoSession**
- **Handshaking**
- **Connected**

### [connection-03] —

**Obligations:**
- **t1**: Contract behavior is correct

The server MUST NOT treat a client as “Connected” (for purposes of entity replication, message delivery, tick semantics, etc.) until the handshake is finalized including tick sync.

---

## Authentication & identity tokens

### `require_auth = false`

### [connection-04] —

**Obligations:**
- **t1**: Contract behavior is correct

If `require_auth = false`, the server MUST allow clients to attempt connection without any pre-auth step.

### [connection-05] —

**Obligations:**
- **t1**: Contract behavior is correct

Implementations MAY still support optional application-level auth, but it must not be required by Naia for connection establishment when `require_auth = false`.

### `require_auth = true`

This mode uses an out-of-band HTTP auth step and a one-time identity token.

#### Pre-transport auth request (HTTP)

### [connection-06] —

**Obligations:**
- **t1**: Contract behavior is correct

When `require_auth = true`, a client MUST obtain a server-issued identity token via an out-of-band auth request (HTTP) BEFORE initializing the transport connection attempt.

### [connection-07] —

**Obligations:**
- **t1**: Contract behavior is correct

The server MUST evaluate the auth request and return either:
- `200 OK` (accepted) with an identity token, or
- `401 Unauthorized` (rejected) with no identity token.

### [connection-08] —

**Obligations:**
- **t1**: Contract behavior is correct

When the server receives an auth request in this mode, it MUST emit exactly one `AuthEvent` for that request.

### [connection-09] —

**Obligations:**
- **t1**: Contract behavior is correct

There is no Naia-level “auth timeout” during the transport handshake, because auth is completed before the transport session begins.

#### Identity token properties

### [connection-10] —

**Obligations:**
- **t1**: Contract behavior is correct

An identity token MUST be:
- **One-time use** (cannot be used successfully more than once), and
- **Time-limited** with TTL = **1 hour** from issuance.

### [connection-11] —

**Obligations:**
- **t1**: Contract behavior is correct

If a token is expired, already-used, or invalid, the server MUST explicitly reject the connection attempt (see “Explicit rejection”).

### [connection-12] —

**Obligations:**
- **t1**: Contract behavior is correct

Identity tokens MUST be required for **all transports** when `require_auth = true` (not only WebRTC).

### [connection-13] —

**Obligations:**
- **t1**: Contract behavior is correct

On first successful validation attempt, the server MUST mark the token as used (consumed). Replays MUST fail.

---

## Transport handshake & tick sync

### [connection-14] —

**Obligations:**
- **t1**: Contract behavior is correct

A successful connection handshake MUST include a tick synchronization step. A client MUST NOT be considered "Connected" until tick sync completes.

### [connection-14a] — protocol_id check during handshake

**Obligations:**
- **t1**: protocol_id check during handshake works correctly

The connection handshake MUST verify **`protocol_id`** (see Protocol Identity section below) as the first protocol-level check.

**Handshake ordering:**
1. Transport connection established
2. **`protocol_id` exchange and comparison** ← HARD GATE (see `connection-31`)
3. Auth validation (if `require_auth = true`)
4. Tick synchronization
5. `ConnectEvent` emitted (connection ready)

If `protocol_id` does not match, the server MUST reject with `ProtocolMismatch` before proceeding to any further handshake steps.

### [connection-15] —

**Obligations:**
- **t1**: Contract behavior is correct

The client MUST emit `ConnectEvent` only at the moment the handshake is finalized (including protocol identity verification and tick sync).

### [connection-16] —

**Obligations:**
- **t1**: Contract behavior is correct

The server MUST emit `ConnectEvent` only at the moment the handshake is finalized (including protocol identity verification and tick sync).

### [connection-17] —

**Obligations:**
- **t1**: Contract behavior is correct

Naia MUST NOT deliver any entity replication "writes" as part of an established session until after `ConnectEvent` is emitted for that session (server-side), and the client MUST NOT apply any such writes until after it has emitted `ConnectEvent`.

(See `04_time_ticks_commands.spec.md` for tick semantics and how tick sync interacts with command history.)

---

## Explicit rejection

### [connection-18] —

**Obligations:**
- **t1**: Contract behavior is correct

The server MUST explicitly reject a connection attempt when:
- `require_auth = true` and the client presents no identity token,
- the presented token is invalid/expired/already-used,
- the server otherwise chooses to deny the attempt before session establishment.

### [connection-19] —

**Obligations:**
- **t1**: Contract behavior is correct

When the server explicitly rejects:
- The client MUST emit `RejectEvent`.
- The client MUST NOT emit `ConnectEvent`.
- The client MUST NOT emit `DisconnectEvent` (because it was never connected).

### [connection-20] —

**Obligations:**
- **t1**: Contract behavior is correct

After a `RejectEvent`, the client’s public `ConnectionStatus` MUST be (or return to) a non-connected state (e.g. Disconnected), with no special “Rejected” status.

---

## Disconnect semantics

### [connection-21] —

**Obligations:**
- **t1**: Contract behavior is correct

`DisconnectEvent` (client-side) MUST only be emitted if the client previously emitted `ConnectEvent` for the session.

### [connection-22] —

**Obligations:**
- **t1**: Contract behavior is correct

`DisconnectEvent` (server-side) MUST only be emitted if the server previously emitted `ConnectEvent` for the session.

### [connection-23] —

**Obligations:**
- **t1**: Contract behavior is correct

When a client disconnects (or is disconnected) after session establishment:
- It is treated as immediately out-of-scope for all entities, and
- Any client-owned entities owned by that client MUST be despawned by the server.
(See `08_entity_ownership.spec.md` and `06_entity_scopes.spec.md`.)

---

## Event ordering guarantees

### Successful session (require_auth = true)

### [connection-24] —

**Obligations:**
- **t1**: Contract behavior is correct

For a single successful connection where `require_auth = true`, the server MUST observe events in this order:
1. `AuthEvent`
2. `ConnectEvent`
3. `DisconnectEvent` (eventually)

### Successful session (require_auth = false)

### [connection-25] —

**Obligations:**
- **t1**: Contract behavior is correct

For a single successful connection where `require_auth = false`, the server MUST observe:
1. `ConnectEvent`
2. `DisconnectEvent` (eventually)

### Client-side ordering (all modes)

### [connection-26] —

**Obligations:**
- **t1**: Contract behavior is correct

For a single successful session, the client MUST observe:
1. `ConnectEvent`
2. `DisconnectEvent` (eventually)

### [connection-27] —

**Obligations:**
- **t1**: Contract behavior is correct

For a rejected attempt, the client MUST observe:
1. `RejectEvent`
…and MUST NOT observe `ConnectEvent` or `DisconnectEvent` for that attempt.

---

## Reconnect semantics

### [connection-28] — Reconnect is a fresh session

**Obligations:**
- **t1**: Reconnect is a fresh session works correctly

When a client "reconnects" (disconnects and connects again):
- This is a **fresh connection** that builds world state from a new snapshot
- Session resumption is **out of scope** unless explicitly specified in a future spec
- The server treats the reconnecting client as a new session
- Any prior entity state, authority, buffered data from the previous session is discarded on the server

**Implications:**
- Client-owned entities from the previous session were despawned on disconnect (per `08_entity_ownership.spec.md`)
- The client receives fresh entity spawns for all in-scope entities
- Authority state starts fresh (no carryover from previous session)
- Pending requests/responses from previous session are not resumed

**Observable signals:**
- `ConnectEvent` on successful reconnect (same as first connect)
- Fresh entity spawns (not updates from previous state)

**Test obligations:**
- `connection-28.t1`: Reconnecting client receives fresh entity spawns
- `connection-28.t2`: Previous session authority does not carry over

---

## Protocol Identity

This section defines the **protocol identity** mechanism that gates all Naia connections.

### Definitions

- **Protocol crate**: the shared Rust crate that defines the message/component/channel registry via `Protocol::builder()`.
- **`protocol_id`**: a deterministic 128-bit identifier for the compiled protocol crate's wire-relevant surface.
- **Wire-relevant surface**: any aspect of the protocol that affects encoding, decoding, or message semantics on the wire.

---

### [connection-29] — protocol_id definition

**Obligations:**
- **t1**: protocol_id definition works correctly

Every protocol crate MUST compute a single `protocol_id` value that uniquely identifies its wire-relevant surface.

**`protocol_id` MUST be derived from:**
- Channel registry: channel kinds, modes, directions, and registration order
- Message type registry: type IDs, field schemas, field order, registration order
- Request/Response type registry: type IDs, field schemas, registration order
- Component type registry: type IDs, field schemas, replicated field order, registration order
- Naia wire protocol version

**Stability guarantee:**
- `protocol_id` MUST be **deterministic**: identical protocol crate source with identical dependencies MUST produce the same `protocol_id` across rebuilds.
- `protocol_id` MUST **change** if any wire-relevant surface changes.
- `protocol_id` MAY remain the same if only non-wire-relevant changes occur (e.g., documentation, non-replicated fields, internal refactoring).

**Observable signals:**
- `protocol_id` is queryable at runtime via protocol API

**Test obligations:**
- `connection-29.t1`: Different channel registrations produce different `protocol_id`
- `connection-29.t2`: Different component schemas produce different `protocol_id`
- `connection-29.t3`: Same protocol crate produces same `protocol_id` across builds
- `connection-29.t4`: Non-wire-relevant changes do not change `protocol_id`

---

### [connection-30] — protocol_id wire encoding

**Obligations:**
- **t1**: protocol_id wire encoding works correctly

**Fixed-width encoding:**
- `protocol_id` MUST be encoded as a **16-byte (128-bit) unsigned integer** (`u128`).
- Wire encoding MUST use **little-endian** byte order.

**Handshake exchange:**
- Both client and server MUST send their `protocol_id` during the handshake.
- The exchange occurs before any other protocol-level data.

**Test obligations:**
- `connection-30.t1`: `protocol_id` is encoded as 16 bytes little-endian on wire

---

### [connection-31] — protocol_id handshake gate

**Obligations:**
- **t1**: protocol_id handshake gate works correctly

**Timing:**
Protocol identity comparison MUST occur during the handshake, BEFORE:
1. `ConnectEvent` is emitted on either side
2. Entity replication begins
3. Any messages are delivered
4. Auth validation (if `require_auth = true`)

**Handshake ordering (updated from connection-14a):**
1. Transport connection established
2. **`protocol_id` exchange and comparison** ← HARD GATE
3. Auth validation (if `require_auth = true`)
4. Tick synchronization
5. `ConnectEvent` emitted (connection ready)

**Mismatch behavior:**
If `protocol_id` values do not match:
- Server MUST reject the connection immediately
- Client MUST receive a **`ProtocolMismatch`** error/event (distinct from other rejection reasons)
- Client MUST NOT emit `ConnectEvent`
- Client MUST NOT emit `DisconnectEvent` (connection was never established)
- The rejection MUST occur before any further handshake steps

**Error classification (per `00_common.spec.md`):**
- Protocol mismatch is a **deployment configuration error**, not a runtime error
- No panic occurs; connection fails with clear `ProtocolMismatch` indication

**Observable signals:**
- `RejectEvent` on client with `ProtocolMismatch` reason
- No `ConnectEvent` on either side

**Test obligations:**
- `connection-31.t1`: Mismatched `protocol_id` causes `ProtocolMismatch` rejection
- `connection-31.t2`: Matched `protocol_id` allows connection to proceed
- `connection-31.t3`: `ProtocolMismatch` is distinguishable from other rejection reasons

---

### [connection-32] — What affects protocol_id

**Obligations:**
- **t1**: What affects protocol_id works correctly

The following aspects are **wire-relevant** and MUST affect `protocol_id`:

| Aspect | Affects `protocol_id` |
|--------|----------------------|
| Channel count | Yes |
| Channel kinds (type IDs) | Yes |
| Channel modes | Yes |
| Channel directions | Yes |
| Channel registration order | Yes |
| Message type count | Yes |
| Message type IDs | Yes |
| Message field schemas | Yes |
| Message registration order | Yes |
| Request/Response type count | Yes |
| Request/Response type IDs | Yes |
| Request/Response field schemas | Yes |
| Component type count | Yes |
| Component type IDs | Yes |
| Component field schemas | Yes |
| Replicated field order | Yes |
| Component registration order | Yes |
| Naia wire protocol version | Yes |

**Consequence:** Any change to the above requires a new `protocol_id` and will cause existing clients to fail connection.

**Test obligations:**
- `connection-32.t1`: Each wire-relevant change produces different `protocol_id`

---

### [connection-33] — No partial compatibility

**Obligations:**
- **t1**: No partial compatibility works correctly

**Strict matching:**
- There is NO extension negotiation
- There is NO partial compatibility mode
- There is NO version range acceptance
- Either `protocol_id` matches exactly, or connection is rejected

**Upgrade path:**
When protocol changes require breaking compatibility:
- Old clients MUST be rejected by new servers
- Old servers MUST reject new clients
- Application layer MUST handle gradual rollout (parallel servers, feature flags, etc.)

**Test obligations:**
- `connection-33.t1`: Breaking protocol change causes `ProtocolMismatch`

---

## Non-goals / Out of scope

- The exact HTTP route(s), headers, or body format of the auth request.
- Transport-specific wire details for how the token is conveyed.
- Engine adapter (bevy/hecs) implementation details.
- Retry/backoff policies for repeated connection attempts (may be defined in a future spec if needed).
- Session resumption / state persistence across reconnects.
- Wire format details for protocol identity exchange.

## Test obligations

Summary of test obligations from contracts above:

**Authentication & Identity:**
- `connection-06.t1`: Auth request required before transport when `require_auth = true`
- `connection-10.t1`: Token is one-time use
- `connection-10.t2`: Token expires after TTL

**Handshake & Events:**
- `connection-14.t1`: Tick sync completes before ConnectEvent
- `connection-14a.t1`: `protocol_id` verified before ConnectEvent
- `connection-15.t1`: Client emits ConnectEvent only after handshake complete
- `connection-16.t1`: Server emits ConnectEvent only after handshake complete

**Rejection:**
- `connection-18.t1`: Missing token causes rejection when required
- `connection-19.t1`: Rejected client emits RejectEvent, not ConnectEvent

**Disconnect:**
- `connection-21.t1`: Client DisconnectEvent only after ConnectEvent
- `connection-22.t1`: Server DisconnectEvent only after ConnectEvent
- `connection-23.t1`: Client-owned entities despawned on disconnect

**Reconnect:**
- `connection-28.t1`: Reconnecting client receives fresh entity spawns
- `connection-28.t2`: Previous session authority does not carry over

**Protocol Identity:**
- `connection-29.t1`: Different channel registrations produce different `protocol_id`
- `connection-29.t2`: Different component schemas produce different `protocol_id`
- `connection-29.t3`: Same protocol crate produces same `protocol_id` across builds
- `connection-29.t4`: Non-wire-relevant changes do not change `protocol_id`
- `connection-30.t1`: `protocol_id` is encoded as 16 bytes little-endian on wire
- `connection-31.t1`: Mismatched `protocol_id` causes `ProtocolMismatch` rejection
- `connection-31.t2`: Matched `protocol_id` allows connection to proceed
- `connection-31.t3`: `ProtocolMismatch` is distinguishable from other rejection reasons
- `connection-32.t1`: Each wire-relevant change produces different `protocol_id`
- `connection-33.t1`: Breaking protocol change causes `ProtocolMismatch`