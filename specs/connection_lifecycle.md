# Connection Lifecycle Contract

This spec defines the only valid semantics for connection establishment, authentication gating, identity binding, handshake/token rules, disconnect/cleanup, rejects, and timeouts.

Normative keywords: **MUST**, **MUST NOT**, **SHOULD**, **MAY**.

---

## Scope

This spec covers:
- Client connection attempt → server accept/reject
- Authentication gating when `require_auth = true`
- Identity binding (including handshake identity tokens)
- Disconnect semantics (graceful, timeout, duplicate/loss-induced)
- Cleanup guarantees (no ghost users/entities/scope)
- Error surfaces (rejects, handshake mismatch, invalid tokens)

This spec does **not** define:
- Entity scope/rooms rules (see `specs/entity_scopes.md`)
- Entity replication ordering/identity mapping (see `specs/entity_replication.md`)
- Messaging channel semantics (see `specs/messaging.md`)
- Time/ticks internals beyond “timeout eventually happens” (see `specs/time_ticks_commands.md`)
- Transport fault model specifics (see `specs/transport.md`)

Related specs:
- `specs/entity_scopes.md`
- `specs/entity_replication.md`
- `specs/server_events_api.md`
- `specs/client_events_api.md`
- `specs/transport.md`
- `specs/observability_metrics.md`

---

## Definitions

- **Client**: a network participant attempting to connect to the server.
- **Server**: the authoritative host managing users, scope policy, and replication.
- **Connection Attempt**: the process beginning when a client initiates a connection and ending in Accepted or Rejected.
- **Connected**: server-visible state where the user is admitted and participates in scope/replication/messaging.
- **Rejected**: server-visible outcome where the attempt is denied; the client is not Connected.
- **Authentication Required**: configuration where the server requires an auth decision before admitting a user (`require_auth = true`).
- **Auth Decision**: server-side determination of Accept or Reject for a client’s auth payload.
- **Identity**: server-bound stable user identity for the lifetime of a connection.
- **Identity Token**: a server-issued opaque token that binds/authorizes identity at handshake (if enabled by the server’s chosen model).
- **Data-plane**: all replication/messages/requests that presume Connected state.
- **Control-plane**: handshake/auth/connection management messages that occur during Connection Attempt.
- **Cleanup**: removal of all server-side and client-side state associated with a connection so it cannot affect future sessions.

---

## Contracts

### connection-01 — Connection attempt resolves to exactly one terminal outcome
A Connection Attempt MUST resolve to exactly one terminal outcome for the client: **Accepted** or **Rejected**.

- Accepted implies the client becomes Connected.
- Rejected implies the client does not become Connected.

A client MUST NOT be both Connected and Rejected for the same attempt.

#### Test obligations
- **connection-01.t1** — Basic accept path produces Connected once  
  Given an empty server; When client A connects successfully; Then the server exposes exactly one connect indication for A and A is Connected.
- **connection-01.t2** — Reject path produces Reject once and never Connected  
  Given server configured to reject (capacity/handshake/auth); When A attempts to connect; Then the server exposes exactly one reject indication and no connect indication for A.

---

### connection-02 — Connect event ordering is stable
If clients A then B are accepted in that order (no interleaving rejects for those attempts), the server-visible connect ordering MUST be stable: A’s connect indication precedes B’s.

#### Test obligations
- **connection-02.t1**  
  Given a server; When A connects then B connects; Then exactly two connect indications appear in order [A, B] with no duplicates.

---

### connection-03 — Disconnect is idempotent and yields one effective cleanup
If a connection is terminated and later duplicate termination signals occur (duplicate disconnect, connection-lost, timeout, retries), the system MUST:
- Treat termination as idempotent, producing at most one effective disconnect indication per connection attempt, and
- Ensure Cleanup is complete and remains complete.

#### Test obligations
- **connection-03.t1**  
  Given A and B connected; When A disconnects and later a duplicate/loss-induced disconnect signal for A occurs; Then only one disconnect indication for A is exposed and A’s state is fully cleaned up.
- **connection-03.t2**  
  Given A disconnects; When time advances and any late packets arrive for A; Then they do not recreate user/entity/scope state.

---

### connection-04 — Cleanup removes all per-connection scope and entity residues
When a connection terminates (graceful or timeout), the system MUST remove all connection-associated state such that:
- The server no longer considers the user Connected.
- The user is removed from all room/scope membership and scope overrides.
- Any per-user scoped entities become OutOfScope for that user (server-side).
- Other clients MUST NOT observe “ghost” entities attributable to the disconnected user.

This spec does not prescribe *how* scope cleanup is implemented; only the observable outcomes are required.

#### Test obligations
- **connection-04.t1**  
  Given A and B connected and server has scoped entities; When A disconnects; Then B never observes entities incorrectly remaining in scope due to A’s stale membership.

---

### connection-05 — Authentication gating: no data-plane before auth acceptance
If Authentication Required is enabled (`require_auth = true`), then until the server reaches an Accept Auth Decision for the client:
- The client MUST NOT be considered Connected.
- The server MUST NOT expose a connect indication for that client.
- The client MUST receive no data-plane replication/messages/events that require Connected state.

#### Test obligations
- **connection-05.t1** — No replication before auth decision  
  Given `require_auth = true` and existing in-scope entities; When A connects and auth decision is delayed; Then A receives no replicated entities or data-plane events until auth is accepted.

---

### connection-06 — Auth accept: auth decision precedes connect admission
If `require_auth = true` and the server accepts a client’s auth payload, then:
- The server MUST expose exactly one auth indication for that client, and
- The server MUST expose a connect indication after the accept decision (not before).

#### Test obligations
- **connection-06.t1**  
  Given `require_auth = true` and auth handler accepts valid credentials; When A connects with valid auth; Then server exposes one auth indication then one connect indication for A, in that order.

