# Spec: Entity Authority

Entity Authority defines how a client can acquire and release the right to **write replicated updates** for a
**server-owned delegated** entity, and what each side can observe about that right.

Authority is distinct from:
- **Ownership** (see `entity_ownership.md`): who ultimately owns the entity
- **Delegation** (see `entity_delegation.md`): how delegated entities arbitrate authority (first-request wins)
- **Scope** (see `entity_scopes.md`): whether the entity exists on the client
- **Replication** (see `entity_replication.md`): ordering/lifetime/reordering semantics

This spec defines:
- the authority state machine (`EntityAuthStatus`)
- client request/release semantics (including optimistic `Requested`)
- server-controlled authority (server as a holder; server override/reset)
- required behavior for illegal cases, duplicates, and out-of-scope conditions

---

## 1) Definitions

### Authority applies only to delegated entities
Authority exists only for entities where:
- `replication_config(E) == Some(Delegated)` (see `entity_delegation.md` / `entity_publication.md`)

### EntityAuthStatus
`EntityAuthStatus` is observable on the client via `client.entity(...).authority()` (and/or entity_mut variants)
and has the following values:

- `Available`: no client currently holds authority; eligible clients may request authority.
- `Requested`: this client has requested authority; request is pending server resolution.
- `Granted`: this client currently holds authority and may write replicated updates.
- `Releasing`: this client is releasing authority; writing may still be permitted until the release finalizes.
- `Denied`: authority is currently held by another client OR by the server, so this client cannot obtain it now.

Non-normative note:
- This aligns with the shared client-side permission split where `can_mutate()` may become true earlier than
  `can_write()`, but writing is only legal under `Granted` (and possibly `Releasing`).

### Diagnostics enabled
When diagnostics are enabled, Naia MAY emit warnings on illegal/bug states; otherwise it MUST remain silent
in production.

---

## 2) Core Contracts

### entity-authority-01 — Authority is defined only for delegated entities
For any entity `E`:
- If `replication_config(E) != Some(Delegated)`, then `authority(E)` MUST be `None` on clients (no authority state).
- Any attempt to request or release authority on a non-delegated entity MUST return an error (see below).

### entity-authority-02 — Single-writer rule (client-side)
For any delegated entity `E` and a given client `C`:
- `C` MUST only be permitted to **write** replicated updates for `E` when `EntityAuthStatus(C,E)` is:
    - `Granted`, or
    - `Releasing` (until release finalizes)

For all other statuses (`Available`, `Requested`, `Denied`):
- if Naia would enqueue/serialize/send a replication write for `E`, it MUST panic.

This is a hard invariant: Naia controls writing and must enforce it strictly.

### entity-authority-03 — Meaning of Denied
For a delegated entity `E` as observed by a client `C`:
- `Denied` MUST mean: authority is currently held by another client OR by the server.
- While `Denied`, the client MUST NOT be granted authority until the current holder releases or the server resets.
- When authority becomes available again, `Denied` MUST transition to `Available`.

This is not “you asked and were rejected”; it is “currently unavailable.”

---

## 3) Client API Semantics (Request / Release)

### entity-authority-04 — request_authority() is optimistic: Available → Requested immediately
If a client calls `request_authority(E)` for a delegated entity `E` and the client is eligible (in-scope, etc.):
- the client MUST transition locally from `Available` → `Requested` immediately (optimistic pending),
  without waiting for a server round-trip.

### entity-authority-05 — request_authority() completion transitions
After `Requested`, the client MUST eventually observe one of:

- `Requested → Granted` if the server grants authority
- `Requested → Denied` if authority is held by another client or by the server (first-request-wins arbitration)
- `Requested → Available` if the server resets authority globally (e.g., server override) before granting

The client MUST NOT remain permanently in `Requested` unless the entity/lifetime ends (see scope/lifetime rules).

### entity-authority-06 — release_authority() transitions: Granted → Releasing → Available
If the client currently holds authority:
- `release_authority(E)` MUST transition `Granted → Releasing` immediately (local optimistic),
- and MUST eventually finalize to `Available` after the server processes release.

