# Entity Scopes

Entity Scopes define whether a given Entity `E` is **in-scope** or **out-of-scope** for a given User/Client `U`,
and the required observable consequences of scope transitions.

This spec defines:
- The **scope membership predicate** (Rooms + per-user include/exclude filters + required coupling).
- The **state machine** for `InScope(U,E)` / `OutOfScope(U,E)` and its client-visible effects.
- Deterministic **tick-level collapse** rules for scope changes.
- Required behavior under reordering / illegal states.

This spec does not define:
- Ownership write permissions (see `08_entity_ownership.spec.md`)
- Publication gating for client-owned entities (see `09_entity_publication.spec.md`)
- Delegation/authority semantics (see `10_entity_delegation.spec.md`, `11_entity_authority.spec.md`)
- Replication ordering/wire format (see `07_entity_replication.spec.md`)

---

## 1) Vocabulary

- **User U**: a server-identified remote client/user (keyed by `user_key`).
- **Entity E**: a networked entity tracked by Naia replication.
- **Room**: a server-managed grouping for coarse scope gating (users and entities may be members of multiple rooms).
- **SharesRoom(U,E)**: true iff `U` and `E` share at least one common room.
- **Include(U,E)**: per-user scope inclusion filter set via `server.user_scope_mut(user_key).include(entity)`.
- **Exclude(U,E)**: per-user scope exclusion filter set via `server.user_scope_mut(user_key).exclude(entity)`.

### Debug mode
- **Debug mode**: when `debug_assertions` are enabled (or equivalent feature flag), Naia MAY emit warnings for unusual but handled conditions.
  In production (default), Naia MUST remain silent. Per `00_common.spec.md`, tests MUST NOT assert on warning content.

---

## 2) Core Scope Predicate

### [entity-scopes-01] — Rooms are a required coarse gate for non-owners

**Obligations:**
- **t1**: Rooms are a required coarse gate for non-owners works correctly
For any user `U` and entity `E`, `SharesRoom(U,E)` MUST be a necessary precondition for `InScope(U,E)`, except where
other specs explicitly override (e.g. owning client always in-scope for its client-owned entities; see below).

If `SharesRoom(U,E) == false`, then `OutOfScope(U,E)` MUST hold.

### [entity-scopes-02] — Per-user include/exclude is an additional filter (additive after Rooms)

**Obligations:**
- **t1**: Per-user include/exclude is an additional filter (additive after Rooms) works correctly
Assuming `SharesRoom(U,E) == true`, the server MUST apply the per-user filter as follows:

- If `Exclude(U,E)` is active, then `OutOfScope(U,E)` MUST hold.
- Else if `Include(U,E)` is active, then `InScope(U,E)` MUST hold (subject to other gates like publication).
- Else (neither active), the default MUST be `InScope(U,E)` (subject to other gates like publication).

### [entity-scopes-03] — Include/Exclude ordering: last call wins

**Obligations:**
- **t1**: Include/Exclude ordering: last call wins works correctly
If both `Include(U,E)` and `Exclude(U,E)` are applied over time, the effective filter state MUST be determined by
the most recently applied call for that `(U,E)` pair (last call wins).

This rule is defined in terms of the server’s resolved mutation order (i.e. “last call” means last in the server’s
finalized application order for that tick).

### [entity-scopes-04] — Roomless entities are out-of-scope for all non-owners

**Obligations:**
- **t1**: Roomless entities are out-of-scope for all non-owners works correctly
If `E` is in zero rooms, then for all users `U` that are not explicitly forced in-scope by other specs,
`OutOfScope(U,E)` MUST hold, regardless of `Include(U,E)`.

(Include/exclude does not bypass the Rooms gate.)

---

## 3) Required Coupling to Ownership & Publication

### [entity-scopes-05] — Owning client is always in-scope for its client-owned entities

**Obligations:**
- **t1**: Owning client is always in-scope for its client-owned entities works correctly

For a client-owned entity `E` with owning client `A`:
- `InScope(A,E)` MUST always hold while `A` is connected.
- Publication and per-user scope filters MUST NOT remove `E` from `A`'s scope.
- Room membership changes MUST NOT remove `E` from `A`'s scope.
- `Exclude(A,E)` calls MUST be ignored for owner-owned entities (or return an error).

**This is an absolute invariant:** No scoping, publication, or room operation may hide an entity from its owner while the owner is connected.

(This restates the required coupling from `08_entity_ownership.spec.md` / `09_entity_publication.spec.md` as a scope invariant.)

**Observable signals:**
- Owning client never receives despawn for owned entity while connected

**Test obligations:**
- `entity-scopes-05.t1`: Owning client retains visibility of owned entity across all scope operations
- `entity-scopes-05.t2`: Exclude(owner, owned_entity) has no effect or returns error

### [entity-scopes-06] — Publication can force non-owners out-of-scope