---

### connection-07 — Auth reject: no connect and no data-plane
If `require_auth = true` and the server rejects a client’s auth payload, then:
- The server MUST expose an auth indication reflecting rejection.
- The server MUST NOT expose a connect indication for that client.
- The client MUST receive no data-plane replication/messages/events for that attempt.
- Cleanup MUST leave no half-connected state.

#### Test obligations
- **connection-07.t1**  
  Given `require_auth = true` and auth handler rejects credentials; When A connects with invalid auth; Then server exposes an auth indication but no connect indication, and A receives no replication.

---

### connection-08 — Auth disabled: connect occurs without auth requirement
If Authentication Required is disabled (`require_auth = false`), then:
- A successful connection MUST produce a connect indication.
- The server MUST NOT require an auth decision to admit the client.
- Whether an auth indication exists in this mode is implementation-defined; however, the system MUST NOT block admission on missing auth payload.

(If the implementation chooses to emit auth indications even when not required, that behavior MUST be documented as an extension. The core contract here is “no gating.”)

#### Test obligations
- **connection-08.t1**  
  Given `require_auth = false`; When A connects (with or without auth payload); Then A is admitted and can receive replication.

---

### connection-09 — Identity is bound at admission and MUST NOT silently swap mid-session
Once a connection attempt is Accepted and the user is Connected, the user’s Identity MUST be bound for the remainder of that connection. The system MUST NOT silently change Identity mid-session in response to additional auth payloads or identity-token-like messages.

If a mid-session identity-swap attempt occurs, the system MUST either:
- Ignore it, or
- Reject/terminate the connection with a clear disconnect/error outcome.

It MUST NOT result in a silent identity swap.

#### Test obligations
- **connection-09.t1**  
  Given A authenticated and connected; When A sends additional auth payload attempting to change identity; Then identity does not change; attempt is ignored or causes disconnect; no silent swap.

---

### connection-10 — Handshake/protocol mismatch fails before Connected state
If the client and server are incompatible at handshake/protocol level (version/schema/handshake mismatch), then:
- The attempt MUST fail before the client is considered Connected.
- The server MUST NOT create gameplay/data-plane state for that attempt.
- The client MUST receive a clear error/reject outcome (exact surface depends on transport; see `specs/transport.md`).

#### Test obligations
- **connection-10.t1**  
  Given server expects a specific handshake/protocol; When client connects with incompatible handshake/version; Then handshake fails, no connect indication is emitted, and state is cleaned up.

---

### connection-11 — Identity token validation is strict and failure is clean
If the server uses identity tokens in its handshake model, then:
- A malformed or tampered token MUST be rejected.
- An expired token (if expiration is part of the model) MUST be rejected.
- A reuse of a single-use token (if single-use is part of the model) MUST follow the documented rule (reject or forced-new-identity), and MUST NOT be silently accepted as fresh.

In all such failure cases:
- The client MUST NOT become Connected.
- The server MUST NOT emit a connect indication.
- Cleanup MUST leave no half-connected state.

#### Test obligations
- **connection-11.t1** — Malformed/tampered token rejected cleanly  
  Given server expects identity tokens; When client uses malformed/tampered token; Then handshake fails, no connect indication, and cleanup is complete.
- **connection-11.t2** — Expired/reused token obeys documented semantics  
  Given token has expiry/single-use rules; When client uses expired or already-used token; Then server enforces the documented outcome and does not silently accept it.

---

### connection-12 — Valid identity token round-trips without hidden state
If the server exposes an API to create identity tokens, and the server’s handshake model accepts those tokens, then:
- A token created by the server and presented by a client MUST be sufficient to connect according to the documented model.
- Successful admission MUST bind the connection’s Identity as documented.

#### Test obligations
- **connection-12.t1**  
  Given server generates a token via public API; When client uses it to connect; Then handshake succeeds and identity is bound as documented.

---

### connection-13 — Capacity-based reject produces reject outcome, not connect
If the server rejects a connection attempt due to capacity limits, then:
- The server MUST expose a reject indication (or equivalent reject surface).
- The server MUST NOT expose a connect indication for that attempt.
- Cleanup MUST be complete.

#### Test obligations
- **connection-13.t1**  
  Given server at max concurrent users; When another client tries to connect; Then reject is surfaced, no connect indication is emitted.

---

### connection-14 — Timeout/heartbeat-based disconnect eventually terminates cleanly
If heartbeat/timeout is configured and connectivity drops or traffic stops long enough, then:
- The connection MUST eventually terminate.
- The server MUST expose a disconnect indication (timeout/loss reason is implementation-defined but should be observable).
- Cleanup MUST be complete (see `connection-04`).

This contract does not define the exact duration or heartbeat algorithm; only eventual termination and cleanup.

#### Test obligations
- **connection-14.t1**  
  Given configured heartbeat/timeout; When traffic stops longer than timeout; Then both sides eventually observe disconnect and state is cleaned up.

---

### connection-15 — No replication/messages are delivered to a rejected or disconnected client
After a client is Rejected, or after a client is disconnected (for any reason), the system MUST NOT deliver further data-plane replication/messages/events to that client for that attempt.

Late-arriving packets MUST be ignored safely.

#### Test obligations
- **connection-15.t1**  
  Given A is rejected or disconnected; When time advances and late packets arrive; Then A receives no replication/messages/events and no state is resurrected.

---

## Notes for implementers

- This spec is intentionally transport-agnostic. Transport-specific error surfaces and timing expectations belong in `specs/transport.md`.
- Event naming in tests should match your harness’s conventions (e.g., ConnectEvent/RejectEvent/AuthEvent/DisconnectEvent). This spec cares about the semantics and ordering, not the type names.