If the client is `Requested` and calls `release_authority(E)`:
- it MUST cancel its request locally and transition to `Available`.
- the server MAY ignore the cancellation if it never observed the request; behavior must remain convergent.

### entity-authority-07 — Client-side error returns (Result semantics)
`request_authority(E)` and `release_authority(E)` MAY return errors. At minimum:

- If `replication_config(E) != Some(Delegated)`: MUST return an error (e.g., `ErrNotDelegated`)
- If the entity is out-of-scope on this client: MUST return an error (e.g., `ErrNotInScope`)
- If the entity does not exist in the client’s current lifetime: MUST return an error (e.g., `ErrNoSuchEntity`)

Errors MUST NOT mutate authority status except where explicitly specified (e.g., cancel Requested on release).

Non-normative note:
- Even with client-side errors, the server must still be safe if it receives invalid requests; see §6.

---

## 4) Server Semantics (Grant / Reset / Server as Holder)

### entity-authority-08 — First-request wins arbitration (delegation law)
Authority arbitration MUST follow the rules defined in `entity_delegation.md`:
- first eligible request wins
- others remain denied until release/reset

Authority spec defines the client-observable status transitions and events resulting from that law.

### entity-authority-09 — Server may hold authority and block clients
The server MAY act as an authority holder for a delegated entity.

If the server is holding authority for `E`:
- all clients observing `E` MUST be in `Denied` for `E` (except a client currently in `Requested`, which must
  transition to `Denied` once the server state is observed/applied).

### entity-authority-10 — Server override/reset
The server MAY reset authority for a delegated entity at any time.

When the server resets authority for `E`:
- any client in `Granted` or `Releasing` MUST transition to `Available` (authority revoked)
- any client in `Denied` MUST transition to `Available`
- any client in `Requested` MUST transition to `Available` (request cleared)

This is the server’s “break glass” control.

---

## 5) Scope, Lifetime, and Disconnect Interactions

### entity-authority-11 — Out-of-scope ends authority for that client
If a client becomes out-of-scope for delegated entity `E` (or the entity despawns due to publication/scope):
- the client MUST treat the entity’s lifetime as ended
- any authority status for that entity MUST be cleared (entity no longer exists locally)
- any pending buffered actions for that entity MUST be discarded (see `entity_replication.md`)

### entity-authority-12 — Authority holder losing scope forces global release/reset
If the authority-holding client loses scope for `E` (or disconnects):
- the server MUST release/reset authority for `E`
- other in-scope clients MUST transition from `Denied` to `Available`

(Exact timing is per replication tick semantics; clients must converge.)

### entity-authority-13 — Delegation disable clears authority
If an entity stops being delegated (`replication_config` changes away from `Delegated`):
- authority MUST become `None` on all clients for that entity
- any pending `Requested` MUST be cleared
- any `Granted/Releasing` MUST be revoked (server wins)

---

## 6) Illegal / Misuse Cases (Robustness)

### entity-authority-14 — Out-of-scope requests are ignored server-side
If the server receives an authority request for `(U,E)` while `OutOfScope(U,E)`:
- in production, it MUST ignore it silently
- when diagnostics are enabled, it MAY emit a warning

This complements client-side `ErrNotInScope`. The system must remain safe even if invalid requests occur.

### entity-authority-15 — Duplicate/late authority signals are idempotent
Authority grant/reset signals may be duplicated or reordered.

Clients MUST:
- not emit duplicate observable “grant” effects for the same lifetime
- converge to the server’s final resolved authority state
- ignore authority signals for entities not in the active lifetime (see `entity_replication.md`)

---

## 7) Observability (Events)

Authority changes MUST be observable via:
- `authority()` (status) while the entity is delegated and in the client’s lifetime
- client/server events as defined in `client_events_api.md` and `server_events_api.md`

This spec defines semantics, not exact event names. At minimum, the event layer MUST be able to represent:
- “authority granted to this client for entity E”
- “authority reset/revoked for entity E”

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

- Delegation: `entity_delegation.md`
- Ownership: `entity_ownership.md`
- Scopes & lifetimes: `entity_scopes.md`
- Replication ordering/lifetime gating: `entity_replication.md`
- Events: `server_events_api.md`, `client_events_api.md`, `world_integration.md`