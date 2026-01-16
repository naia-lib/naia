# Entity Authority

Entity Authority defines how a client can acquire and release the right to **write replicated updates** for a
**server-owned delegated** entity, and what each side can observe about that right.

Authority is distinct from:
- **Ownership** (see `08_entity_ownership.spec.md`): who ultimately owns the entity
- **Delegation** (see `10_entity_delegation.spec.md`): how delegated entities arbitrate authority (first-request wins)
- **Scope** (see `06_entity_scopes.spec.md`): whether the entity exists on the client
- **Replication** (see `07_entity_replication.spec.md`): ordering/lifetime/reordering semantics

This spec defines:
- the authority state machine (`EntityAuthStatus`)
- client request/release semantics (including optimistic `Requested`)
- server-controlled authority (server as a holder; server override/reset)
- required behavior for illegal cases, duplicates, and out-of-scope conditions

---

## 1) Definitions

### Authority applies only to delegated entities
Authority exists only for entities where:
- `replication_config(E) == Some(Delegated)` (see `10_entity_delegation.spec.md` / `09_entity_publication.spec.md`)

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

### Debug mode
In Debug mode (`debug_assertions` enabled), Naia MAY emit warnings on unusual but handled conditions; in production it MUST remain silent. Per `00_common.spec.md`, tests MUST NOT assert on warning content.

---

## 2) Core Contracts

### [entity-authority-01] — Authority is defined only for delegated entities

**Obligations:**
- **t1**: Authority is defined only for delegated entities.
For any entity `E`:
- If `replication_config(E) != Some(Delegated)`, then `authority(E)` MUST be `None` on clients (no authority state).
- Any attempt to request or release authority on a non-delegated entity MUST return an error (see below).

### [entity-authority-02] — Single-writer rule (client-side)

**Obligations:**
- **t1**: Single-writer rule (client-side).
For any delegated entity `E` and a given client `C`:
- `C` MUST only be permitted to **write** replicated updates for `E` when `EntityAuthStatus(C,E)` is:
    - `Granted`, or
    - `Releasing` (until release finalizes)

For all other statuses (`Available`, `Requested`, `Denied`):
- if Naia would enqueue/serialize/send a replication write for `E`, it MUST panic.

This is a hard invariant: Naia controls writing and must enforce it strictly.

### [entity-authority-03] — Meaning of Denied

**Obligations:**
- **t1**: Meaning of Denied.
For a delegated entity `E` as observed by a client `C`:
- `Denied` MUST mean: authority is currently held by another client OR by the server.
- While `Denied`, the client MUST NOT be granted authority until the current holder releases or the server resets.
- When authority becomes available again, `Denied` MUST transition to `Available`.

This is not “you asked and were rejected”; it is “currently unavailable.”

---

## 3) Client API Semantics (Request / Release)

### [entity-authority-04] — request_authority() is optimistic: Available → Requested immediately

**Obligations:**
- **t1**: request_authority() is optimistic: Available → Requested immediately.
If a client calls `request_authority(E)` for a delegated entity `E` and the client is eligible (in-scope, etc.):
- the client MUST transition locally from `Available` → `Requested` immediately (optimistic pending),
  without waiting for a server round-trip.

### Request resolution

- Calling `request_authority()` MUST immediately set local status to `Requested` (optimistic pending).
- The server MUST resolve the request:
  - If authority is `Available`, the first request received wins and becomes `Granted`.
  - If authority is held by someone else (client or server), the requester MUST become `Denied` (no queue).

### [entity-authority-05] — request_authority() completion transitions

**Obligations:**
- **t1**: request_authority() completion transitions.
After `Requested`, the client MUST eventually observe one of:

- `Requested → Granted` if the server grants authority
- `Requested → Denied` if authority is held by another client or by the server (first-request-wins arbitration)
- `Requested → Available` if the server resets authority globally (e.g., server override) before granting

The client MUST NOT remain permanently in `Requested` unless the entity/lifetime ends (see scope/lifetime rules).

### [entity-authority-06] — release_authority() transitions: Granted → Releasing → Available

**Obligations:**
- **t1**: release_authority() transitions: Granted → Releasing → Available.
If the client currently holds authority:
- `release_authority(E)` MUST transition `Granted → Releasing` immediately (local optimistic),
- and MUST eventually finalize to `Available` after the server processes release.

