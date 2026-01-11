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

> connection-01 (MUST): Client behavior MUST be describable by the above conceptual states, even if the implementation uses different internal states.

> connection-02 (MUST NOT): The client MUST NOT expose a public “Rejected” connection state. Rejection is an event (RejectEvent), not a persistent state.

### Server states (per-client-session conceptual)

- **NoSession**
- **Handshaking**
- **Connected**

> connection-03 (MUST): The server MUST NOT treat a client as “Connected” (for purposes of entity replication, message delivery, tick semantics, etc.) until the handshake is finalized including tick sync.

---

## Authentication & identity tokens

### `require_auth = false`

> connection-04 (MUST): If `require_auth = false`, the server MUST allow clients to attempt connection without any pre-auth step.

> connection-05 (MAY): Implementations MAY still support optional application-level auth, but it must not be required by Naia for connection establishment when `require_auth = false`.

### `require_auth = true`

This mode uses an out-of-band HTTP auth step and a one-time identity token.

#### Pre-transport auth request (HTTP)

> connection-06 (MUST): When `require_auth = true`, a client MUST obtain a server-issued identity token via an out-of-band auth request (HTTP) BEFORE initializing the transport connection attempt.

> connection-07 (MUST): The server MUST evaluate the auth request and return either:
- `200 OK` (accepted) with an identity token, or
- `401 Unauthorized` (rejected) with no identity token.

> connection-08 (MUST): When the server receives an auth request in this mode, it MUST emit exactly one `AuthEvent` for that request.

> connection-09 (MUST): There is no Naia-level “auth timeout” during the transport handshake, because auth is completed before the transport session begins.

#### Identity token properties

> connection-10 (MUST): An identity token MUST be:
- **One-time use** (cannot be used successfully more than once), and
- **Time-limited** with TTL = **1 hour** from issuance.

> connection-11 (MUST): If a token is expired, already-used, or invalid, the server MUST explicitly reject the connection attempt (see “Explicit rejection”).

> connection-12 (MUST): Identity tokens MUST be required for **all transports** when `require_auth = true` (not only WebRTC).

> connection-13 (MUST): On first successful validation attempt, the server MUST mark the token as used (consumed). Replays MUST fail.

---

## Transport handshake & tick sync

> connection-14 (MUST): A successful connection handshake MUST include a tick synchronization step. A client MUST NOT be considered “Connected” until tick sync completes.

> connection-15 (MUST): The client MUST emit `ConnectEvent` only at the moment the handshake is finalized (including tick sync).

> connection-16 (MUST): The server MUST emit `ConnectEvent` only at the moment the handshake is finalized (including tick sync).

> connection-17 (MUST): Naia MUST NOT deliver any entity replication “writes” as part of an established session until after `ConnectEvent` is emitted for that session (server-side), and the client MUST NOT apply any such writes until after it has emitted `ConnectEvent`.

(See `5_time_ticks_commands.md` for tick semantics and how tick sync interacts with command history.)

---

## Explicit rejection

> connection-18 (MUST): The server MUST explicitly reject a connection attempt when:
- `require_auth = true` and the client presents no identity token,
- the presented token is invalid/expired/already-used,
- the server otherwise chooses to deny the attempt before session establishment.

> connection-19 (MUST): When the server explicitly rejects:
- The client MUST emit `RejectEvent`.
- The client MUST NOT emit `ConnectEvent`.
- The client MUST NOT emit `DisconnectEvent` (because it was never connected).

> connection-20 (MUST): After a `RejectEvent`, the client’s public `ConnectionStatus` MUST be (or return to) a non-connected state (e.g. Disconnected), with no special “Rejected” status.

---

## Disconnect semantics

> connection-21 (MUST): `DisconnectEvent` (client-side) MUST only be emitted if the client previously emitted `ConnectEvent` for the session.

> connection-22 (MUST): `DisconnectEvent` (server-side) MUST only be emitted if the server previously emitted `ConnectEvent` for the session.

> connection-23 (MUST): When a client disconnects (or is disconnected) after session establishment:
- It is treated as immediately out-of-scope for all entities, and
- Any client-owned entities owned by that client MUST be despawned by the server.
(See `9_entity_ownership.md` and `7_entity_scopes.md`.)

---

## Event ordering guarantees

### Successful session (require_auth = true)

> connection-24 (MUST): For a single successful connection where `require_auth = true`, the server MUST observe events in this order:
1. `AuthEvent`
2. `ConnectEvent`
3. `DisconnectEvent` (eventually)

### Successful session (require_auth = false)

> connection-25 (MUST): For a single successful connection where `require_auth = false`, the server MUST observe:
1. `ConnectEvent`
2. `DisconnectEvent` (eventually)

### Client-side ordering (all modes)

> connection-26 (MUST): For a single successful session, the client MUST observe:
1. `ConnectEvent`
2. `DisconnectEvent` (eventually)

> connection-27 (MUST): For a rejected attempt, the client MUST observe:
1. `RejectEvent`
…and MUST NOT observe `ConnectEvent` or `DisconnectEvent` for that attempt.

---

## Non-goals / Out of scope

- The exact HTTP route(s), headers, or body format of the auth request.
- Transport-specific wire details for how the token is conveyed.
- Engine adapter (bevy/hecs) implementation details.
- Retry/backoff policies for repeated connection attempts (may be defined in a future spec if needed).
