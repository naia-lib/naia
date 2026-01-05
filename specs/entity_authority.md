# Spec: Entity Authority (Delegated Entities)
Defines the only valid semantics for authority holder, client/server authority operations, status fanout, and events.

**Status:** Active  
**Version:** v1  
**Related specs:** `entity_delegation.md`, `entity_scopes.md`, `entity_replication.md`, `entity_ownership.md`  
**Applies to:** server + client

**Contract ID format:** `entity-authority-<nn>` (stable; never reused; never renumbered)

---

## 1) Scope & Vocabulary

**In scope**
- Authority holder states for delegated entities only.
- Status visibility (Granted/Denied/Available) and fanout.
- Client/server authority APIs and error semantics.
- Authority release semantics and event emission rules.

**Out of scope**
- Delegation enable/disable and migration (see `entity_delegation.md`).
- Delivery guarantees (see `entity_replication.md`).

**Vocabulary**
- **Delegated(E)**: defined in `entity_delegation.md`.
- **Authority Holder** (delegated only): `None`, `Server`, or `Client(A)`.
- **Status observed by client**: `Granted`, `Denied`, or `Available`.

---

## 2) Contract (Rules)

### entity-authority-01 — Authority exists only for delegated entities
Authority status/events MUST exist only for delegated entities.
For non-delegated entities, authority MUST NOT be queried, stored, inferred, or surfaced.

### entity-authority-02 — Authority holder state is exactly one of three
For delegated `E`, Authority Holder MUST be exactly one of:
- `None`, `Server`, `Client(A)`.

### entity-authority-03 — Status fanout (Denied broadcast rule)
For each client `C` with `InScope(C,E)`:
- If holder is `Client(A)`: A observes `Granted`, all `C != A` observe `Denied`.
- If holder is `Server`: all observe `Denied`.
- If holder is `None`: all observe `Available`.

### entity-authority-04 — Client request/release operations and errors
A client `A` MAY call:
- `request_authority(E)` only if delegated, in-scope, and A observes `Available`.
- `release_authority(E)` only if delegated, in-scope, and A observes `Granted`.

Calls MUST return `Result` errors:
- `ErrNotDelegated` if not delegated.
- `ErrNotInScope` if out-of-scope (defined even if “should be unreachable”).
- `ErrNotAvailable` if request when not `Available`.
- `ErrNotHolder` if release when not `Granted`.

### entity-authority-05 — Server give/take/release operations and errors
Server MAY call:
- `give_authority(A,E)` only if delegated and `InScope(A,E)`.
- `take_authority(E)` only if delegated.
- `release_authority(E)` only if delegated.

On non-delegated entities, server authority calls MUST return `ErrNotDelegated` and MUST NOT panic.

### entity-authority-06 — Server priority in conflicts
If multiple authority actions apply in the same tick, server actions have priority over client actions:
- `take_authority` and `give_authority` override client request/release in that tick.

### entity-authority-07 — Delegated write acceptance
For delegated `E`:
- If holder is `Client(A)`: server MUST accept writes from `A`, reject/ignore from others.
- If holder is `Server` or `None`: server MUST reject/ignore all client writes.

### entity-authority-08 — Authority release conditions
Authority MUST be released (holder becomes `None`) when any occur:
- holder client successfully calls `release_authority(E)`;
- server calls `release_authority(E)`;
- server calls `give_authority` or `take_authority` (overwrites prior holder);
- holder client becomes out-of-scope for `E`;
- holder client disconnects.

### entity-authority-09 — Denied → Available must be observable on release
When authority is released from a state where non-holders observed `Denied`,
those clients MUST observe `Denied → Available` (explicitly signaling “up for grabs”).

### entity-authority-10 — Events exist only for delegated entities and are transition-driven
For delegated `E` and client `C`:
- `AuthGranted(E)` iff C transitions to observing `Granted`.
- `AuthDenied(E)` iff C transitions to observing `Denied` from a non-Denied state.
- `AuthLost(E)` iff C transitions from `Granted` to anything else (`Available` or `Denied`).

Events MUST be emitted exactly once per actual status transition and MUST NOT be emitted for redundant no-op updates.

### entity-authority-11 — Same-to-same status transitions are forbidden as a correctness requirement
Implementations MUST NOT rely on same-to-same status transitions (e.g., `Denied → Denied`) for correctness.

---

## 3) Contract IDs (Obligations)
(These map to the authority files: client ops, server ops, scope coupling, migration/events.)

---

## 4) Interfaces & Observability
- All in-scope clients observe exactly one of {Granted, Denied, Available}.
- If server holds authority, all clients observe Denied.

---

## 5) Invariants & Non-Goals

**Always true**
- Authority is an entity-level delegated concept (not per-component).

**Non-goals**
- Does not define transport delivery mechanics (see `entity_replication.md`).

---

## 6) Changelog
- v1: extracted from prior omnibus contract.
