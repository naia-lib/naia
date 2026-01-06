# Naia Ownership & Delegation Contract (Iron-Clad)

This document defines the **only** valid semantics for entity ownership, publication, delegation, authority, scope, and state transitions.  
Normative keywords: **MUST**, **MUST NOT**, **MAY**, **SHALL**, **SHOULD**.

---

## 0) Glossary

- **Entity `E`**: a replicated object with a stable identity.
- **Owner(E)**: the single writer-of-record for a **non-delegated** entity: `Server` or `Client(A)`.
- **Delegated(E)**: a server-owned entity for which authority may be granted/denied per client.
- **Authority Holder**: for delegated entities only: `None`, `Server`, or `Client(A)`.
- **Scope**: `InScope(C,E)` means client `C` has `E` present in its local world. If `OutOfScope`, `E` is absent (despawned) on that client.
- **Published (client-owned only)**: server is permitted to scope `E` to non-owning clients.
- **Unpublished (client-owned only)**: server is forbidden to scope `E` to non-owning clients.

---

## 1) Server-Visible Canonical Entity States

At any instant, each entity is in exactly one of:

1. `ServerOwnedEntityUndelegated`
2. `ServerOwnedEntityDelegatedAuthNone`
3. `ServerOwnedEntityDelegatedAuthHeld(Server)`
4. `ServerOwnedEntityDelegatedAuthHeld(Client(A))`
5. `ClientOwnedEntityUnpublished(Owner=A)`
6. `ClientOwnedEntityPublished(Owner=A)`

Scope is orthogonal: for each client `C`, either `InScope(C,E)` or `OutOfScope(C,E)`.

---

## 2) Invariants (Always True)

### 2.1 Ownership / Delegation exclusivity
- If `E` is in any `ServerOwnedEntity*` state, then `Owner(E)=Server`.
- If `E` is in any `ClientOwnedEntity*` state, then `Owner(E)=Client(A)` for exactly one `A`.
- Delegation exists **only** for server-owned entities:
  - `Delegated(E)` iff `E` is in a `ServerOwnedEntityDelegated*` state.
- Undelegated entities have **no concept of authority**:
  - For `ServerOwnedEntityUndelegated` and all `ClientOwnedEntity*` states, authority status is undefined and MUST NOT be queried, stored, inferred, or surfaced.

### 2.2 Publication constraints
- Server-owned entities are inherently published:
  - `ServerOwnedEntity*` MUST be scoping-eligible for any client, subject to server scope policy.
- Client-owned entities:
  - If `ClientOwnedEntityUnpublished(Owner=A)`: for all clients `C != A`, `OutOfScope(C,E)` MUST hold.
  - If `ClientOwnedEntityPublished(Owner=A)`: server MAY place `E` into scope of clients `C != A` per normal scope policy.

### 2.3 Write acceptance
- For `ClientOwnedEntity*(Owner=A)`:
  - Server MUST accept writes from `A`.
  - Server MUST reject/ignore writes from any `C != A`.
- For `ServerOwnedEntityUndelegated`:
  - Server is authoritative; client writes MUST be rejected/ignored.
- For delegated server-owned entities, write acceptance is governed by Section 4.

### 2.4 Messaging guarantees
- Authority/status delivery uses a strict monotonic message-id/ack model:
  - Receivers MUST NOT observe out-of-order authority status application.
  - Duplicate deliveries MUST NOT occur; thus same-to-same authority status transitions MUST NOT be required for correctness.

---

## 3) Scope Laws

### 3.1 Visibility is scope
- A client MUST be able to act on an entity (including requesting authority) only if `InScope(C,E)`.
- If `OutOfScope(C,E)`, `E` MUST NOT exist in the client world.

### 3.2 Scope removal effects
- If a delegated entity has `Authority Holder = Client(A)` and `E` becomes `OutOfScope(A,E)`, then authority MUST be released (Section 4.6), yielding `DelegatedAuthNone`.

---

## 4) Delegation & Authority (Delegated Entities Only)

### 4.1 Delegated state shape
If `E` is delegated, it MUST be exactly one of:
- `DelegatedAuthNone`
- `DelegatedAuthHeld(Server)`
- `DelegatedAuthHeld(Client(A))`

### 4.2 Server broadcast rule (Denied fanout)
For any delegated entity `E`, for every client `C` such that `InScope(C,E)`:
- If `Authority Holder = Client(A)`:
  - Client `A` MUST observe `Granted`.
  - Every client `C != A` MUST observe `Denied`.
- If `Authority Holder = Server`:
  - Every client `C` MUST observe `Denied`.
- If `Authority Holder = None`:
  - Every client `C` MUST observe `Available`.

### 4.3 Client API (delegated entities only)
A client `A` MAY call:
- `request_authority(E)` only when:
  - `Delegated(E)` AND `InScope(A,E)` AND (A currently observes `Available`).
- `release_authority(E)` only when:
  - `Delegated(E)` AND `InScope(A,E)` AND (A currently holds authority; i.e., A observes `Granted`).

Client calls MUST return `Result`:
- `ErrNotDelegated` if `E` is not delegated.
- `ErrNotInScope` if `OutOfScope(A,E)` (this SHOULD be unreachable in normal client usage, but is still defined).
- `ErrNotAvailable` for `request_authority` when status is not `Available`.
- `ErrNotHolder` for `release_authority` when status is not `Granted`.