If the client is `Requested` and calls `release_authority(E)`:
- it MUST cancel its request locally and transition to `Available`.
- the server MAY ignore the cancellation if it never observed the request; behavior must remain convergent.

### [entity-authority-07] — Client-side error returns (Result semantics)

**Obligations:**
- **t1**: Client-side error returns (Result semantics).
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

**Obligations:**
- **t1**: First-request wins arbitration (delegation law).
Authority arbitration MUST follow the rules defined in `10_entity_delegation.spec.md`:
- first eligible request wins
- others remain denied until release/reset

Authority spec defines the client-observable status transitions and events resulting from that law.

### [entity-authority-09] — Server may hold authority and block clients

**Obligations:**
- **t1**: Server may hold authority and block clients.
The server MAY act as an authority holder for a delegated entity.

If the server is holding authority for `E`:
- all clients observing `E` MUST be in `Denied` for `E` (except a client currently in `Requested`, which must
  transition to `Denied` once the server state is observed/applied).

### [entity-authority-10] — Server override/reset

**Obligations:**
- **t1**: Server override/reset.
The server MAY reset authority for a delegated entity at any time.

When the server resets authority for `E`:
- any client in `Granted` or `Releasing` MUST transition to `Available` (authority revoked)
- any client in `Denied` MUST transition to `Available`
- any client in `Requested` MUST transition to `Available` (request cleared)

This is the server’s “break glass” control.

---

## 5) Scope, Lifetime, and Disconnect Interactions

### [entity-authority-11] — Out-of-scope ends authority for that client

**Obligations:**
- **t1**: Out-of-scope ends authority for that client.
If a client becomes out-of-scope for delegated entity `E` (or the entity despawns due to publication/scope):
- the client MUST treat the entity’s lifetime as ended
- any authority status for that entity MUST be cleared (entity no longer exists locally)
- any pending buffered actions for that entity MUST be discarded (see `07_entity_replication.spec.md`)

### [entity-authority-12] — Authority holder losing scope forces global release/reset

**Obligations:**
- **t1**: Authority holder losing scope forces global release/reset.
If the authority-holding client loses scope for `E` (or disconnects):
- the server MUST release/reset authority for `E`
- other in-scope clients MUST transition from `Denied` to `Available`

(Exact timing is per replication tick semantics; clients must converge.)

### [entity-authority-13] — Delegation disable clears authority

**Obligations:**
- **t1**: Delegation disable clears authority.
If an entity stops being delegated (`replication_config` changes away from `Delegated`):
- authority MUST become `None` on all clients for that entity
- any pending `Requested` MUST be cleared
- any `Granted/Releasing` MUST be revoked (server wins)

---

## 6) Illegal / Misuse Cases (Robustness)

### [entity-authority-14] — Out-of-scope requests are ignored server-side

**Obligations:**
- **t1**: Out-of-scope requests are ignored server-side.
If the server receives an authority request for `(U,E)` while `OutOfScope(U,E)`:
- in production, it MUST ignore it silently
- when Debug mode are enabled, it MAY emit a warning

This complements client-side `ErrNotInScope`. The system must remain safe even if invalid requests occur.

### [entity-authority-15] — Duplicate/late authority signals are idempotent

**Obligations:**
- **t1**: Duplicate/late authority signals are idempotent.
Authority grant/reset signals may be duplicated or reordered.

Clients MUST:
- not emit duplicate observable “grant” effects for the same lifetime
- converge to the server’s final resolved authority state
- ignore authority signals for entities not in the active lifetime (see `07_entity_replication.spec.md`)

---

## 7) Observability (Events)

### [entity-authority-16] — Authority observability

**Obligations:**
- **t1**: Authority observability.

Authority changes MUST be observable via:
- `authority()` (status) while the entity is delegated and in the client’s lifetime
- client/server events as defined in `13_client_events_api.spec.md` and `12_server_events_api.spec.md`

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
- entity-authority-14: out-of-scope requests ignored server-side (warn only in Debug mode)
- entity-authority-15: duplicates/reordering are idempotent and lifetime-safe

---

## 9) Cross-references

- Delegation: `10_entity_delegation.spec.md`
- Ownership: `08_entity_ownership.spec.md`
- Scopes & lifetimes: `06_entity_scopes.spec.md`
- Replication ordering/lifetime gating: `07_entity_replication.spec.md`
- Events: `12_server_events_api.spec.md`, `13_client_events_api.spec.md`, `14_world_integration.spec.md`