**Obligations:**
- **t1**: Publication can force non-owners out-of-scope works correctly
For client-owned entities, publication state MUST be treated as an additional gate for non-owners:
- If client-owned `E` is Unpublished/Private, then for all `U != Owner(E)`, `OutOfScope(U,E)` MUST hold.

(See `09_entity_publication.spec.md` for publication semantics; this spec defines the scope consequence.)

---

## 4) Scope State Machine & Client-Visible Effects

For each pair `(U,E)` from the server’s perspective, the scope state is exactly one of:
- `InScope(U,E)`
- `OutOfScope(U,E)`

### [entity-scopes-07] — OutOfScope ⇒ despawn on that client

**Obligations:**
- **t1**: OutOfScope ⇒ despawn on that client works correctly
When a client corresponding to user `U` becomes `OutOfScope(U,E)`:
- `E` MUST be despawned on that client (removed from the client’s networked entity pool).

### [entity-scopes-08] — Despawn destroys all components, including local-only components

**Obligations:**
- **t1**: Despawn destroys all components, including local-only components works correctly
When `E` despawns on a client due to leaving scope:
- all components associated with `E` in that client’s networked entity pool MUST be destroyed,
  including any local-only components the client may have attached.

### [entity-scopes-09] — OutOfScope ⇒ ignore late replication updates for that entity

**Obligations:**
- **t1**: OutOfScope ⇒ ignore late replication updates for that entity works correctly
If a client receives replication updates for an entity `E` that is currently `OutOfScope` on that client:
- the client MUST ignore them silently in production.
- when Debug mode is enabled, the client MAY emit a warning.

This rule exists to make the protocol tolerant to packet reordering and racey delivery.

### [entity-scopes-10] — InScope ⇒ entity exists in networked entity pool

**Obligations:**
- **t1**: InScope ⇒ entity exists in networked entity pool works correctly
If a client is `InScope(U,E)`, then `E` MUST exist in that client’s networked entity pool (i.e. be present as a
replicated/spawned entity), subject to normal replication delivery and eventual consistency.

---

## 5) Tick Semantics & Collapse Rules

### [entity-scopes-11] — Scope is resolved per server tick; intermediate states are not observable

**Obligations:**
- **t1**: Scope is resolved per server tick; intermediate states are not observable works correctly
The server MUST resolve the final scope state for each `(U,E)` once per server tick and emit only the delta from
the prior tick’s resolved state.

If within one server tick operations would cause `InScope(U,E)` to flip multiple times (e.g. add/remove room membership,
include/exclude toggles), the server MUST collapse to the final resolved state and MUST NOT emit intermediate
spawn/despawn transitions.

### [entity-scopes-12] — Leaving scope for ≥1 tick creates a new lifetime on re-entry

**Obligations:**
- **t1**: Leaving scope for ≥1 tick creates a new lifetime on re-entry works correctly
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

**Obligations:**
- **t1**: Disconnect implies OutOfScope for that user for all entities works correctly
When a client disconnects (user `U` removed from the server connection set):
- `OutOfScope(U,E)` MUST be treated as holding for all entities `E` immediately.
- The server MUST cease replicating entities to that client.

Note: Separately, `08_entity_ownership.spec.md` defines that client-owned entities are globally despawned when their owning
client disconnects. This spec does not redefine that rule; it defines per-user scope state.

---

## 7) Illegal / Misuse Cases

These cases SHOULD NOT occur in correct usage, but behavior is defined for determinism and safety.

### [entity-scopes-14] — Include/exclude without shared room cannot force scope

**Obligations:**
- **t1**: Include/exclude without shared room cannot force scope works correctly
If `Include(U,E)` is active but `SharesRoom(U,E) == false`, then `OutOfScope(U,E)` MUST hold.

When Debug mode is enabled, the server MAY emit a warning indicating the include is ineffective due to room gating.

### [entity-scopes-15] — Unknown entity/user references

**Obligations:**
- **t1**: Unknown entity/user references works correctly
If the server receives (or internally attempts) a scope operation referencing an unknown entity or unknown user:
- in production, it MUST ignore the operation silently.
- when Debug mode is enabled, it MAY emit a warning.

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
- **entity-scopes-09**: Prove late updates for out-of-scope entities are ignored (warn only in Debug mode).
- **entity-scopes-11**: Prove same-tick flip-flops collapse to final state; no intermediate spawn/despawn.
- **entity-scopes-12**: Prove re-entry after ≥1 tick out-of-scope produces fresh spawn snapshot lifetime.
- **entity-scopes-13**: Prove disconnect implies OutOfScope for that user and replication ceases.

---

## 9) Cross-references

- Ownership: `08_entity_ownership.spec.md`
- Publication: `09_entity_publication.spec.md`
- Replication ordering/wire rules: `07_entity_replication.spec.md`
- Delegation/Authority coupling: `10_entity_delegation.spec.md`, `11_entity_authority.spec.md`
- Events/lifetimes: `12_server_events_api.spec.md`, `13_client_events_api.spec.md`, `14_world_integration.spec.md`