### 4.4 Server API (delegated entities only)
The server MAY call:
- `give_authority(A, E)`:
  - Preconditions: `Delegated(E)` AND `InScope(A,E)`.
  - Effect: `Authority Holder := Client(A)` (server has priority; overwrites any prior holder).
- `take_authority(E)`:
  - Preconditions: `Delegated(E)`.
  - Effect: `Authority Holder := Server` (server has priority; overwrites any prior holder).
- `release_authority(E)`:
  - Preconditions: `Delegated(E)`.
  - Effect: `Authority Holder := None`.

Server calls MUST NOT panic on non-delegated entities; they MUST return `ErrNotDelegated`.

### 4.5 Delegated write acceptance
For delegated `E`:
- If `Authority Holder = Client(A)`:
  - Server MUST accept writes from `A`.
  - Server MUST reject/ignore writes from any `C != A`.
- If `Authority Holder = Server` or `None`:
  - Server MUST reject/ignore all client writes.

### 4.6 Authority release semantics
Authority MUST be released (i.e., `Authority Holder := None`) when any of the following occur:
- Holder client calls `release_authority(E)` successfully.
- Server calls `release_authority(E)`.
- Server calls `give_authority(B,E)` or `take_authority(E)` (overwrites prior holder).
- Holder client becomes `OutOfScope(holder,E)` (Section 3.2).
- Holder client disconnects.

When authority is released from a state where non-holders observed `Denied`, they MUST observe:
- `Denied → Available` (explicitly indicating authority is now available).

### 4.7 Authority status emissions (events)
Authority-related events exist **only** for delegated entities.
Define events per client `C` for delegated `E`:
- `AuthGranted(E)`: emitted iff C transitions to observing `Granted`.
- `AuthDenied(E)`: emitted iff C transitions to observing `Denied` **from a non-Denied state**.
- `AuthLost(E)`: emitted iff C transitions from observing `Granted` to observing anything else (`Available` or `Denied`).

Events MUST be emitted exactly once per actual status transition and MUST NOT be emitted for redundant no-op updates.

---

## 5) Delegation Enable/Disable

### 5.1 Enable delegation (server-owned)
Only the server MAY enable delegation on a server-owned undelegated entity:
- Transition: `ServerOwnedEntityUndelegated → ServerOwnedEntityDelegatedAuthNone`
- Postcondition: All `InScope(C,E)` clients MUST observe `Available`.

### 5.2 Disable delegation (server-only)
Only the server MAY disable delegation on a delegated entity:
- Transition: `ServerOwnedEntityDelegated* → ServerOwnedEntityUndelegated`
- Postcondition:
  - Authority ceases to exist; clients MUST NOT retain any authority status for `E`.
  - Client-side MUST treat `E` as non-delegated immediately after the transition.

Disable delegation MUST implicitly clear any prior holder.

---

## 6) Client-Owned Publication (Automatic, server or owner initiated)

Client-owned entities MAY transition:
- `ClientOwnedEntityUnpublished(Owner=A) ↔ ClientOwnedEntityPublished(Owner=A)`

Rules:
- Only the server OR the owning client MAY cause this transition.
- This transition is **automatic** (system-driven); it is not required to be a public API.
- When unpublished, the server MUST enforce `OutOfScope(C,E)` for all `C != A`.
- When published, the server MAY scope `E` to others per policy.

---

## 7) Client-Owned Delegation (Migration)

### 7.1 Preconditions
A client-owned entity MUST be published before being delegated:
- `ClientOwnedEntityPublished(Owner=A)` is required.
- Attempting to delegate an unpublished client-owned entity MUST fail with `ErrNotPublished`.

### 7.2 Initiators
Delegation of a client-owned entity MAY be initiated by:
- The server, OR
- The owning client `A`.

### 7.3 Migration effect (client-owned → server-owned delegated)
On delegation of `ClientOwnedEntityPublished(Owner=A)`:
- Entity identity MUST remain continuous (no despawn+spawn; same logical `E`).
- State transition:
  - `ClientOwnedEntityPublished(Owner=A) → ServerOwnedEntityDelegatedAuthHeld(Client(A))`
  - EXCEPT: if at migration time `OutOfScope(A,E)` holds, then:
    - `ClientOwnedEntityPublished(Owner=A) → ServerOwnedEntityDelegatedAuthNone`
- After migration, scope membership is controlled solely by server policy:
  - `E` remains in scope for a client iff the server deems `InScope(client,E)`; the contract does not guarantee preservation beyond that.

---

## 8) Ordering / Atomicity (Server Tick Law)

The server MUST compute state changes atomically per tick:
1) Apply scope membership changes.
2) Apply delegation enable/disable and client-owned migration.
3) Apply authority actions (`give_authority`, `take_authority`, `release_authority`, client requests/releases).
4) Emit resulting authority statuses to all `InScope` clients per Section 4.2, and emit events per Section 4.7.

If multiple authority actions apply in the same tick, server actions have priority over client actions:
- `take_authority` and `give_authority` override any client request/release in that tick.

---

## 9) Forbidden Behaviors

- Emitting or checking authority status for any non-delegated entity.
- Allowing any client to write to a server-owned undelegated entity.
- Allowing any non-owner client to ever see an unpublished client-owned entity.
- Delegating a client-owned entity while unpublished.
- When `Authority Holder = Server`, allowing any client to observe `Granted` or `Available`.
- Relying on same-to-same authority status transitions (e.g., `Denied→Denied`) for correctness.

---
